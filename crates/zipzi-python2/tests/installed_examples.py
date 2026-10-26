"""Run the documentation workflow from normal installed packages only."""
from pathlib import Path
import argparse
import os
import subprocess
from tempfile import TemporaryDirectory
import time
import zipzi
from zipzi.docs.recipes import author_and_save,close_repeatedly


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('local', 'connected'),
                        help='Connected requires ZIPZI_PYTHON2_HOST pointing to the normal CPU host')
    args = parser.parse_args()
    assert '/site-packages/zipzi/' in zipzi.__file__
    child=None
    with TemporaryDirectory(prefix='zzdocs-',dir='/tmp') as directory:
        folder=Path(directory)
        try:
            if args.mode=='local':runtime=zipzi.local()
            else:
                child=subprocess.Popen([os.environ['ZIPZI_PYTHON2_HOST']],env=dict(os.environ,ZIPZI_RUNTIME_DIR=directory,ZIPZI_RUNTIME_PROFILE='docs'))
                end=time.monotonic()+10
                while True:
                    try:runtime=zipzi.connect(endpoint=folder/'docs.sock');break
                    except (FileNotFoundError,ConnectionRefusedError):
                        if child.poll() is not None or time.monotonic()>end:raise
                        time.sleep(.01)
            with runtime:
                receipt=author_and_save(runtime,folder)
                assert receipt.durable
                close_repeatedly(runtime)
                assert runtime.sessions.list()==()
            print(args.mode+': installed documentation workflow, save/reopen, pixels and 40 session closes passed')
        finally:
            if child is not None:child.terminate();child.wait(timeout=5)

if __name__=='__main__':main()
