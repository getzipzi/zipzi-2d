"""Production desktop cold-start/reopen acceptance. Run with installed pure client."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
from uuid import uuid4
import zipzi


def main():
    host = Path(os.environ["ZIPZI_TEST_EDITOR_HOST"]).resolve()
    with tempfile.TemporaryDirectory(prefix="zzdesk-", dir="/tmp") as folder:
        directory = Path(folder)
        env = dict(os.environ, ZIPZI_RUNTIME_DIR=folder, ZIPZI_RUNTIME_PROFILE="test")
        env.pop("ZIPZI_DEV_SESSION_DIR", None)
        env.pop("PYTHONPATH", None)
        env["ZIPZI_DEV_PID_FILE"] = str(directory / "owner")
        env["ZIPZI_DEV_READY_FILE"] = str(directory / "ready")
        program = '''import json,sys,time,zipzi
from pathlib import Path
c=zipzi.connect(host=sys.argv[1]); marker=Path(sys.argv[2])
if sys.argv[3]=='create':
 s=c.create_session(sys.argv[4])
 with s.edit('cold-start Python') as e:e.create_parameter(sys.argv[5], 'Cold start',-1,1,0)
 marker.write_text(json.dumps(s.session_id))
else:
 deadline=time.monotonic()+15
 while not marker.exists():
  assert time.monotonic()<deadline; time.sleep(.01)
 s=c.attach_session(json.loads(marker.read_text()))
 assert s.parameter(sys.argv[5]).name=='Cold start'
print(json.dumps({'runtime':c.runtime_instance_id,'pid':c.host_pid,'session':s.session_id}),flush=True)
'''
        parameter = str(uuid4())
        children = [subprocess.Popen([sys.executable, "-I", "-c", program, str(host), str(directory / "session"), role, str(uuid4()), parameter], env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True) for role in ("create", "attach")]
        pid = None
        try:
            results = []
            for child in children:
                out, err = child.communicate(timeout=35)
                assert child.returncode == 0, err
                result = json.loads(out)
                pid = result["pid"]
                results.append(result)
            assert results[0] == results[1], results
            assert (directory / "ready").read_text() == "handshake complete"
            with zipzi.connect(start=False, directory=folder, profile="test") as connection:
                assert connection.host_pid == pid
                session = connection.attach_session(results[0]["session"])
                assert session.history_lengths() == (1,0)
                assert session.modified
                # A new desktop invocation forwards reopening to this resident host.
                reopened = subprocess.run([str(host)], env=env, capture_output=True, timeout=20)
                assert reopened.returncode == 0, reopened.stderr.decode()
                assert (directory / "owner").read_text() == f"reused:{pid}"
                assert connection.runtime_instance_id == results[0]["runtime"]
                with zipzi.connect(start=False, directory=folder, profile="test") as check:
                    assert check.host_pid == pid
                    assert check.runtime_instance_id == results[0]["runtime"]
                saved = session.save(directory / "project")
                assert saved.manifest.is_file()
                assert not session.modified
                assert connection.open_project(saved.manifest).session_id == session.session_id
            print(json.dumps({"production_host": str(host), "clients": 2, "same_runtime": True,
                "desktop_reopen_reused_host": True, "development_readiness_after_handshake": True, "python_exit_retains_unsaved_session": True,
                "history_steps": 1, "save_roundtrip": True}))
        finally:
            for child in children:
                if child.poll() is None:
                    child.kill()
                    child.wait()
            if pid is not None:
                try: os.kill(pid, signal.SIGTERM)
                except ProcessLookupError: pass


if __name__ == "__main__":
    main()
