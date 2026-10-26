"""Run the documentation workflow from normal installed packages only."""
from pathlib import Path
import argparse
import os
import subprocess
from tempfile import TemporaryDirectory
import time
import zipzi
from zipzi.docs.recipes import author_and_save,close_repeatedly,discovery_and_failure,recover_public_calls


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('mode', choices=('local', 'connected'),
                        help='Connected requires ZIPZI_PYTHON2_HOST pointing to the normal CPU host')
    parser.add_argument('--drop',choices=('apply_authoring','commit','job'),
                        help='Connected-only lost-reply example using an external test-support host')
    args = parser.parse_args()
    if args.drop and args.mode!='connected':parser.error('--drop requires connected')
    assert '/site-packages/zipzi/' in zipzi.__file__
    child=None
    with TemporaryDirectory(prefix='zzdocs-',dir='/tmp') as directory:
        folder=Path(directory)
        try:
            if args.mode=='local':runtime=zipzi.local()
            else:
                env=dict(os.environ,ZIPZI_RUNTIME_DIR=directory,ZIPZI_RUNTIME_PROFILE='docs')
                if args.drop:env.update(ZIPZI_TEST_DROP_RESPONSE=args.drop)
                child=subprocess.Popen([os.environ['ZIPZI_PYTHON2_HOST']],env=env)
                end=time.monotonic()+10
                while True:
                    try:runtime=zipzi.connect(endpoint=folder/'docs.sock');break
                    except (FileNotFoundError,ConnectionRefusedError):
                        if child.poll() is not None or time.monotonic()>end:raise
                        time.sleep(.01)
            with runtime:
                if args.drop:
                    session=runtime.sessions.create()
                    recover_public_calls(session)
                    session.close(discard_modified=True)
                else:
                    receipt=author_and_save(runtime,folder)
                    assert receipt.durable
                    discovery_and_failure(runtime)
                    session=runtime.sessions.create()
                    recover_public_calls(session)
                    session.close(discard_modified=True)
                    close_repeatedly(runtime)
                assert runtime.sessions.list()==()
            print(args.mode+': installed documentation examples passed'+(' (lost '+args.drop+' reply)' if args.drop else ' (mixed batch, discovery, failure, recovery, save/reopen, pixels, 40 closes)'))
        finally:
            if child is not None:child.terminate();child.wait(timeout=5)

if __name__=='__main__':main()
