"""Installed pure client; real subprocess races and host-owned endpoint locks."""
from dataclasses import FrozenInstanceError
import importlib.util
import json
import os
from pathlib import Path
import signal
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
from zipzi import bindings, events, meshes, ops, parameters, sampling, scene


def stop(pid):
    try:
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        pass


class InstalledConnection(unittest.TestCase):
    def setUp(self):
        self.assertIn("/site-packages/zipzi/", zipzi.__file__)
        self.assertIsNone(importlib.util.find_spec("zipzi_native"))
        self.directory = tempfile.TemporaryDirectory(prefix="zzpy2c-", dir="/tmp")
        self.addCleanup(self.directory.cleanup)
        self.folder = Path(self.directory.name)
        self.endpoint = self.folder / "test.sock"
        self.log = self.folder / "pids"
        self.host = Path(os.environ["ZIPZI_PYTHON2_HOST"])
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
        return zipzi.connect(endpoint=self.endpoint, start=True, host=self.host, **options)

    def test_installed_complete_calls_keep_projection_job_and_cached_edit_state(self):
        geometry = meshes.Geometry((42, 7, 99), ((0., 0.), (1., 0.), (0., 1.)),
                                   ((0., 0.), (1., 0.), (0., 1.)), ((42, 7, 99),))
        for operation in ("apply_authoring", "scene_snapshot", "commit", "job"):
            with self.subTest(operation=operation), patch.dict(os.environ, ZIPZI_TEST_DROP_RESPONSE=operation):
                with zipzi.connect(endpoint=self.folder / operation / "test.sock", start=True, host=self.host) as runtime:
                    session = runtime.sessions.create("79200000-0000-4000-8000-000000000050")
                    with session.edit("installed complete call") as edit:
                        id = zipzi.recover_call(edit.meshes.create, meshes.MeshSpec("Face", geometry))
                        self.assertIsInstance(id, str)
                        snapshot = zipzi.recover_call(edit.scene.get, id)
                        self.assertIsInstance(snapshot, zipzi.Snapshot)
                        self.assertEqual(snapshot.value.name, "Face")
                        job = zipzi.recover_call(edit.commit)
                        self.assertIsInstance(job, zipzi.Job)
                        self.assertEqual(edit.state, zipzi.EditState.committing)
                        self.assertIsNone(edit.receipt)
                        receipt = zipzi.recover_call(job.wait)
                        self.assertIsInstance(receipt, zipzi.Receipt)
                    self.assertIs(edit.receipt, receipt)
                    self.assertEqual(edit.state, zipzi.EditState.committed)
                    self.assertEqual(session.info().history, (1, 0))
                    self.assertEqual(session.meshes.get(id).value.geometry.vertex_ids, (42, 7, 99))

    def test_two_process_cold_start_shares_one_unsaved_session_and_history(self):
        script = '''
import json,sys,time,zipzi
from pathlib import Path
from zipzi import meshes,ops
while not Path(sys.argv[4]).exists(): time.sleep(.005)
with zipzi.connect(endpoint=Path(sys.argv[1]),start=True,host=Path(sys.argv[2])) as r:
 if sys.argv[3]=='0':
  s=r.sessions.create('79200000-0000-4000-8000-000000000001')
  g=meshes.Geometry((0,1,2),((0.,0.),(1.,0.),(0.,1.)),((0.,0.),(1.,0.),(0.,1.)),((0,1,2),))
  with s.edit('first client') as e: e.meshes.create(meshes.MeshSpec('First',g,id='79200000-0000-4000-8000-000000000002'))
 else:
  end=time.monotonic()+10
  while True:
   sessions=r.sessions.list()
   if sessions and sessions[0].history==(1,0): break
   assert time.monotonic()<end
   time.sleep(.01)
  s=r.sessions.attach(sessions[0].id)
  with s.edit('second client') as e: e.meshes.rename('79200000-0000-4000-8000-000000000002','Second')
 assert r.connection is not None
 print(json.dumps(dict(runtime=r.connection.runtime,pid=r.connection.host_pid,serial=s.id.serial)))
'''
        gate = self.folder / "go"
        children = [subprocess.Popen([sys.executable, "-I", "-c", script, str(self.endpoint), str(self.host), str(index), str(gate)],
            cwd="/tmp", stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) for index in range(2)]
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
        with zipzi.connect(endpoint=self.endpoint) as runtime:
            info, = runtime.sessions.list()
            self.assertEqual(info.history, (2, 0))
            self.assertIsNone(info.path)
            self.assertEqual(info.state, "ready")
            session = runtime.sessions.attach(info.id)
            self.assertEqual(session.id, info.id)
            self.assertEqual(session.meshes.get("79200000-0000-4000-8000-000000000002").value.name, "Second")
            session.undo().wait()
            self.assertEqual(session.meshes.get("79200000-0000-4000-8000-000000000002").value.name, "First")

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
                zipzi.connect(endpoint=self.endpoint, start=True, host=Path("/missing/host"), sdk_build="wrong")
            self.assertEqual(error.exception.code, "BUILD_MISMATCH")
            self.assertEqual(len(self.pids()), 1)
            assert first.connection is not None
            with zipzi.connect(endpoint=self.endpoint, sdk_build=first.connection.sdk_build) as second:
                self.assertEqual(first.connection, second.connection)
                with self.assertRaises(FrozenInstanceError):
                    second.connection.host_pid = 0

    def test_discovery_without_start_and_explicit_host_environment(self):
        with patch.dict(os.environ, ZIPZI_RUNTIME_DIR=str(self.folder), ZIPZI_RUNTIME_PROFILE="test", ZIPZI_EDITOR_HOST=str(self.host)):
            with self.assertRaises(FileNotFoundError):
                zipzi.connect()
            self.assertEqual(self.pids(), [])
            with zipzi.connect(start=True) as first:
                with zipzi.connect() as second:
                    self.assertEqual(first.connection, second.connection)

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
                with zipzi.connect(endpoint=endpoint, start=True, host=self.host) as runtime:
                    session = runtime.sessions.create("79200000-0000-4000-8000-000000000010")
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        runtime.sessions.list() if operation == "list_sessions" else runtime.sessions.attach(session.id)
                    recovered = error.exception.recover()
                    self.assertEqual(recovered[0].id if operation == "list_sessions" else recovered.id, session.id)
                    self.assertEqual(runtime.sessions.attach(session.id).info().history, (0, 0))

    def test_foreign_runtime_identity_and_window_capability_are_checked(self):
        with self.connect() as runtime:
            session = runtime.sessions.create("79200000-0000-4000-8000-000000000011")
            old = session.id
            with self.assertRaises(zipzi.SdkFailure) as error:
                session.show_window()
            self.assertEqual(error.exception.code, "UNSUPPORTED_OPERATION")
        stop(self.pids()[0])
        # Wait for the actual process to release its endpoint, not a fixed sleep.
        end = time.monotonic() + 5
        while self.endpoint.exists():
            # SIGTERM bypasses Drop, so socket existence is not a liveness test.
            try:
                with zipzi.connect(endpoint=self.endpoint, timeout=.1):
                    pass
            except (ConnectionRefusedError, FileNotFoundError, EOFError, ConnectionResetError):
                break
            self.assertLess(time.monotonic(), end)
            time.sleep(.01)
        with self.connect() as fresh:
            with self.assertRaises(zipzi.SdkFailure) as error:
                fresh.sessions.attach(old)
            self.assertEqual(error.exception.code, "INVALID_REFERENCE")

    def test_invalid_timeouts_profiles_and_host_paths_are_rejected(self):
        for value in (0., -1., float("inf"), float("nan"), True):
            with self.assertRaises(ValueError):
                self.connect(startup_timeout=value)
        for path in (Path("relative.sock"), self.folder / "Bad.sock", self.folder / "x.txt"):
            with self.assertRaises(ValueError):
                zipzi.connect(endpoint=path)
        with self.assertRaises(ValueError):
            zipzi.connect(endpoint=self.endpoint, start=True, host=Path("relative"))
        with self.assertRaises(zipzi.SdkFailure) as error:
            zipzi.connect(endpoint=self.endpoint, start=True, host=Path("/missing/host"))
        self.assertEqual(error.exception.code, "HOST_NOT_FOUND")
        self.assertEqual(self.pids(), [])

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

    def test_installed_parameter_and_event_recovery_preserves_original_request(self):
        for operation in ("subscribe", "events", "parameter_snapshot", "apply_authoring", "commit", "job"):
            with self.subTest(operation=operation), patch.dict(os.environ, ZIPZI_TEST_DROP_RESPONSE=operation):
                endpoint = self.folder / operation / "test.sock"
                with zipzi.connect(endpoint=endpoint, start=True, host=self.host) as runtime:
                    session = runtime.sessions.create("79200000-0000-4000-8000-000000000030")
                    if operation == "subscribe":
                        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                            session.subscribe()
                        stream = lost.exception.recover()
                        self.assertIsInstance(stream, events.SessionSubscription)
                    else:
                        stream = session.subscribe()
                    parameter = "79200000-0000-4000-8000-000000000031"
                    edit = session.edit("installed mixed parameter batch")
                    operations = (parameters.Create(parameters.ParameterSpec("First", -1., 1., 0., id=parameter,
                        runtime_id="ParamRecovery", decimal_places=4, repeat=True)),
                        parameters.Patch(parameter, parameters.ParameterPatch(name="Final")))
                    if operation in {"apply_authoring", "commit", "job"}:
                        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                            with edit:
                                edit.apply_many(operations)
                        recovered = lost.exception.recover()
                        if operation == "apply_authoring":
                            self.assertEqual(recovered.source.sequence, 1)
                            edit.commit().wait()
                        elif operation == "commit":
                            self.assertIsInstance(recovered, zipzi.Job)
                            recovered.wait()
                    else:
                        with edit:
                            edit.apply_many(operations)
                    if operation == "parameter_snapshot":
                        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                            session.parameters.get(parameter)
                        value = lost.exception.recover()
                    else:
                        value = session.parameters.get(parameter)
                    self.assertEqual((value.value.name,value.value.runtime_id,value.value.decimal_places,value.value.repeat), ("Final","ParamRecovery",4,True))
                    if operation == "events":
                        before = stream.cursor
                        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                            stream.poll()
                        self.assertEqual(stream.cursor, before)
                        with self.assertRaises(zipzi.Busy):
                            stream.resync()
                        delivered = lost.exception.recover()
                    else:
                        delivered = stream.poll()
                    published = [event for event in delivered if isinstance(event, events.Published)]
                    self.assertEqual(len(published), 1)
                    self.assertEqual(published[0].receipt.after, session.info().version)
                    self.assertEqual(stream.poll(), ())
                    self.assertEqual(session.info().history, (1, 0))

    def test_installed_cpu_sample_recovery_keeps_old_frame_after_peer_write(self):
        with patch.dict(os.environ, ZIPZI_TEST_DROP_RESPONSE="sample_modeling"), self.connect() as runtime:
            session = runtime.sessions.create("79200000-0000-4000-8000-000000000040")
            angle = "79200000-0000-4000-8000-000000000041"
            face = "79200000-0000-4000-8000-000000000042"
            binding = "79200000-0000-4000-8000-000000000043"
            geometry = meshes.Geometry((0,1,2), ((0.,0.),(1.,0.),(0.,1.)), ((0.,0.),(1.,0.),(0.,1.)), ((0,1,2),))
            with session.edit("installed binding") as edit:
                edit.apply_many((meshes.Create(meshes.MeshSpec("Face",geometry,id=face)),
                    parameters.Create(parameters.ParameterSpec("Angle",0.,1.,0.,id=angle)),
                    bindings.Create(bindings.MeshBindingSpec(face,(bindings.Axis(angle,(0.,1.)),),(
                        bindings.Keyform((0.,),geometry.positions),
                        bindings.Keyform((1.,),tuple((x+10.,y) for x,y in geometry.positions))),id=binding))))
            before = session.info()
            with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                session.modeling.sample(({angle:.5},))
            with zipzi.connect(endpoint=self.endpoint) as peer:
                with peer.sessions.attach(session.id).edit("later") as edit:
                    edit.bindings.set_keyform(binding, bindings.Keyform((1.,),tuple((x+20.,y) for x,y in geometry.positions)))
            original = lost.exception.recover()
            self.assertIsInstance(original,zipzi.Snapshot)
            self.assertEqual(original.value.version,before.version)
            self.assertEqual(original.value.frames[0].drawables[0].positions[0][0],5.)
            self.assertEqual(session.modeling.sample(({angle:.5},)).value.frames[0].drawables[0].positions[0][0],10.)
            self.assertEqual(session.info().history,(2,0))

    def test_installed_lost_busy_response_allows_retry_and_cancel(self):
        for finish in ("commit", "cancel"):
            with self.subTest(finish=finish), patch.dict(os.environ,
                    ZIPZI_TEST_BUSY_COMMIT_ONCE="1", ZIPZI_TEST_DROP_RESPONSE="commit"):
                with zipzi.connect(endpoint=self.folder / finish / "test.sock", start=True, host=self.host) as runtime:
                    session = runtime.sessions.create("79200000-0000-4000-8000-000000000042")
                    edit = session.edit("recover Busy")
                    with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                        with edit:
                            parameter = edit.parameters.create(parameters.ParameterSpec("Base", -1., 1., 0.))
                            source = edit.parameters.get(parameter).source
                    with self.assertRaises(zipzi.Busy):
                        lost.exception.recover()
                    self.assertIsNone(edit.pending_request)
                    self.assertEqual(edit.parameters.get(parameter).source, source)
                    if finish == "commit":
                        edit.parameters.patch(parameter, parameters.ParameterPatch(name="Retried"))
                        edit.commit().wait()
                        self.assertEqual(session.parameters.get(parameter).value.name, "Retried")
                        self.assertEqual(session.info().history, (1, 0))
                    else:
                        edit.cancel()
                        self.assertEqual(session.parameters.list().items, ())
                        self.assertEqual(session.info().history, (0, 0))

    def test_installed_busy_and_caught_errors_keep_the_public_edit_rules(self):
        with patch.dict(os.environ, ZIPZI_TEST_BUSY_COMMIT_ONCE="1"):
            with self.connect() as runtime:
                session = runtime.sessions.create("79200000-0000-4000-8000-000000000040")
                edit = session.edit("confirmed Busy")
                with self.assertRaises(zipzi.Busy):
                    with edit:
                        parameter = edit.parameters.create(parameters.ParameterSpec("Base", -1., 1., 0.))
                self.assertEqual(edit.parameters.get(parameter).source.sequence, 1)
                edit.commit().wait()
                with self.assertRaises(zipzi.SdkFailure) as caught:
                    with session.edit("caught domain failure") as failed:
                        failed.parameters.patch(parameter, parameters.ParameterPatch(name="Unpublished"))
                        try:
                            failed.parameters.patch(parameter, parameters.ParameterPatch(minimum=2.))
                        except zipzi.SdkFailure:
                            pass
                self.assertEqual(caught.exception.code, "EDIT_ABORTED")
                self.assertEqual(session.parameters.get(parameter).value.name, "Base")
                self.assertEqual(session.info().history, (1, 0))
        with patch.dict(os.environ, ZIPZI_TEST_DROP_RESPONSE="release_edit"):
            with zipzi.connect(endpoint=self.folder / "cleanup" / "test.sock", start=True, host=self.host) as runtime:
                session = runtime.sessions.create("79200000-0000-4000-8000-000000000041")
                edit = session.edit("lost cleanup")
                with self.assertRaises(zipzi.SdkFailure) as original:
                    with edit:
                        edit.parameters.create(parameters.ParameterSpec("Invalid", 1., -1., 0.))
                self.assertEqual(original.exception.code, "INVALID_PARAMETER")
                self.assertTrue(any("cleanup failed" in note.lower() for note in original.exception.__notes__))
                self.assertIsInstance(original.exception.cleanup, zipzi.OutcomeUnknown)
                original.exception.cleanup.recover()
                self.assertEqual(session.parameters.list().items, ())
                self.assertEqual(session.info().history, (0, 0))


if __name__ == "__main__":
    unittest.main()
