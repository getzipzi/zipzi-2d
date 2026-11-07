"""Installed pure client; real subprocess races and host-owned endpoint locks."""
import importlib.util
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch
import zipzi
from zipzi import values as v


sys.path.insert(0, str(Path(__file__).resolve().parent))
from connection_support import connect, stop


class InstalledConnection(unittest.TestCase):
    def setUp(self):
        self.assertIn("/site-packages/zipzi/", Path(zipzi.__file__).as_posix())
        self.assertIsNone(importlib.util.find_spec("zipzi_native"))
        self.directory = tempfile.TemporaryDirectory(prefix="zzpyc-", dir=None if os.name == "nt" else "/tmp")
        self.addCleanup(self.directory.cleanup)
        self.folder = Path(self.directory.name)
        self.endpoint = self.folder / "test.sock"
        self.log = self.folder / "pids"
        self.host = Path(os.environ["ZIPZI_KERNEL_HOST"])
        self.environment = patch.dict(os.environ, ZIPZI_TEST_PID_LOG=str(self.log))
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.addCleanup(self.stop_hosts)

    def stop_hosts(self):
        for pid in self.pids():
            stop(pid)

    def pids(self):
        return [int(line) for line in self.log.read_text().splitlines()] if self.log.exists() else []

    def connect(self, **options):
        return connect(endpoint=self.endpoint, start=True, host=self.host, **options)

    def test_two_process_cold_start_shares_one_unsaved_session_and_history(self):
        script = """
import json,sys,time,zipzi
from pathlib import Path
from zipzi import values as v
while not Path(sys.argv[4]).exists(): time.sleep(.005)
with connect(endpoint=Path(sys.argv[1]),start=True,host=Path(sys.argv[2])) as r:
    parameter='79200000-0000-4000-8000-000000000002'
    if sys.argv[3]=='0':
        s=r.sessions.create(document_id='79200000-0000-4000-8000-000000000001')
        with s.draft('first client') as e:
            e.apply(v.CommandCreateParameter(spec=v.ParameterCreateSpec(
                id=parameter,name='First',minimum=-1.,maximum=1.,default_value=0.)))
    else:
        end=time.monotonic()+10
        while True:
            sessions=r.sessions.list()
            if sessions and sessions[0]['version']['revision']==1: break
            assert time.monotonic()<end
            time.sleep(.01)
        s=r.sessions.attach(sessions[0]['session'])
        with s.draft('second client') as e:
            e.apply(v.CommandPatchParameter(id=parameter,patch=v.ParameterPatch(name='Second')))
    print(json.dumps(dict(runtime=r.connection['runtime'],pid=r.connection['host_pid'],session=s.id)))
"""
        script = "import runpy; connect = runpy.run_path(" + repr(str(Path(__file__).resolve())) + ")['connect']\n" + script
        gate = self.folder / "go"
        children = [subprocess.Popen([sys.executable, "-I", "-c", script, str(self.endpoint), str(self.host), str(index), str(gate)],
            cwd=tempfile.gettempdir(), stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) for index in range(2)]
        def cleanup():
            for child in children:
                if child.poll() is None:
                    child.kill()
                child.wait(timeout=5)
        self.addCleanup(cleanup)
        gate.touch()
        outputs = []
        for child in children:
            stdout, stderr = child.communicate(timeout=20)
            self.assertEqual(child.returncode, 0, stderr)
            outputs.append(json.loads(stdout))
        self.assertEqual(outputs[0], outputs[1])
        self.assertEqual(self.pids(), [outputs[0]["pid"]])
        with connect(endpoint=self.endpoint) as runtime:
            info, = runtime.sessions.list()
            self.assertIsNone(info['path'])
            session = runtime.sessions.attach(info['session'])
            self.assertEqual(session.id, outputs[0]['session'])
            self.assertEqual(session.info()['history']['undo_steps'], 2)
            parameter='79200000-0000-4000-8000-000000000002'
            query=v.QueryObjects(addresses=(v.ObjectAddress(kind=v.ObjectKind.PARAMETER,id=parameter),))
            self.assertEqual(session.query(query).value['records'][0]['value']['name'], 'Second')
            session.undo()
            self.assertEqual(session.query(query).value['records'][0]['value']['name'], 'First')

    def test_timeout_preserves_slow_host_ownership_and_next_attempt_reuses_it(self):
        with patch.dict(os.environ, ZIPZI_TEST_START_DELAY_MS="500"):
            for _ in range(2):
                with self.assertRaises(TimeoutError):
                    self.connect(startup_timeout=.08)
            with self.connect(startup_timeout=3) as runtime:
                self.assertIsNotNone(runtime.connection)
                self.assertEqual(len(self.pids()), 1)

    def test_failed_start_releases_lock_for_later_retry(self):
        with patch.dict(os.environ, ZIPZI_TEST_FAIL_START="1"):
            with self.assertRaises(zipzi.SdkFailure) as error:
                self.connect()
            self.assertEqual(error.exception.code, "HOST_START_FAILED")
        with self.connect() as runtime:
            self.assertEqual(len(self.pids()), 2)
            self.assertEqual(runtime.sessions.list(), ())

    def test_protocol_refusal_keeps_existing_host(self):
        with self.connect() as first:
            with patch('zipzi.backends.connected.CONTRACT', 'deliberately-incompatible'):
                with self.assertRaises(zipzi.SdkFailure) as failure:
                    self.connect()
            self.assertEqual(failure.exception.code, 'PROTOCOL_MISMATCH')
            with self.connect() as second:
                self.assertEqual(first.connection['runtime'], second.connection['runtime'])
            self.assertEqual(len(self.pids()), 1)

    def test_same_protocol_allows_different_capabilities_and_extended_metadata(self):
        from zipzi.backends import connected

        exchange = connected._exchange
        for capabilities in ([], ["future_optional_service"]):
            with self.subTest(capabilities=capabilities):
                def extended_host(stream, body):
                    response = exchange(stream, body)
                    if "operation" not in json.loads(body):
                        response["ok"]["capabilities"] = capabilities
                        response["ok"]["sdk_build"] = "99.0+different-sources"
                        response["ok"]["host_version"] = "99.0"
                        response["ok"]["future_metadata"] = {"enabled": True}
                    return response

                with patch.object(connected, "_exchange", side_effect=extended_host):
                    with self.connect() as runtime:
                        self.assertEqual(runtime.connection['protocol'], "zipzi-runtime-v4")
                        self.assertEqual(runtime.connection['sdk_build'], "99.0+different-sources")
                        self.assertEqual(runtime.connection['capabilities'], tuple(capabilities))
                        session = runtime.sessions.create()
                        self.assertIn(session.id, [info['session'] for info in runtime.sessions.list()])
                        self.assertEqual(session.info()["session"], session.id)
                        session.close(discard_modified=True)
        self.assertEqual(len(self.pids()), 1)

    def test_incompatible_host_response_is_rejected(self):
        from zipzi.backends import connected

        exchange = connected._exchange
        def incompatible_host(stream, body):
            response = exchange(stream, body)
            response["ok"]["protocol"] = "zipzi-runtime-v2"
            return response

        with patch.object(connected, "_exchange", side_effect=incompatible_host):
            with self.assertRaises(zipzi.SdkFailure) as failure:
                self.connect()
        self.assertEqual(failure.exception.code, "PROTOCOL_MISMATCH")
        with self.connect() as runtime:
            self.assertEqual(runtime.sessions.list(), ())
        self.assertEqual(len(self.pids()), 1)

    def test_launcher_exit_during_startup_preserves_host(self):
        script = ('import runpy; from pathlib import Path; '
                  'connect=runpy.run_path(' + repr(str(Path(__file__).resolve())) + ')["connect"]; '
                  'connect(endpoint=Path(' + repr(str(self.endpoint)) + '), start=True, host=Path('
                  + repr(str(self.host)) + '), startup_timeout=15)')
        child = subprocess.Popen([sys.executable, '-I', '-c', script],
            env=dict(os.environ, ZIPZI_TEST_START_DELAY_MS='1500'),
            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        try:
            deadline = time.monotonic() + 10
            while not self.pids():
                self.assertIsNone(child.poll())
                self.assertLess(time.monotonic(), deadline)
                time.sleep(.01)
            pid, = self.pids()
            child.kill()
            child.communicate(timeout=10)
            with self.connect(startup_timeout=10) as runtime:
                self.assertEqual(runtime.connection['host_pid'], pid)
                self.assertEqual(self.pids(), [pid])
        finally:
            if child.poll() is None:
                child.kill()
            child.communicate(timeout=10)

    @unittest.skipIf(os.name == "nt", "Unix socket inode/permission fixture; Windows pipe acceptance is separate")
    def test_stale_socket_is_replaced_by_exclusive_host_owner(self):
        with socket.socket(socket.AF_UNIX) as stale:
            stale.bind(str(self.endpoint))
        self.endpoint.chmod(0o600)
        with self.connect() as runtime:
            self.assertIsNotNone(runtime.connection)
            self.assertEqual(len(self.pids()), 1)

    def test_mismatched_build_never_launches_a_replacement(self):
        with self.connect() as first:
            self.assertIsNotNone(first.connection)
            with self.assertRaises(zipzi.SdkFailure) as error:
                connect(endpoint=self.endpoint, start=True, host=(self.folder / "missing-host"), sdk_build="wrong")
            self.assertEqual(error.exception.code, "BUILD_MISMATCH")
            self.assertEqual(len(self.pids()), 1)
            assert first.connection is not None
            with connect(endpoint=self.endpoint, sdk_build=first.connection['sdk_build']) as second:
                self.assertEqual(first.connection["runtime"], second.connection["runtime"])
                with self.assertRaises(TypeError):
                    second.connection["host_pid"] = 0

    def test_discovery_without_start_and_explicit_host_environment(self):
        with patch.dict(os.environ, ZIPZI_RUNTIME_DIR=str(self.folder), ZIPZI_RUNTIME_PROFILE="test", ZIPZI_EDITOR_HOST=str(self.host)):
            with self.assertRaises(FileNotFoundError):
                connect()
            self.assertEqual(self.pids(), [])
            with connect(start=True) as first:
                with connect() as second:
                    self.assertEqual(first.connection["runtime"], second.connection["runtime"])

    @unittest.skipIf(os.name == "nt", "Unix socket inode/permission fixture; Windows pipe acceptance is separate")
    def test_unsafe_directory_socket_and_lock_are_rejected_without_writes(self):
        self.folder.chmod(0o755)
        with self.assertRaises(PermissionError):
            self.connect()
        self.folder.chmod(0o700)
        self.endpoint.write_text("owned content")
        with self.assertRaises(PermissionError):
            self.connect()
        self.assertEqual(self.endpoint.read_text(), "owned content")
        self.endpoint.unlink()
        target = self.folder / "user-file"
        target.write_text("preserve")
        for path in (self.endpoint, self.endpoint.with_suffix(".lock")):
            path.symlink_to(target)
            with self.assertRaises(OSError):
                self.connect()
            self.assertEqual(target.read_text(), "preserve")
            path.unlink()
        self.assertEqual(self.pids(), [])

    def test_list_and_attach_response_loss_recover_same_owner(self):
        for operation in ("list_sessions", "attach_session"):
            with self.subTest(operation=operation), patch.dict(os.environ, ZIPZI_TEST_DROP_RESPONSE=operation):
                directory = self.folder / operation
                endpoint = directory / "test.sock"
                with connect(endpoint=endpoint, start=True, host=self.host) as runtime:
                    session = runtime.sessions.create(document_id="79200000-0000-4000-8000-000000000010")
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        runtime.sessions.list() if operation == "list_sessions" else runtime.sessions.attach(session.id)
                    recovered = error.exception.recover()
                    self.assertEqual(recovered[0]["session"] if operation == "list_sessions" else recovered.id, session.id)
                    self.assertEqual(runtime.sessions.attach(session.id).info()["history"]["undo_steps"], 0)

    def test_foreign_session_key_is_rejected_after_host_restart(self):
        with self.connect() as runtime:
            session = runtime.sessions.create(document_id="79200000-0000-4000-8000-000000000011")
            old = session.id
        stop(self.pids()[0])
        # Wait for the actual process to release its endpoint, not a fixed sleep.
        end = time.monotonic() + 5
        while self.endpoint.exists():
            # SIGTERM bypasses Drop, so socket existence is not a liveness test.
            try:
                with connect(endpoint=self.endpoint, timeout=.1):
                    pass
            except (ConnectionRefusedError, FileNotFoundError, EOFError, ConnectionResetError):
                break
            self.assertLess(time.monotonic(), end)
            time.sleep(.01)
        with self.connect() as fresh:
            with self.assertRaises(zipzi.SdkFailure) as error:
                fresh.sessions.attach(old)
            self.assertEqual(error.exception.code, "SESSION_NOT_FOUND")

    @unittest.skipIf(os.name == "nt", "Unix endpoint path validation")
    def test_invalid_timeouts_profiles_and_host_paths_are_rejected(self):
        for value in (0., -1., float("inf"), float("nan"), True):
            with self.assertRaises(ValueError):
                self.connect(startup_timeout=value)
        for path in (Path("relative.sock"), self.folder / "Bad.sock", self.folder / "x.txt"):
            with self.assertRaises(ValueError):
                zipzi.connect(endpoint=path)
        with self.assertRaises(ValueError):
            connect(endpoint=self.endpoint, start=True, host=Path("relative"))
        with self.assertRaises(zipzi.SdkFailure) as error:
            connect(endpoint=self.endpoint, start=True, host=(self.folder / "missing-host"))
        self.assertEqual(error.exception.code, "HOST_NOT_FOUND")
        self.assertEqual(self.pids(), [])

    @unittest.skipIf(os.name == "nt", "Unix socket inode/permission fixture; Windows pipe acceptance is separate")
    def test_protocol_mismatch_and_handshake_timeout_never_start_another_host(self):
        for respond in (True, False):
            with self.subTest(respond=respond), socket.socket(socket.AF_UNIX) as listener:
                listener.bind(str(self.endpoint))
                self.endpoint.chmod(0o600)
                listener.listen(1)
                release = threading.Event()
                def server():
                    with listener.accept()[0] as peer:
                        peer.settimeout(2)
                        def read(length):
                            data = bytearray()
                            while len(data) < length:
                                chunk = peer.recv(length-len(data))
                                if not chunk:
                                    raise EOFError()
                                data.extend(chunk)
                            return data
                        length, = struct.unpack(">I", read(4))
                        read(length)
                        if respond:
                            body = json.dumps(dict(error=dict(code="PROTOCOL_MISMATCH", message="different contract"))).encode()
                            peer.sendall(struct.pack(">I", len(body))+body)
                        else:
                            release.wait(2)
                worker = threading.Thread(target=server, daemon=True)
                worker.start()
                try:
                    if respond:
                        with self.assertRaises(zipzi.SdkFailure) as error:
                            self.connect()
                        self.assertEqual(error.exception.code, "PROTOCOL_MISMATCH")
                    else:
                        started = time.monotonic()
                        with self.assertRaises(TimeoutError):
                            self.connect(startup_timeout=.08)
                        self.assertLess(time.monotonic()-started, 1.)
                    self.assertEqual(self.pids(), [])
                finally:
                    release.set()
                    worker.join(3)
                    self.endpoint.unlink()

if __name__ == "__main__":
    unittest.main()
