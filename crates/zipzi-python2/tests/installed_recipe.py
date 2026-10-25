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
from typing import get_type_hints

import zipzi
from zipzi import bindings, sampling, events, meshes, ops, parameters, resources

def main():
    mode = sys.argv[1]
    assert version("zipzi-python2") == "0.1.0"
    assert "/site-packages/zipzi/" in zipzi.__file__, zipzi.__file__
    assert get_type_hints(ops.CreateImage)["image"] is resources.PreparedImage
    assert "zipzi_native._native" not in sys.modules
    assert (importlib.util.find_spec("zipzi_native") is not None) == (mode == "local")
    with tempfile.TemporaryDirectory(prefix="zzpy2-", dir="/tmp") as folder:
        folder = Path(folder)
        child = None
        try:
            if mode == "local":
                assert version("zipzi-python2-native") == "0.1.0"
                runtime = zipzi.local()
                assert not hasattr(runtime._backend._native, "_test_control")
                assert not hasattr(runtime._backend._native, "_measure")
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
                stream = session.subscribe()
                geometry = meshes.Geometry((42, 7, 99), ((0., 0.), (1., 0.), (0., 1.)),
                    ((0., 0.), (1., 0.), (0., 1.)), ((42, 7, 99),))
                with runtime.resources.prepare_image(1, 1, bytes((12, 34, 56, 255))) as image:
                    with session.edit("installed") as edit:
                        asset = edit.resources.create_image("Draft", image)
                        face = edit.meshes.create(meshes.MeshSpec("Face", geometry, texture_asset_id=asset))
                        with edit.resources.plan(resources.RenameIntent(asset, "Draft", "Texture")) as plan:
                            assert plan.report.source.sequence == 2
                            edit.apply_many((ops.ApplyPlan(plan), ops.Rename(face, "Installed")))
                        with runtime.resources.prepare_image(1, 1, bytes((21, 43, 65, 255))) as replacement:
                            edit.resources.replace_image(asset, replacement)
                        snapshot = edit.meshes.get(face)
                        assert snapshot.source.sequence == 4
                        assert edit.resources.get(asset).pixels.rgba == bytes((21, 43, 65, 255))
                        parameter = "79300000-0000-4000-8000-000000000002"
                        result = edit.apply_many((ops.CreateParameter(parameters.ParameterSpec("Draft", -1., 1., 0.,
                            id=parameter, runtime_id="ParamInstalled", kind=parameters.Kind.BLEND_SHAPE, decimal_places=3, repeat=True)),
                            ops.PatchParameter(parameter, parameters.Patch(name="Installed parameter", default_value=.5, repeat=False))))
                        assert result.source.sequence == 5
                        assert edit.parameters.get_many((parameter,)).items[0].name == "Installed parameter"
                        assert edit.meshes.get_many((face,))[0].name == "Installed"
                        angle = "79300000-0000-4000-8000-000000000003"
                        result = edit.apply_many((ops.CreateParameter(parameters.ParameterSpec("Pose", 0., 1., 0., id=angle)),
                            ops.CreateMeshBinding(bindings.MeshBindingSpec(face, (bindings.Axis(angle, (0., 1.)),), (
                                bindings.Keyform((0.,), geometry.positions),
                                bindings.Keyform((1.,), tuple((x+10., y) for x,y in geometry.positions)))))))
                        binding = result.ids[-1]
                        assert result.source.sequence == 6
                        canonical = edit.bindings.get(binding)
                        edit.bindings.set_keyform(binding, bindings.Keyform((1.,), tuple((x+20.,y) for x,y in geometry.positions)))
                        cpu = edit.modeling.sample(({angle:.5},))
                        assert cpu.frames[0].drawables[0].positions[0][0] == 10.
                        assert cpu.frames[0].render_plan == (sampling.RenderCommand(sampling.RenderKind.DRAW_MESH,face),)
                    assert image.pixels().rgba == bytes((12, 34, 56, 255))
                assert runtime.resources.usage().prepared_images == 0
                assert runtime.resources.usage().plans == 0
                assert session.info().history == (1, 0)
                assert session.meshes.get(face).name == "Installed"
                assert session.modeling.sample(({angle:.5},)).frames[0].drawables[0].positions[0][0] == 10.
                published = [e for e in stream.poll() if isinstance(e, events.Published)]
                assert len(published) == 1 and parameter in published[0].receipt.object_ids
                assert session.undo().wait().changed
                assert session.info().history == (0, 1)
                try:
                    session.meshes.get(face)
                except zipzi.SdkFailure as error:
                    assert error.code == "MISSING_MESH"
                else:
                    raise AssertionError("Undo retained a newly created mesh")
                assert session.redo().wait().changed
                assert session.resources.get(asset).pixels.rgba == bytes((21, 43, 65, 255))
                assert session.info().history == (1, 0)
                save_job = session.project.save(folder / "project")
                deadline = time.monotonic() + 10
                while save_job.state() not in {"succeeded", "failed", "cancelled"}:
                    if time.monotonic() >= deadline:
                        raise TimeoutError("Installed Job completion")
                    time.sleep(.005)
                save_job.cancel()  # A terminal success remains success.
                saved = save_job.wait()
                assert save_job.state() == "succeeded"
                assert saved.manifest.is_file()
                assert any(isinstance(e, events.Saved) and e.receipt == saved for e in stream.poll())
            if child is not None:
                child.terminate()
                child.wait(timeout=5)
                child = None
                # A fresh host is required to reopen from disk, rather than
                # return a session retained by the disconnected host.
                (folder / "reopened").mkdir(mode=0o700)
                child = subprocess.Popen([os.environ["ZIPZI_PYTHON2_HOST"]], env=dict(os.environ,
                    ZIPZI_RUNTIME_DIR=str(folder / "reopened"), ZIPZI_RUNTIME_PROFILE="python2"))
                deadline = time.monotonic() + 10
                while True:
                    try:
                        fresh = zipzi.connect(endpoint=folder / "reopened/python2.sock")
                        break
                    except (FileNotFoundError, ConnectionRefusedError):
                        assert child.poll() is None
                        if time.monotonic() >= deadline:
                            raise TimeoutError("Reopen host startup")
                        time.sleep(.01)
            else:
                fresh = zipzi.local()
            with fresh:
                reopened = fresh.sessions.open(saved.manifest).wait()
                assert reopened.resources.get(asset).name == "Texture"
                assert reopened.resources.get(asset).pixels.rgba == bytes((21, 43, 65, 255))
                assert reopened.meshes.get(face).name == "Installed"
                value = reopened.parameters.get(parameter)
                assert (value.name,value.runtime_id,value.kind,value.decimal_places,value.repeat,value.default_value) == (
                    "Installed parameter","ParamInstalled",parameters.Kind.BLEND_SHAPE,3,False,.5)
                assert reopened.bindings.get(binding).axes == canonical.axes
                assert reopened.modeling.sample(({angle:.5},)).frames[0].drawables[0].positions[0][0] == 10.
                assert reopened.info().history == (0, 0)
            print(json.dumps(dict(mode=mode, name=snapshot.name, vertices=snapshot.geometry.vertex_ids, history=(1, 0))))
        finally:
            if child is not None:
                child.terminate()
                child.wait(timeout=5)

if __name__ == "__main__":
    main()
