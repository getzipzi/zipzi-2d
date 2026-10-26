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
import tomllib
from typing import get_type_hints

import zipzi
from zipzi import bindings, sampling, events, meshes, ops, parameters, resources, scene

def main():
    mode = sys.argv[1]
    expected_version = tomllib.loads((Path(__file__).resolve().parents[1] / "client/pyproject.toml").read_text())["project"]["version"]
    assert version("zipzi") == expected_version
    assert "/site-packages/zipzi/" in zipzi.__file__, zipzi.__file__
    assert get_type_hints(resources.CreateImage)["image"] is resources.PreparedImage
    assert get_type_hints(zipzi.OutcomeUnknown.recover)["return"] is object
    assert get_type_hints(zipzi.recover_call)
    assert "zipzi_native._native" not in sys.modules
    assert (importlib.util.find_spec("zipzi_native") is not None) == (mode == "local")
    with tempfile.TemporaryDirectory(prefix="zzpy2-", dir="/tmp") as folder:
        folder = Path(folder)
        child = None
        try:
            if mode == "local":
                assert version("zipzi-native") == expected_version
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
                        asset = zipzi.recover_call(edit.resources.create_image, "Draft", image)
                        face = zipzi.recover_call(edit.meshes.create, meshes.MeshSpec("Face", geometry, texture_asset_id=asset))
                        with edit.plan(resources.RenameImage(asset, "Texture", expected_name="Draft")) as plan:
                            assert plan.report.source.sequence == 2
                            edit.apply_many((plan, meshes.Rename(face, "Installed")))
                        with runtime.resources.prepare_image(1, 1, bytes((21, 43, 65, 255))) as replacement:
                            edit.resources.replace_image(asset, replacement)
                        snapshot = edit.meshes.get(face)
                        assert snapshot.source.sequence == 4
                        assert edit.resources.get(asset).value.pixels.rgba == bytes((21, 43, 65, 255))
                        parameter = "79300000-0000-4000-8000-000000000002"
                        result = edit.apply_many((parameters.Create(parameters.ParameterSpec("Draft", -1., 1., 0.,
                            id=parameter, runtime_id="ParamInstalled", kind=parameters.Kind.BLEND_SHAPE, decimal_places=3, repeat=True)),
                            parameters.Patch(parameter, parameters.ParameterPatch(name="Installed parameter", default_value=.5, repeat=False))))
                        assert result.source.sequence == 5
                        assert edit.parameters.get_many((parameter,)).items[0].name == "Installed parameter"
                        assert edit.meshes.get_many((face,)).items[0].name == "Installed"
                        angle = "79300000-0000-4000-8000-000000000003"
                        result = edit.apply_many((parameters.Create(parameters.ParameterSpec("Pose", 0., 1., 0., id=angle)),
                            bindings.Create(bindings.MeshBindingSpec(face, (bindings.Axis(angle, (0., 1.)),), (
                                bindings.Keyform((0.,), geometry.positions),
                                bindings.Keyform((1.,), tuple((x+10., y) for x,y in geometry.positions)))))))
                        binding = result.ids[-1]
                        assert result.source.sequence == 6
                        canonical = edit.bindings.get(binding)
                        edit.bindings.set_keyform(binding, bindings.Keyform((1.,), tuple((x+20.,y) for x,y in geometry.positions)))
                        cpu = edit.modeling.sample(({angle:.5},))
                        assert cpu.value.frames[0].drawables[0].positions[0][0] == 10.
                        assert cpu.value.frames[0].render_plan == (sampling.RenderCommand(sampling.RenderKind.DRAW_MESH,face),)
                        part = "79300000-0000-4000-8000-000000000010"
                        parent = "79300000-0000-4000-8000-000000000011"
                        blend_parameter = "79300000-0000-4000-8000-000000000012"
                        table = "79300000-0000-4000-8000-000000000013"
                        blend = "79300000-0000-4000-8000-000000000014"
                        base_sample = "79300000-0000-4000-8000-000000000015"
                        active_sample = "79300000-0000-4000-8000-000000000016"
                        edit.apply_many((
                            scene.CreatePart(scene.PartSpec("Installed folder", id=part)),
                            scene.CreateRotation("Parent", scene.Placement(part_id=part), id=parent),
                            scene.MoveToPart((face,), part_id=part),
                            parameters.Create(parameters.ParameterSpec("Blend", 0., 1., 0., id=blend_parameter, kind=parameters.Kind.BLEND_SHAPE)),
                            scene.CreateBlendTable(blend_parameter, (scene.Sample(base_sample,0.),scene.Sample(active_sample,1.)), base_sample, id=table),
                            scene.CreateMeshBlend(face, table, id=blend),
                            scene.SetMeshBlendVertices(face, blend, active_sample, tuple(scene.VertexPatch(v,position=(2.,0.)) for v in geometry.vertex_ids)),
                            scene.InsertMeshVertex(face, 50, (.5,0.), (.5,0.), (scene.VertexWeight(42,.5),scene.VertexWeight(7,.5))),
                            scene.ReplaceTriangles(face, ((42,50,99),(50,7,99))),
                        ))
                        with edit.plan(scene.ReparentLocal((face,),parent_id=parent)) as plan:
                            assert plan.report.cost is not None
                            edit.apply(plan)
                        assert edit.scene.get(face).value.parent_id == parent
                        assert edit.modeling.sample(({angle:.5,blend_parameter:.5},)).value.frames[0].drawables[0].positions[0][0] == 11.
                    assert image.pixels().rgba == bytes((12, 34, 56, 255))
                assert runtime.resources.usage().prepared_images == 0
                assert runtime.resources.usage().plans == 0
                assert session.info().history == (1, 0)
                with session.reference("mesh",face) as ref:
                    assert ref.resolve().version == session.info().version
                    assert ref.scene_capabilities().patch.state == "available"
                    with ref.label() as address:
                        assert address.expected == "Installed"
                        with address.plan("Installed") as plan:
                            assert plan.report.no_op
                assert session.meshes.get(face).value.name == "Installed"
                assert session.modeling.sample(({angle:.5},)).value.frames[0].drawables[0].positions[0][0] == 10.
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
                assert session.resources.get(asset).value.pixels.rgba == bytes((21, 43, 65, 255))
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
                import runpy
                r4=runpy.run_path(str(Path(__file__).with_name("r4_recipe.py")))["run"](runtime,Path(__file__).resolve().parents[3],folder/"r4")
                p3b=runpy.run_path(str(Path(__file__).with_name("p3b_recipe.py")))["run"](runtime,folder/"p3b")
                p4=runpy.run_path(str(Path(__file__).with_name("p4_recipe.py")))["run"](runtime,folder/"p4")
                r5=runpy.run_path(str(Path(__file__).with_name("r5_authoring_recipe.py")))["run"](runtime,folder/"r5")
                preview=runpy.run_path(str(Path(__file__).with_name("r5_preview_recipe.py")))["run"](runtime,folder/"r5-preview")
                spatial=runpy.run_path(str(Path(__file__).with_name("r5_spatial_recipe.py")))["run"](runtime,folder/"r5-spatial")
                print(json.dumps(dict(r4=r4,p3b=p3b,p4=p4,r5=r5,preview=preview,spatial=spatial),sort_keys=True))
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
                assert reopened.resources.get(asset).value.name == "Texture"
                assert reopened.resources.get(asset).value.pixels.rgba == bytes((21, 43, 65, 255))
                assert reopened.meshes.get(face).value.name == "Installed"
                assert reopened.scene.get(face).value.parent_id == parent
                assert reopened.scene.get(part).value.name == "Installed folder"
                assert reopened.meshes.get(face).value.geometry.vertex_ids == (42,7,99,50)
                assert reopened.modeling.sample(({angle:.5,blend_parameter:.5},)).value.frames[0].drawables[0].positions[0][0] == 11.
                value = reopened.parameters.get(parameter)
                assert (value.value.name,value.value.runtime_id,value.value.kind,value.value.decimal_places,value.value.repeat,value.value.default_value) == (
                    "Installed parameter","ParamInstalled",parameters.Kind.BLEND_SHAPE,3,False,.5)
                assert reopened.bindings.get(binding).value.axes == canonical.value.axes
                assert reopened.modeling.sample(({angle:.5},)).value.frames[0].drawables[0].positions[0][0] == 10.
                assert reopened.info().history == (0, 0)
            print(json.dumps(dict(mode=mode, name=snapshot.value.name, vertices=snapshot.value.geometry.vertex_ids, history=(1, 0))))
        finally:
            if child is not None:
                child.terminate()
                child.wait(timeout=5)

if __name__ == "__main__":
    main()
