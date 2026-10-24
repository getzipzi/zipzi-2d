"""Run from /tmp with -I: exercise installed wheels without source imports."""
import importlib.util
from importlib.metadata import version
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

import zipzi
from zipzi import meshes, ops

def main():
    mode = sys.argv[1]
    assert version("zipzi-python2") == "0.1.0"
    assert "/site-packages/zipzi/" in zipzi.__file__, zipzi.__file__
    assert "zipzi_native._native" not in sys.modules
    assert (importlib.util.find_spec("zipzi_native") is not None) == (mode == "local")
    with tempfile.TemporaryDirectory(prefix="zzpy2-", dir="/tmp") as folder:
        folder = Path(folder)
        child = None
        try:
            if mode == "local":
                assert version("zipzi-python2-native") == "0.1.0"
                runtime = zipzi.local()
            else:
                child = subprocess.Popen([os.environ["ZIPZI_PYTHON2_HOST"]],
                    env=dict(os.environ, ZIPZI_RUNTIME_DIR=str(folder), ZIPZI_RUNTIME_PROFILE="python2"))
                deadline = time.monotonic() + 10
                while True:
                    try:
                        runtime = zipzi.connect(endpoint=folder / "python2.sock")
                        break
                    except (FileNotFoundError, ConnectionRefusedError):
                        assert child.poll() is None
                        if time.monotonic() >= deadline:
                            raise TimeoutError("Host startup")
                        time.sleep(.01)
            with runtime:
                session = runtime.sessions.create("79300000-0000-4000-8000-000000000001")
                geometry = meshes.Geometry((42, 7, 99), ((0., 0.), (1., 0.), (0., 1.)),
                    ((0., 0.), (1., 0.), (0., 1.)), ((42, 7, 99),))
                with session.edit("installed") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("Face", geometry))
                    edit.apply(ops.Rename(face, "Installed"))
                    snapshot = edit.meshes.get(face)
                    assert snapshot.source.sequence == 2
                assert session.info().history == (1, 0)
                assert session.meshes.get(face).name == "Installed"
                saved = session.project.save(folder / "project").wait()
                assert saved.manifest.is_file()
            print(json.dumps(dict(mode=mode, name=snapshot.name, vertices=snapshot.geometry.vertex_ids, history=(1, 0))))
        finally:
            if child is not None:
                child.terminate()
                child.wait(timeout=5)

if __name__ == "__main__":
    main()
