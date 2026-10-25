"""Installed new client against a current production desktop executable."""
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
import unittest
import zipzi


class DesktopConnection(unittest.TestCase):
    def test_cold_start_reopen_same_session_and_save_in_real_editor(self):
        self.assertIn("/site-packages/zipzi/", zipzi.__file__)
        self.assertIsNone(importlib.util.find_spec("zipzi_native"))
        host = Path(os.environ["ZIPZI_PYTHON2_EDITOR"])
        self.assertTrue(host.is_absolute() and host.is_file())
        with tempfile.TemporaryDirectory(prefix="zzpy2e-", dir="/tmp") as directory:
            folder = Path(directory)
            endpoint = folder / "test.sock"
            processes = []
            host_pids = set()
            try:
                script = '''
import json,sys,time,zipzi
from pathlib import Path
from zipzi import meshes
while not Path(sys.argv[4]).exists(): time.sleep(.005)
with zipzi.connect(endpoint=Path(sys.argv[1]),start=True,host=Path(sys.argv[2])) as r:
 assert r.connection is not None
 print(json.dumps(dict(pid=r.connection.host_pid,runtime=r.connection.runtime)),flush=True)
 assert 'show_window' in r.connection.capabilities
 if sys.argv[3]=='0':
  s=r.sessions.create('79200000-0000-4000-8000-000000000020')
  g=meshes.Geometry((0,1,2),((0.,0.),(1.,0.),(0.,1.)),((0.,0.),(1.,0.),(0.,1.)),((0,1,2),))
  with s.edit('desktop client one') as e: e.meshes.create(meshes.MeshSpec('First',g,id='79200000-0000-4000-8000-000000000021'))
 else:
  end=time.monotonic()+10
  while True:
   sessions=r.sessions.list()
   if sessions and sessions[0].history==(1,0): break
   assert time.monotonic()<end
   time.sleep(.01)
  s=r.sessions.attach(sessions[0].id)
  with s.edit('desktop client two') as e: e.meshes.rename('79200000-0000-4000-8000-000000000021','Second')
 print(json.dumps(dict(serial=s.id.serial)),flush=True)
'''
                gate = folder / "go"
                environment = {k: v for k, v in os.environ.items()
                    if k not in {"ZIPZI_DEV_SESSION_DIR", "ZIPZI_DEV_PID_FILE", "ZIPZI_DEV_READY_FILE", "PYTHONPATH"}}
                for index in range(2):
                    processes.append(subprocess.Popen([sys.executable, "-I", "-c", script, str(endpoint), str(host), str(index), str(gate)],
                        cwd="/tmp", env=environment, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True))
                gate.touch()
                identities = []
                for child in processes:
                    stdout, stderr = child.communicate(timeout=25)
                    rows = [json.loads(line) for line in stdout.splitlines()]
                    if rows:
                        host_pids.add(rows[0]["pid"])
                    self.assertEqual(child.returncode, 0, stderr)
                    self.assertEqual(len(rows), 2)
                    identities.append(rows)
                self.assertEqual(identities[0], identities[1])
                self.assertEqual(len(host_pids), 1)
                with zipzi.connect(endpoint=endpoint) as runtime:
                    info, = runtime.sessions.list()
                    session = runtime.sessions.attach(info.id)
                    self.assertEqual(session.info().history, (2, 0))
                    self.assertTrue(session.info().modified)
                    self.assertEqual(session.meshes.get("79200000-0000-4000-8000-000000000021").name, "Second")
                    session.show_window()
                    # Desktop relaunch uses its existing ownership/reopen path.
                    reopened = subprocess.Popen([str(host)], cwd="/tmp", env=dict(environment,
                        ZIPZI_RUNTIME_DIR=str(folder), ZIPZI_RUNTIME_PROFILE="test"), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
                    processes.append(reopened)
                    _, stderr = reopened.communicate(timeout=20)
                    self.assertEqual(reopened.returncode, 0, stderr.decode())
                    with zipzi.connect(endpoint=endpoint) as another:
                        self.assertEqual(runtime.connection, another.connection)
                        self.assertEqual(another.sessions.attach(session.id).info().history, (2, 0))
                    saved = session.project.save(folder / "project").wait()
                    self.assertTrue(saved.manifest.is_file())
                    self.assertFalse(session.info().modified)
                    old_id = session.id
                os.kill(next(iter(host_pids)), signal.SIGTERM)
                end = time.monotonic() + 5
                while True:
                    try:
                        with zipzi.connect(endpoint=endpoint, timeout=.1):
                            pass
                    except (OSError, EOFError):
                        break
                    self.assertLess(time.monotonic(), end)
                    time.sleep(.01)
                with zipzi.connect(endpoint=endpoint, start=True, host=host) as fresh:
                    assert fresh.connection is not None
                    host_pids.add(fresh.connection.host_pid)
                    self.assertNotEqual(old_id.runtime, fresh.connection.runtime)
                    with self.assertRaises(zipzi.SdkFailure):
                        fresh.sessions.attach(old_id)
                    session = fresh.sessions.open(saved.manifest).wait()
                    self.assertEqual(session.meshes.get("79200000-0000-4000-8000-000000000021").name, "Second")
                    self.assertEqual(session.info().history, (0, 0))
                print("desktop: concurrent cold start, shared unsaved history, window/relaunch reuse, save/restart/open passed")
            finally:
                # This private endpoint belongs exclusively to this test.
                try:
                    with zipzi.connect(endpoint=endpoint, timeout=.5) as resident:
                        if resident.connection:
                            host_pids.add(resident.connection.host_pid)
                except (OSError, EOFError, zipzi.SdkFailure):
                    pass
                for child in processes:
                    if child.poll() is None:
                        child.kill()
                    child.wait(timeout=5)
                for pid in host_pids:
                    try:
                        os.kill(pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass


if __name__ == "__main__":
    unittest.main()
