#!/usr/bin/env python3
"""Opt-in macOS process check for --no-window; terminates only its own child."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='zipzi-no-window-') as temporary:
        pid_file = Path(temporary)/'host.pid'
        env = dict(os.environ, ZIPZI_CONFIG_DIR=str(Path(temporary)/'config'), ZIPZI_DEV_PID_FILE=str(pid_file), ZIPZI_DEV_BACKGROUND='1')
        for key in list(env):
            if key.startswith(('ZIPZI_BENCH', 'ZIPZI_WINDOW_BENCH', 'ZIPZI_FRAME_TRACE', 'ZIPZI_DEV_SESSION')):
                env.pop(key)
        with (args.output/'application.log').open('w') as log:
            child = subprocess.Popen([str(args.binary.resolve()), '--no-window'], env=env, stdout=log, stderr=log)
            try:
                deadline = time.monotonic()+10
                while not pid_file.exists():
                    if child.poll() is not None or time.monotonic() >= deadline:
                        raise RuntimeError('host exited or did not publish its PID')
                    time.sleep(0.05)
                if int(pid_file.read_text()) != child.pid:
                    raise RuntimeError('PID discovery selected another process')
                time.sleep(1)
                if child.poll() is not None:
                    raise RuntimeError('host exited without a window')
                script = "ObjC.import('CoreGraphics'); JSON.stringify(ObjC.deepUnwrap(ObjC.castRefToObject($.CGWindowListCopyWindowInfo(0, 0))).filter(w => Number(w.kCGWindowOwnerPID) === " + str(child.pid) + ").length)"
                count = int(subprocess.check_output(['osascript','-l','JavaScript','-e',script], text=True, timeout=10).strip())
                if count:
                    raise RuntimeError(f'no-window host created {count} WindowServer windows')
                result = dict(pid_discovery_matches=True, host_alive_without_window=True, window_count=count, cleanup_spawned_pid=True)
            finally:
                if child.poll() is None:
                    child.terminate()
                    try:
                        child.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        child.kill()
                        child.wait(timeout=5)
        (args.output/'results.json').write_text(json.dumps(result, indent=2)+'\n')
        print(json.dumps(result))


if __name__ == '__main__':
    main()
