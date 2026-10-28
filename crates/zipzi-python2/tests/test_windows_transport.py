"""Native Windows fixture transport acceptance; run after building the host."""
import os
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch
import psutil

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'client/python'))
from zipzi._windows_pipe import Pipe, endpoint_name
from zipzi.backends.connected import _exchange, _body, CONTRACT


class WindowsTransport(unittest.TestCase):
    def test_sendall_has_one_budget_across_chunks(self):
        pipe = Pipe()
        pipe.settimeout(1)
        with patch('zipzi._windows_pipe.time.monotonic', side_effect=[0, .1, .7, 1.3]), \
             patch.object(pipe, '_io', return_value=65536) as transfer:
            with self.assertRaises(TimeoutError):
                pipe.sendall(b'x' * (3 * 65536))
            self.assertEqual(transfer.call_count, 2)
            self.assertAlmostEqual(transfer.call_args_list[0].args[3], .9)
            self.assertAlmostEqual(transfer.call_args_list[1].args[3], .3)

    def test_concurrent_public_clients_and_crash_restart(self):
        import zipzi
        host = Path(os.environ['ZIPZI_PYTHON2_HOST']).resolve(strict=True)
        with tempfile.TemporaryDirectory(prefix='zipzi race 中文 ') as folder:
            log = Path(folder) / 'owners.txt'
            gate = Path(folder) / 'go'
            environment = dict(os.environ, ZIPZI_RUNTIME_DIR=folder,
                ZIPZI_RUNTIME_PROFILE='race', ZIPZI_TEST_PID_LOG=str(log),
                ZIPZI_TEST_START_DELAY_MS='200')
            clients = [subprocess.Popen([sys.executable, __file__, '--worker', str(gate)],
                env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
                for _ in range(4)]
            try:
                gate.touch()
                results = []
                for child in clients:
                    out, err = child.communicate(timeout=30)
                    self.assertEqual(child.returncode, 0, err)
                    results.append(json.loads(out))
                self.assertEqual(len({row['runtime'] for row in results}), 1)
                self.assertEqual(len({row['pid'] for row in results}), 1)
                self.assertEqual(log.read_text().splitlines(), [str(results[0]['pid'])])
                owner = psutil.Process(results[0]['pid'])
                self.assertTrue(owner.is_running())
                owner.kill()
                owner.wait(timeout=10)
                with patch.dict(os.environ, environment):
                    with zipzi.connect(start=True, host=host, startup_timeout=10) as runtime:
                        self.assertNotEqual(str(runtime.connection.runtime), results[0]['runtime'])
                        self.assertEqual(len(runtime.sessions.list()), 0)
                self.assertEqual(len(log.read_text().splitlines()), 2)
            finally:
                for child in clients:
                    if child.poll() is None:
                        child.kill()
                    child.communicate(timeout=10)
                if log.exists():
                    for pid in log.read_text().splitlines():
                        try:
                            owner = psutil.Process(int(pid))
                            if Path(owner.exe()).resolve() == host:
                                owner.terminate()
                                owner.wait(timeout=10)
                        except psutil.NoSuchProcess:
                            pass

    def test_cold_start_then_reuse_resident(self):
        from zipzi._endpoint import connect_backend
        host = Path(os.environ['ZIPZI_PYTHON2_HOST']).resolve(strict=True)
        with tempfile.TemporaryDirectory(prefix='zipzi cold ') as folder, patch.dict(os.environ,
            ZIPZI_RUNTIME_DIR=folder, ZIPZI_RUNTIME_PROFILE='cold'):
            first = connect_backend(None, start=True, host=host, timeout=3, startup_timeout=10, sdk_build=None)
            owner = psutil.Process(first.connection_info.host_pid)
            try:
                first._disconnect()
                self.assertTrue(owner.is_running())
                second = connect_backend(None, start=True, host=host, timeout=3, startup_timeout=10, sdk_build=None)
                try:
                    self.assertEqual(first.connection_info.runtime, second.connection_info.runtime)
                    self.assertEqual(first.connection_info.host_pid, second.connection_info.host_pid)
                finally:
                    second._disconnect()
            finally:
                first._disconnect()
                owner.terminate()
                owner.wait(timeout=10)

    def test_real_fixture_handshake_and_idle_timeout(self):
        host = Path(os.environ['ZIPZI_PYTHON2_HOST']).resolve(strict=True)
        with tempfile.TemporaryDirectory(prefix='zipzi 中文 ') as folder:
            environment = dict(os.environ, ZIPZI_RUNTIME_DIR=folder, ZIPZI_RUNTIME_PROFILE='test')
            process = subprocess.Popen([str(host), '--no-window'], env=environment,
                stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
            pipe = Pipe()
            try:
                endpoint = endpoint_name(Path(folder), 'test')
                deadline = time.monotonic() + 10
                while True:
                    try:
                        pipe.connect(str(endpoint))
                        break
                    except FileNotFoundError:
                        if process.poll() is not None or time.monotonic() > deadline:
                            self.fail('Fixture failed to publish pipe')
                        time.sleep(.025)
                response = _exchange(pipe, _body({'protocol': CONTRACT}))
                self.assertIn('ok', response)
                pipe.settimeout(.025)
                with self.assertRaises(TimeoutError):
                    pipe.recv(1)
            finally:
                pipe.close()
                process.terminate()
                process.communicate(timeout=10)


if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == '--worker':
        import zipzi
        deadline = time.monotonic() + 10
        while not Path(sys.argv[2]).exists():
            if time.monotonic() > deadline:
                raise TimeoutError('client start barrier')
            time.sleep(.005)
        with zipzi.connect(start=True, host=Path(os.environ['ZIPZI_PYTHON2_HOST']),
                           startup_timeout=15) as runtime:
            assert len(runtime.sessions.list()) == 0
            print(json.dumps({'runtime': str(runtime.connection.runtime),
                              'pid': runtime.connection.host_pid}))
    else:
        unittest.main()
