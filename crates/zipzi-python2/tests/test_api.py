"""One handwritten recipe on Local and Connected; independent geometry/history assertions."""
from dataclasses import FrozenInstanceError, replace
import gc
import weakref
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch
from uuid import UUID

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "crates/zipzi-python2/client/python"
sys.path[:0] = [str(SOURCE), os.environ["ZIPZI_PYTHON2_NATIVE"]]
import zipzi
from zipzi import modeling as m
from zipzi import bindings, sampling, events, meshes, ops, parameters, resources, scene

def uid(n: int) -> str:
    return f"79000000-0000-4000-8000-{n:012x}"

def runtime_endpoint(directory):
    if os.name == 'nt':
        from zipzi._windows_pipe import endpoint_name
        return endpoint_name(directory, 'python2')
    return directory / 'python2.sock'

GEOMETRY = meshes.Geometry((42, 7, 99), ((0., 0.), (1., 0.), (0., 1.)),
    ((0., 0.), (1., 0.), (0., 1.)), ((42, 7, 99),))

class PythonSdk(unittest.TestCase):
    def test_manual_mesh_source_contract_and_existing_large_writes(self):
        from zipzi._generated import binary, modeling as wire
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime=self.runtime(mode)
                session=runtime.sessions.create(uid(1))
                with session.edit("mesh") as edit:
                    mesh=edit.meshes.create(meshes.MeshSpec("Mesh",GEOMETRY,id=uid(2)))
                    captured=edit.meshes.get(mesh).source
                    operation=m.ModelingCommandManualMesh(mesh_id=mesh,expected=captured,
                        operation=m.ManualMeshOperationInsertVertex(id=100,uv=m.Vec2(.25,.25)))
                    self.assertEqual(wire.read_ModelingCommand(wire.encode_ModelingCommand(operation)),operation)
                    # Independently check the opaque source writer against the
                    # established source reader, including candidate sequence.
                    for source in (captured,session.meshes.list().source):
                        writer=binary.Writer();binary._write_sourceref(source,writer)
                        reader=binary.Reader(bytes(writer.data))
                        self.assertEqual(binary._read_sourceref(reader),source);reader.finish()
                    edit.apply(operation)
                    self.assertEqual(edit.meshes.get(mesh).value.geometry.vertex_ids, (42,7,99,100))
                with self.assertRaises(zipzi.SdkFailure):
                    with session.edit("stale source") as edit:edit.apply(operation)
                # A pre-existing operation large enough to use binary upload
                # must still decode under the extended command signature.
                with session.edit("large existing operation") as edit:
                    edit.meshes.rename(mesh,"M"*(70<<10))
                self.assertEqual(session.meshes.get(mesh).value.name,"M"*(70<<10))
    def test_r5_spatial_frozen_sources_hidden_geometry_and_animated_numerics(self):
        import runpy
        recipe=runpy.run_path(str(Path(__file__).with_name("r5_spatial_recipe.py")))["run"]
        results=[recipe(self.runtime(mode),self.folder/(mode+"-spatial")) for mode in ("local","connected")]
        self.assertEqual(results[0],results[1])

    def test_r5_spatial_complete_call_recovery(self):
        from zipzi import spatial as s
        for phase in ("request_spatial_samples","job","read_blob_binary"):
            with self.subTest(phase=phase):
                runtime=self.runtime("connected",drop=phase)
                session=runtime.sessions.create(uid(1))
                job=zipzi.recover_call(session.modeling.spatial,({},{}),(s.Hits(point=(0.,0.)),))
                result=zipzi.recover_call(job.wait)
                self.assertEqual(len(result.value.frames),2)
                self.assertEqual(tuple(f.results[0].value.total for f in result.value.frames),(0,0))
                self.assertEqual(session.info().history,(0,0))

    def test_r5_spatial_worker_cancellation_and_local_direct_values(self):
        from zipzi import spatial as s
        for mode in ("local","connected"):
            with self.subTest(mode=mode):
                runtime=self.runtime(mode)
                session=runtime.sessions.create(uid(1))
                before=session.info()
                backend=runtime._backend
                backend._test_control("hold_io")
                job=session.modeling.spatial(({},),(s.Hits(point=(0.,0.)),))
                deadline=time.monotonic()+3
                while not backend._test_control("entered"):
                    self.assertLess(time.monotonic(),deadline);time.sleep(.005)
                job.cancel();backend._test_control("release_io")
                with self.assertRaises(zipzi.SdkFailure) as cancelled:job.wait()
                self.assertEqual(cancelled.exception.code,"CANCELLED")
                self.assertEqual(session.info(),before)
        runtime=self.runtime("local");session=runtime.sessions.create(uid(1))
        with patch("json.dumps",side_effect=AssertionError("Spatial Python JSON encode")), \
             patch("json.loads",side_effect=AssertionError("Spatial Python JSON decode")):
            result=session.modeling.spatial(({},),(s.Hits(point=(.25,.25)),)).wait()
            self.assertEqual(result.value.frames[0].results[0].value.total,0)
            with session.preview() as preview:
                result=preview.seek(.25,geometry=False,queries=(s.Hits(point=(.25,.25)),)).wait()
                self.assertEqual(result.value.frames[0].spatial[0].value.total,0)

    def test_r5_large_spatial_details_binary_recovery(self):
        from zipzi import spatial as s
        for mode in ("local","connected"):
            runtime=self.runtime(mode,drop="read_blob_binary" if mode=="connected" else None,drop_after=1)
            session=runtime.sessions.create(uid(1))
            # Every triangle contributes independently; the result exceeds one Blob chunk.
            geometry=replace(GEOMETRY,triangles=GEOMETRY.triangles*2048)
            with session.edit("Overlapping triangles") as edit:
                edit.meshes.create(meshes.MeshSpec("Overlap",geometry,id=uid(2)))
            before=session.info()
            job=session.modeling.spatial(tuple({} for _ in range(8)),(s.Hits(point=(.25,.25),details=True),))
            result=zipzi.recover_call(job.wait)
            self.assertEqual(len(result.value.frames),8)
            for frame in result.value.frames:
                triangles=frame.results[0].value.hits[0].triangles
                self.assertEqual(len(triangles),2048)
                self.assertEqual(triangles[-1].triangle_index,2047)
                self.assertEqual(triangles[-1].vertex_ids,(42,7,99))
                self.assertEqual(triangles[-1].barycentric,(.5,.25,.25))
            self.assertEqual(session.info(),before)

    def test_r5_independent_previews_numeric_geometry_and_candidate_sources(self):
        import runpy
        recipe=runpy.run_path(str(Path(__file__).with_name("r5_preview_recipe.py")))["run"]
        results=[recipe(self.runtime(mode),self.folder/(mode+"-preview")) for mode in ("local","connected")]
        self.assertEqual(results[0],results[1])

    def test_r5_preview_complete_call_recovery_and_close(self):
        from zipzi import animation as a
        for phase in ("create_preview","apply_preview","sample_preview","job","read_blob_binary","release_preview"):
            with self.subTest(phase=phase):
                runtime=self.runtime("connected",drop=phase)
                session=runtime.sessions.create(uid(1))
                preview=zipzi.recover_call(session.preview)
                self.assertIsInstance(preview,a.Preview)
                state=zipzi.recover_call(preview.apply,a.Reset())
                self.assertEqual(state.value.sequence,1)
                job=zipzi.recover_call(preview.seek,.5)
                value=zipzi.recover_call(job.wait)
                self.assertEqual(value.value.frames[0].motion.time,.5)
                self.assertEqual(value.value.sequence,2)
                self.assertEqual(preview.info().value.sequence,2)
                zipzi.recover_call(preview.close)
                preview.close()
                self.assertEqual(runtime.resources.usage().previews,0)

    def test_r5_preview_workers_cancel_without_blocking_formal_or_other_clients(self):
        from zipzi import animation as a
        for mode in ("local","connected"):
            with self.subTest(mode=mode):
                runtime=self.runtime(mode)
                session=runtime.sessions.create(uid(1))
                preview=session.preview()
                backend=runtime._backend
                backend._test_control("hold_io")
                job=preview.seek(100.)
                deadline=time.monotonic()+3
                while not backend._test_control("entered"):
                    self.assertLess(time.monotonic(),deadline)
                    time.sleep(.005)
                with self.assertRaises(zipzi.Busy):preview.apply(a.Reset())
                self.assertEqual(session.info().history,(0,0))
                job.cancel()
                backend._test_control("release_io")
                with self.assertRaises(zipzi.SdkFailure) as cancelled:job.wait()
                self.assertEqual(cancelled.exception.code,"CANCELLED")
                self.assertEqual(preview.info().value.time,0.)
                self.assertEqual(preview.seek(.5,geometry=False).wait().value.frames[0].motion.time,.5)
                preview.close()
                self.assertEqual(runtime.resources.usage().previews,0)
        runtime=self.runtime("connected")
        session=runtime.sessions.create(uid(1))
        first=session.preview()
        first.seek(.5,geometry=False).wait()
        with zipzi.connect(endpoint=runtime._backend._endpoint) as peer:
            bound=peer.sessions.attach(session.id)
            second=bound.preview()
            second.seek(1.,geometry=False).wait()
            self.assertEqual(first.info().value.time,.5)
            self.assertEqual(runtime.resources.usage().previews,2)
        self.assertEqual(runtime.resources.usage().previews,1)
        self.assertEqual(first.info().value.time,.5)
        first.close()

    def test_r5_local_preview_has_no_python_json_roundtrip(self):
        from zipzi import animation as a
        runtime=self.runtime("local")
        session=runtime.sessions.create(uid(1))
        with patch("json.dumps",side_effect=AssertionError("Preview encoded Python JSON")), \
             patch("json.loads",side_effect=AssertionError("Preview decoded Python JSON")):
            with session.preview() as preview:
                preview.apply(a.Reset())
                value=preview.sample((0.,.5,0.)).wait()
                self.assertEqual(tuple(f.motion.time for f in value.value.frames),(0.,.5,0.))
                self.assertEqual(preview.info().value.sequence,2)

    def test_r5_large_preview_batch_binary_recovery_preserves_every_frame(self):
        for mode in ("local","connected"):
            runtime=self.runtime(mode,drop="read_blob_binary" if mode=="connected" else None,drop_after=1)
            session=runtime.sessions.create(uid(1))
            size=32
            points=tuple((float(x),float(y)) for y in range(size) for x in range(size))
            triangles=tuple(t for y in range(size-1) for x in range(size-1)
                for t in ((y*size+x,y*size+x+1,(y+1)*size+x),
                          (y*size+x+1,(y+1)*size+x+1,(y+1)*size+x)))
            geometry=meshes.Geometry(tuple(range(size*size)),points,points,triangles)
            with session.edit("Grid") as edit:edit.meshes.create(meshes.MeshSpec("Grid",geometry,id=uid(2)))
            before=session.info()
            with session.preview() as preview:
                job=preview.sample(tuple(0. for _ in range(32)))
                value=zipzi.recover_call(job.wait)
                self.assertEqual(len(value.value.frames),32)
                self.assertEqual(preview.info().value.sequence,1)
                self.assertEqual(tuple(f.index for f in value.value.frames),tuple(range(32)))
                self.assertTrue(all(len(f.geometry.drawables[0].positions)==1024 for f in value.value.frames))
            self.assertEqual(session.info(),before)

    def test_r5_physics_extension_shadowing_aborts_the_candidate_on_both_backends(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                before = session.info()
                with self.assertRaisesRegex(ValueError, "overlap declared fields"):
                    with session.edit("Invalid flattened metadata") as edit:
                        edit.apply(m.ModelingCommandPhysics(intent=m.PhysicsEditCreateGroup(
                            asset_id=uid(2), id=uid(3), name="Rig")))
                        physics = edit.modeling.records(m.ModelingQuery(physics=True)).value.physics
                        group = physics.data.settings[0]
                        normalized = replace(group.normalization, extensions={"Position": 42})
                        invalid = replace(physics, data=replace(physics.data,
                            settings=(replace(group, normalization=normalized),)))
                        edit.apply(m.ModelingCommandSetPhysics(physics=invalid))
                self.assertEqual(session.info(), before)
                self.assertIsNone(session.modeling.records(m.ModelingQuery(physics=True)).value.physics)

    def test_r5_large_motion_binary_records_and_complete_call_recovery(self):
        for mode, phase, after in (("local", None, 0), ("connected", None, 0),
                ("connected", "apply_authoring_blob", 0), ("connected", "modeling_query", 1),
                ("connected", "read_blob_binary", 0)):
            with self.subTest(mode=mode, phase=phase):
                runtime = self.runtime(mode, drop=phase, drop_after=after)
                session = runtime.sessions.create(uid(1))
                with session.edit("Large motion graph") as edit:
                    parameter = edit.parameters.create(parameters.ParameterSpec("P", -1., 1., 0., id=uid(2)))
                    edit.apply(m.ModelingCommandMotion(intent=m.MotionEditCreate(id=uid(3), name="Large",
                        registration_id=uid(4), destination=m.MotionContainerRoot())))
                    clip = edit.modeling.records(m.ModelingQuery(motions=(uid(3),))).value.motions[0]
                    segments = tuple(m.MotionSegmentLinear(end=m.MotionKey(id=uid(10_000+i),
                        time=float(i), value=float(i % 2))) for i in range(1, 4097))
                    track = m.MotionTrack(id=uid(5), target=m.MotionTrackTargetParameter(parameter_id=parameter),
                        initial=m.MotionKey(id=uid(6), time=0., value=0.), segments=segments, extensions={})
                    operation = m.ModelingCommandReplaceMotion(clip=replace(clip, duration=4096., tracks=(track,)))
                    if phase == "apply_authoring_blob":
                        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                            edit.apply(operation)
                        result = lost.exception.recover()
                        self.assertIsInstance(result, zipzi.WriteResult)
                        self.assertEqual(result.source.sequence, 3)
                    else:
                        edit.apply(operation)
                    query = m.ModelingQuery(motions=(uid(3),))
                    if phase in {"modeling_query", "read_blob_binary"}:
                        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                            edit.modeling.records(query)
                        snapshot = lost.exception.recover()
                    else:
                        snapshot = edit.modeling.records(query)
                    self.assertEqual(snapshot.source.sequence, 3)
                    self.assertEqual(snapshot.value.motions[0].tracks[0], track)
                    self.assertEqual(session.info().history, (0, 0))
                self.assertEqual(session.info().history, (1, 0))
                self.assertEqual(snapshot.value.motions[0].tracks[0].segments[-1].end.time, 4096.)

    def test_r5_local_animation_bridge_does_not_encode_or_decode_python_json(self):
        runtime = self.runtime("local")
        with patch("json.dumps", side_effect=AssertionError("Local animation encoded JSON")), \
             patch("json.loads", side_effect=AssertionError("Local animation decoded JSON")):
            session = runtime.sessions.create(uid(1))
            with session.edit("Direct animation") as edit:
                edit.apply_many((
                    m.ModelingCommandMotion(intent=m.MotionEditCreate(id=uid(2), name="Motion",
                        registration_id=uid(3), destination=m.MotionContainerRoot())),
                    m.ModelingCommandExpression(intent=m.ExpressionEditCreate(id=uid(4), name="Expression")),
                    m.ModelingCommandPhysics(intent=m.PhysicsEditCreateGroup(asset_id=uid(5), id=uid(6), name="Physics")),
                    m.ModelingCommandPose(intent=m.PoseEditCreateGroup(pose_id=uid(7), id=uid(8), name="Pose")),
                ))
                selected = edit.modeling.records(m.ModelingQuery(motions=(uid(2),), expressions=(uid(4),), physics=True, pose=True))
                self.assertEqual(selected.value.motions[0].name, "Motion")
                self.assertEqual(selected.value.physics.data.meta.PhysicsSettingCount, 1)
                self.assertEqual(selected.source.sequence, 1)
            self.assertEqual(session.info().history, (1, 0))

    def test_r5_animation_authoring_plans_addresses_and_sidecars(self):
        import runpy
        recipe = runpy.run_path(str(Path(__file__).with_name("r5_authoring_recipe.py")))["run"]
        results = []
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                results.append(recipe(self.runtime(mode), self.folder / (mode + "-r5")))
        if len(results) == 2:
            self.assertEqual(results[0], results[1])

    def test_manual_mesh_structure_roundtrips_local_connected_binary_history_and_project(self):
        boundary = [10, 20, 30, 40]
        support = meshes.Constraint((10, 30), meshes.ConstraintKind.EDGE_SUPPORT)
        meshing = meshes.Meshing(boundary, [support])
        boundary.reverse()
        geometry = meshes.Geometry((10,20,30,40), ((0.,0.),(-1.,0.),(-1.,1.),(0.,1.)),
            ((0.,0.),(1.,0.),(1.,1.),(0.,1.)), ((10,20,30),(10,30,40)), meshing)
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                with session.edit("manual mesh") as edit:
                    mesh = edit.meshes.create(meshes.MeshSpec("mesh", geometry, id=uid(2)))
                    self.assertEqual(edit.meshes.get(mesh).value.geometry, geometry)
                self.assertEqual(session.meshes.get(mesh).value.geometry, geometry)
                before = session.modeling.records(m.ModelingQuery(meshes=(mesh,))).value.meshes[0]
                self.assertEqual(before.meshing.boundary, (10,20,30,40))
                user = replace(before.meshing, constraints=(replace(before.meshing.constraints[0], kind=m.MeshConstraintKind.USER),))
                with session.edit("fix support") as edit:
                    edit.apply(m.ModelingCommandReplaceMesh(mesh=replace(before, meshing=user)))
                expected = replace(geometry, meshing=meshes.Meshing((10,20,30,40), [meshes.Constraint((10,30), meshes.ConstraintKind.USER)]))
                self.assertEqual(session.meshes.get(mesh).value.geometry, expected)
                self.assertEqual(session.info().history, (2,0))
                session.undo().wait()
                self.assertEqual(session.meshes.get(mesh).value.geometry, geometry)
                session.redo().wait()
                self.assertEqual(session.meshes.get(mesh).value.geometry, expected)
                with session.edit("force borrowed binary response") as edit:
                    edit.meshes.rename(mesh, "mesh" * 4096)
                self.assertEqual(session.meshes.get(mesh).value.geometry, expected)
                with self.assertRaises(zipzi.SdkFailure):
                    with session.edit("do not discard structure") as edit:
                        edit.apply(m.ModelingCommandReplaceMesh(mesh=replace(before, meshing=None)))
                saved = session.project.save(self.folder / (mode + "-manual-mesh")).wait()
                reopened = self.runtime(mode).sessions.open(saved.manifest).wait()
                self.assertEqual(reopened.meshes.get(mesh).value.geometry, expected)

    def test_p4_large_attachments_resource_relocation_and_package_roundtrip(self):
        import runpy
        recipe = runpy.run_path(str(Path(__file__).with_name("p4_recipe.py")))["run"]
        results = []
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                results.append(recipe(self.runtime(mode), self.folder / (mode + "-p4")))
        if len(results) == 2:
            self.assertEqual(results[0], results[1])

    def test_p3b_conversions_scene_bindings_and_independent_collections(self):
        import runpy
        recipe = runpy.run_path(str(Path(__file__).with_name("p3b_recipe.py")))["run"]
        results = []
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                results.append(recipe(self.runtime(mode), self.folder / (mode + "-p3b")))
        if len(results) == 2:
            self.assertEqual(results[0], results[1])

    def test_domain_operations_and_unified_plans_preserve_content_and_history(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode), self.runtime(mode) as runtime:
                session = runtime.sessions.create(uid(1))
                with runtime.resources.prepare_image(1, 1, b"rgba") as image:
                    with session.edit("unified operations") as edit:
                        spec = bindings.MeshBindingSpec(uid(2), [bindings.Axis(uid(3), [0., 1.])], [
                            bindings.Keyform([0.], GEOMETRY.positions),
                            bindings.Keyform([1.], [(x+10., y) for x,y in GEOMETRY.positions])], id=uid(4))
                        result = edit.apply_many((meshes.Create(meshes.MeshSpec("Face", GEOMETRY, id=uid(2))),
                            parameters.Create(parameters.ParameterSpec("Angle", 0., 1., 0., id=uid(3))),
                            bindings.Create(spec), resources.CreateImage("Texture", image, id=uid(5))))
                        self.assertEqual(result.source.sequence, 1)
                        self.assertEqual(result.ids, (uid(2), uid(3), uid(4), uid(5)))
                        self.assertEqual(edit.modeling.sample(({uid(3): .5},)).value.frames[0].drawables[0].positions[0], (5., 0.))
                        edit.apply(meshes.Rename(uid(2), "Ready", expected_name="Face"))
                        with edit.plan(meshes.Rename(uid(2), "Final", expected_name="Ready")) as plan:
                            cached = plan.report
                            result = edit.apply_many((plan, parameters.Patch(uid(3), parameters.ParameterPatch(default_value=.5))))
                            self.assertEqual(result.source.sequence, 3)
                        self.assertEqual(plan.report, cached)
                        with edit.plan(resources.RenameImage(uid(5), "Planned texture")) as plan:
                            edit.apply(plan)
                        edit.apply(resources.RenameImage(uid(5), "Final texture", expected_name="Planned texture"))
                self.assertEqual(session.meshes.get(uid(2)).value.name, "Final")
                self.assertEqual(session.resources.get(uid(5)).value.name, "Final texture")
                self.assertEqual(session.resources.get(uid(5)).value.pixels.rgba, b"rgba")
                self.assertEqual(session.parameters.get(uid(3)).value.default_value, .5)
                self.assertEqual(session.info().history, (1, 0))
                before = session.info()
                for operation in (meshes.Rename(uid(2), "Wrong", expected_name="stale"),
                                  resources.RenameImage(uid(5), "Wrong", expected_name="stale")):
                    with self.assertRaises(zipzi.SdkFailure):
                        with session.edit("wrong precondition") as edit:
                            edit.apply(operation)
                    self.assertEqual(session.info(), before)

    def test_unified_plan_batch_order_and_recovery_do_not_repeat_writes(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode), self.runtime(mode) as runtime:
                session = runtime.sessions.create(uid(1))
                with session.edit("initial") as edit:
                    id = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                before = session.info()
                with session.plan(meshes.Rename(id, "Planned")) as plan:
                    for values in ((meshes.Rename(id, "First"), plan), (plan, plan)):
                        with self.assertRaises(ValueError):
                            with session.edit("invalid order") as edit:
                                edit.apply_many(values)
                        self.assertEqual(session.info(), before)
                    with session.edit("apply formal plan") as edit:
                        result = edit.apply(plan)
                        self.assertEqual(result.source.sequence, 1)
                self.assertEqual(session.meshes.get(id).value.name, "Planned")
                self.assertEqual(session.info().history, (2, 0))
        for operation in ("plan_authoring", "apply_authoring"):
            with self.subTest(lost=operation), self.runtime("connected", drop=operation,
                    drop_after=1 if operation=="apply_authoring" else 0) as runtime:
                session = runtime.sessions.create(uid(1))
                with session.edit("initial") as edit:
                    id = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                with zipzi.recover_call(session.plan, meshes.Rename(id, "Recovered")) as plan:
                    with session.edit("recover plan apply") as edit:
                        result = zipzi.recover_call(edit.apply, plan)
                        self.assertEqual(result.source.sequence, 1)
                self.assertEqual(session.meshes.get(id).value.name, "Recovered")
                self.assertEqual(session.info().history, (2, 0))

    def test_generated_dtos_own_inputs_and_remove_parallel_public_intents(self):
        import dataclasses, inspect, pickle, typing
        vertices, positions = [42, 7, 99], [[0.,0.],[1.,0.],[0.,1.]]
        geometry = meshes.Geometry(vertices, positions, positions, [[42,7,99]])
        operation = meshes.SetPositions(uid(2), vertices, positions)
        axis_keys = [0., 1.]
        axis = bindings.Axis(uid(3), axis_keys)
        vertices.clear(); positions[0][0]=100.; axis_keys.clear()
        self.assertEqual(geometry.vertex_ids, (42,7,99))
        self.assertEqual(operation.positions[0], (0.,0.))
        self.assertEqual(axis.keys, (0.,1.))
        for cls in (parameters.Create, parameters.ParameterSpec, parameters.ParameterPatch,
                    bindings.Keyform, sampling.Checks, sampling.Frame, events.Published,
                    m.ModelingCommandScene, m.ModelingCommandResource):
            typing.get_type_hints(cls)
            self.assertNotIn("_variant", inspect.signature(cls).parameters)
            if typing.get_origin(typing.get_type_hints(cls).get("kind")) is typing.ClassVar:
                self.assertNotIn("kind", inspect.signature(cls).parameters)
            self.assertFalse(any(field.name.startswith("_") for field in dataclasses.fields(cls)))
        self.assertEqual(pickle.loads(pickle.dumps((geometry, operation, axis))), (geometry, operation, axis))
        self.assertEqual(set(ops.__all__), {"Operation", "PlannableOperation"})
        self.assertFalse(any(hasattr(ops, name) for name in ("Create", "Rename", "ApplyPlan")))
        self.assertFalse(hasattr(meshes, "RenameIntent"))
        self.assertFalse(hasattr(resources, "RenameIntent"))
        self.assertFalse(hasattr(m, "ModelingIntentScene"))
        self.assertFalse(hasattr(zipzi.Edit, "apply_plan"))

    def test_uniform_results_own_values_and_preserve_empty_batch_sources(self):
        import pickle
        for mode in ("local", "connected"):
            with self.subTest(mode=mode), self.runtime(mode) as runtime:
                session = runtime.sessions.create(uid(1))
                with session.edit("uniform reads") as edit:
                    initial = edit.meshes.get_many(())
                    self.assertIsInstance(initial, zipzi.Batch)
                    self.assertEqual(initial.items, ())
                    self.assertEqual(initial.source.sequence, 0)
                    self.assertIsNotNone(initial.source.edit)
                    a = edit.meshes.create(meshes.MeshSpec("A", GEOMETRY))
                    b = edit.meshes.create(meshes.MeshSpec("B", GEOMETRY))
                    value = edit.meshes.get(a)
                    self.assertIsInstance(value, zipzi.Snapshot)
                    self.assertIsInstance(value.value, meshes.Mesh)
                    self.assertFalse(hasattr(value.value, "source"))
                    batch = edit.meshes.get_many((b, a, b))
                    self.assertEqual(tuple(item.id for item in batch.items), (b, a, b))
                    self.assertEqual(batch.source, value.source)
                    for domain in (edit.meshes, edit.parameters, edit.bindings, edit.scene):
                        empty = domain.get_many(())
                        self.assertIsInstance(empty, zipzi.Batch)
                        self.assertEqual(empty.items, ())
                        self.assertEqual(empty.source, value.source)
                    with self.assertRaises(zipzi.SdkFailure):
                        edit.meshes.get_many((a, uid(999)))
                    self.assertEqual(edit.meshes.get(a), value)
                formal = session.meshes.get_many(())
                self.assertIsNone(formal.source.edit)
                self.assertEqual(formal.source.version, session.info().version)
                items = [value.value]
                owned = zipzi.Batch(value.source, items)
                page = zipzi.Page(value.source, items)
                items.clear()
                self.assertEqual(owned.items, (value.value,))
                self.assertEqual(page.items, owned.items)
                self.assertIsNone(page.next_cursor)
                with self.assertRaises(FrozenInstanceError):
                    value.value.name = "mutated"
                with self.assertRaises(FrozenInstanceError):
                    value.source = formal.source
            self.assertEqual(pickle.loads(pickle.dumps((value, owned, page))), (value, owned, page))

    def test_native_plan_admission_precedes_string_conversion_and_keeps_edit_usable(self):
        class OverBudgetText(str):
            def __len__(self):return 1<<30
        class OverBudgetTuple(tuple):
            def __len__(self):return 1<<30
        with self.runtime("local") as runtime:
            session=runtime.sessions.create(uid(1))
            with session.edit("preflight plan") as edit:
                mesh=edit.meshes.create(meshes.MeshSpec("Mesh",GEOMETRY,id=uid(2)))
                source=edit.meshes.get(mesh).source
                with self.assertRaises(zipzi.SdkFailure) as rejected:
                    edit.plan(meshes.Rename(mesh, OverBudgetText("Renamed"), expected_name="Mesh"))
                self.assertEqual(rejected.exception.code,"BUDGET_EXCEEDED")
                self.assertEqual(runtime.resources.usage().plans,0)
                self.assertEqual(edit.meshes.get(mesh).source,source)
                meshing = meshes.Meshing((7,99,42), ())
                object.__setattr__(meshing, "boundary", OverBudgetTuple((7,99,42)))
                with self.assertRaises(zipzi.SdkFailure) as rejected:
                    edit.apply(meshes.Create(meshes.MeshSpec("Structured", replace(GEOMETRY, meshing=meshing))))
                self.assertEqual(rejected.exception.code, "BUDGET_EXCEEDED")
                self.assertEqual(edit.meshes.get(mesh).source, source)
                edit.meshes.rename(mesh,"Usable")
            self.assertEqual(session.meshes.get(mesh).value.name,"Usable")
            self.assertEqual(session.info().history,(1,0))

    def test_sdk_rectangle_generation_migrates_current_candidate_and_refuses_over_limit(self):
        corners=((0.,0.),(3.,0.),(3.,3.),(0.,3.))
        rectangle=meshes.Geometry((0,1,2,3),corners,((0.,0.),(1.,0.),(1.,1.),(0.,1.)),((0,1,2),(0,2,3)))
        for mode in ("local","connected"):
            with self.subTest(mode=mode),self.runtime(mode) as runtime:
                session=runtime.sessions.create(uid(1))
                with session.edit("compose then generate") as edit:
                    mesh=edit.meshes.create(meshes.MeshSpec("Rectangle",rectangle,id=uid(2)))
                    parameter=edit.parameters.create(parameters.ParameterSpec("P",0.,1.,0.,id=uid(3)))
                    binding=edit.bindings.create(bindings.MeshBindingSpec(mesh,(bindings.Axis(parameter,(0.,1.)),),(bindings.Keyform((0.,),corners),bindings.Keyform((1.,),tuple((x+2.,y) for x,y in corners))),id=uid(4)))
                    edit.apply(m.ModelingCommandRemeshRectangleGrid(id=mesh,columns=3,rows=3))
                    self.assertEqual(len(edit.meshes.get(mesh).value.geometry.vertex_ids),16)
                    self.assertEqual(len(edit.bindings.get(binding).value.keyforms[0].positions),16)
                    self.assertEqual(edit.modeling.sample(({parameter:.5},)).value.frames[0].drawables[0].positions[-1],(4.,3.))
                before=session.info()
                with self.assertRaises(zipzi.SdkFailure) as rejected:
                    with session.edit("outside limit") as edit:edit.apply(m.ModelingCommandRemeshRectangleGrid(id=mesh,columns=256,rows=256))
                self.assertEqual(rejected.exception.code,"INVALID_RECTANGLE_GRID")
                self.assertEqual(session.info(),before)
                session.undo().wait()
                self.assertEqual(session.info().history,(0,1))

    def test_binary_authoring_recovery_every_phase_keeps_single_sequence(self):
        content=bytes(range(256))*6000
        for phase,after in (("start_authoring_upload",0),("append_blob_binary",0),("append_blob_binary",1),("seal_blob",0),("apply_authoring_blob",0),("release_blob",0)):
            with self.subTest(phase=phase,after=after):
                runtime=self.runtime("connected",drop=phase,drop_after=after)
                session=runtime.sessions.create(uid(1))
                with session.edit("binary attachment") as edit:
                    edit.apply(m.ModelingCommandAttachment(intent=m.AttachmentEditCreate(id=uid(2),path="extras/data.bin")))
                    old=edit.modeling.records(m.ModelingQuery(attachments=(uid(2),))).value.attachments[0]
                    with self.assertRaises(zipzi.OutcomeUnknown) as unknown:
                        edit.apply(m.ModelingCommandAttachment(intent=m.AttachmentEditReplace(expected=old,attachment=replace(old,bytes=content))))
                    result=unknown.exception.recover()
                    self.assertIsInstance(result,zipzi.WriteResult)
                    self.assertEqual(result.source.sequence,2)
                    self.assertEqual(edit.modeling.records(m.ModelingQuery(attachments=(uid(2),))).value.attachments[0].bytes,content)
                self.assertEqual(session.info().history,(1,0))

    def test_failed_attached_binary_cleanup_recovers_original_error_and_reclaims_edit(self):
        content=b"x"*(2<<20)
        for cleanup in ("release_blob","release_edit"):
            with self.subTest(cleanup=cleanup):
                runtime=self.runtime("connected",drop=cleanup)
                session=runtime.sessions.create(uid(1))
                edit=session.edit("invalid binary content")
                # A stale expected attachment reaches the actual SDK validator.
                absent=m.PackageAttachment(id=uid(2),path="extras/data.bin",bytes=content)
                with self.assertRaises(zipzi.SdkFailure) as original:
                    edit.apply(m.ModelingCommandAttachment(intent=m.AttachmentEditReplace(expected=absent,attachment=absent)))
                self.assertNotIsInstance(original.exception,zipzi.OutcomeUnknown)
                self.assertIsNotNone(original.exception.cleanup)
                self.assertIsNone(original.exception.cleanup.recover())
                for _ in range(20):
                    candidate=session.edit("slots reclaimed")
                    candidate.cancel()
                self.assertEqual(session.info().history,(0,0))
                self.assertEqual(runtime.resources.usage().rgba_bytes,0)

    def test_a17_resident_distinct_cancel_history_eviction_and_snapshot_lease(self):
        for mode in ("local","connected"):
            with self.subTest(mode=mode):
                runtime=self.runtime(mode)
                session=runtime.sessions.create(uid(1))
                baseline=session.info()
                for n in range(100):
                    with runtime.resources.prepare_image(1,1,bytes((n,1,2,255))) as image:
                        edit=session.edit("cancel distinct image")
                        edit.resources.create_image("Draft",image,id=uid(2))
                        edit.cancel()
                    usage=runtime.resources.usage()
                    self.assertEqual((usage.rgba_bytes,usage.png_bytes,usage.blob_bytes,usage.preparing_bytes,usage.resource_entries,usage.allocations),(0,0,0,0,0,0))
                self.assertEqual(session.info(),baseline)
                with runtime.resources.prepare_image(1,1,bytes((0,1,2,255))) as image:
                    with session.edit("Original") as edit:edit.resources.create_image("Image",image,id=uid(2))
                pinned=session.resources.snapshot(uid(2))
                for n in range(1,101):
                    with runtime.resources.prepare_image(1,1,bytes((n,1,2,255))) as image:
                        with session.edit("replace distinct image") as edit:edit.resources.replace_image(uid(2),image)
                    self.assertEqual(pinned.pixels().rgba,bytes((0,1,2,255)))
                    usage=runtime.resources.usage()
                    self.assertLessEqual(usage.resource_entries,52)
                    self.assertLessEqual(usage.rgba_bytes,52*4)
                    self.assertLessEqual(usage.allocations,52)
                self.assertEqual(session.info().history,(50,0))
                session.undo().wait()
                self.assertEqual(session.resources.get(uid(2)).value.pixels.rgba,bytes((99,1,2,255)))
                session.redo().wait()
                before=runtime.resources.usage().rgba_bytes
                pinned.close()
                self.assertEqual(runtime.resources.usage().rgba_bytes,before-4)
                self.assertEqual(runtime.resources.usage().rgba_bytes,51*4)
                self.assertEqual(runtime.resources.usage().resource_entries,51)

    def test_png_preparation_jobs_cancel_and_lease_encoded_memory(self):
        path=ROOT/"fixtures/external_v50/texture_00.png"
        for mode in ("local","connected"):
            with self.subTest(mode=mode):
                runtime=self.runtime(mode)
                self.control(runtime,"pause_dispatch")
                job=runtime.resources.load_png(path)
                job.cancel()
                self.control(runtime,"resume_dispatch")
                with self.assertRaises(zipzi.SdkFailure) as rejected:job.wait()
                self.assertEqual(rejected.exception.code,"CANCELLED")
                self.assertEqual(runtime.resources.usage().allocations,0)
                with runtime.resources.load_png(path).wait() as disk:
                    pixels=disk.pixels()
                    with runtime.resources.prepare_buffer(path.read_bytes()) as buffer:
                        job=runtime.resources.prepare_png(buffer)
                    with job.wait() as memory:
                        self.assertEqual(memory.pixels(),pixels)
                    self.assertGreater(runtime.resources.usage().png_bytes,0)
                usage=runtime.resources.usage()
                self.assertEqual((usage.rgba_bytes,usage.png_bytes,usage.blob_bytes,usage.preparing_bytes,usage.allocations),(0,0,0,0,0))
                with runtime.resources.prepare_buffer(b"invalid PNG") as buffer:
                    with self.assertRaises(zipzi.SdkFailure):runtime.resources.prepare_png(buffer).wait()
                self.assertEqual(runtime.resources.usage().allocations,0)

    def test_headless_psd_build_save_replace_reopen_and_exports(self):
        for mode in ("local","connected"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as folder:
                runtime=self.runtime(mode)
                folder=Path(folder)
                session=runtime.sessions.import_(ROOT/"crates/zipzi-psd/tests/fixtures/layered.psd",m.ImportFormat.PSD).wait()
                info=session.info()
                records=session.modeling.records(m.ModelingQuery())
                self.assertTrue(records.value.ids["assets"])
                asset=records.value.ids["assets"][0]
                pixels=session.resources.get(asset).value.pixels.rgba
                self.assertTrue(pixels)
                with session.edit("headless build") as edit:
                    mesh=edit.meshes.create(meshes.MeshSpec("Constructed",GEOMETRY,texture_asset_id=asset,id=uid(2)))
                self.assertEqual(session.info().history,(1,0))
                before=session.info()
                for format,name in ((m.ExportFormat.CMO3,"model.cmo3"),(m.ExportFormat.CAN3,"model.can3"),(m.ExportFormat.CUBISM,"cubism"),(m.ExportFormat.CDI3,"model.cdi3.json")):
                    report=session.project.export(folder/name,format,allow_loss=True).wait()
                    self.assertTrue(report.published)
                    self.assertEqual(report.before,before.version)
                    self.assertTrue((folder/name).exists())
                    self.assertEqual(session.info().history,before.history)
                saved=session.project.save(folder/"project").wait()
                with session.reference("mesh",mesh) as reference:
                    report=session.project.open(saved.manifest).wait()
                    self.assertGreater(report.after.generation,report.before.generation)
                    with self.assertRaises(zipzi.SdkFailure):reference.resolve()
                self.assertEqual(session.resources.get(asset).value.pixels.rgba,pixels)
                self.assertEqual(session.info().history,(0,0))
                self.assertEqual(session.meshes.get(mesh).value.name,"Constructed")
                failed=session.project.replace(folder/"missing.psd",m.ImportFormat.PSD)
                before=session.info()
                with self.assertRaises(zipzi.SdkFailure):failed.wait()
                self.assertEqual(session.info().version,before.version)
                self.assertEqual(session.meshes.get(mesh).value.name,"Constructed")
                session.project.export(folder/"package",m.ExportFormat.PACKAGE).wait()
                self.assertTrue((folder/"package").is_dir())

    def test_large_binary_authoring_records_sampling_and_attachment(self):
        # SDK rectangle-grid helper's exact 65536-vertex boundary, with a
        # concrete keyform dependency. Attachment transfer/persistence has its
        # own recovery tests and p4_recipe.py scenario.
        n=256
        positions=tuple((float(x),float(y)) for y in range(n) for x in range(n))
        ids=tuple(range(n*n))
        triangles=tuple(t for y in range(n-1) for x in range(n-1) for t in ((y*n+x,y*n+x+1,(y+1)*n+x+1),(y*n+x,(y+1)*n+x+1,(y+1)*n+x)))
        geometry=meshes.Geometry(ids,positions,tuple((x/255.,y/255.) for x,y in positions),triangles)
        results=[]
        for mode in ("local","connected"):
            with self.subTest(mode=mode):
                runtime=self.runtime(mode)
                session=runtime.sessions.create(uid(1))
                with session.edit("65536 vertices") as edit:
                    mesh=edit.meshes.create(meshes.MeshSpec("Large",geometry,id=uid(2)))
                    parameter=edit.parameters.create(parameters.ParameterSpec("P",0.,1.,0.,id=uid(3)))
                    binding=edit.bindings.create(bindings.MeshBindingSpec(mesh,(bindings.Axis(parameter,(0.,1.)),),(bindings.Keyform((0.,),positions),bindings.Keyform((1.,),tuple((x+2.,y) for x,y in positions))),id=uid(4)))
                    self.assertEqual(edit.meshes.get(mesh).value.geometry.positions,geometry.positions)
                    value=edit.modeling.records(m.ModelingQuery(meshes=(mesh,),bindings=(binding,)))
                    self.assertEqual(len(value.value.meshes[0].base_positions),65536)
                    sample=edit.modeling.sample(({parameter:.5},))
                    self.assertEqual(sample.value.frames[0].drawables[0].positions[-1],(256.,255.))
                before=session.info()
                sampled=session.modeling.sample_async(({parameter:0.},{parameter:1.})).wait()
                self.assertEqual(sampled.value.frames[0].drawables[0].positions[-1],(255.,255.))
                self.assertEqual(sampled.value.frames[1].drawables[0].positions[-1],(257.,255.))
                self.assertEqual(session.info(),before)
                results.append(sampled.value.frames)
                self.assertLessEqual(runtime.resources.usage().blob_bytes,512<<20)
        self.assertEqual(results[0],results[1])

    def test_read_jobs_capture_sources_allow_publication_and_cancel_without_history(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                with session.edit("Mesh") as edit:
                    mesh = edit.meshes.create(meshes.MeshSpec("Mesh", GEOMETRY, id=uid(2)))
                before = session.info()
                self.control(runtime,"pause_dispatch")
                try:
                    sample_job = session.modeling.sample_async(({}, {}, {}))
                    plan_job = session.plan_async(m.ModelingCommandScene(intent=m.SceneEditPatch(id=mesh,
                        expected=m.SceneFieldValueLabel(value="Mesh"),value=m.SceneFieldValueLabel(value="Planned"),clear_glue_pairs=False)))
                    cancelled = session.modeling.sample_async(({},))
                    cancelled.cancel()
                    self.assertEqual(cancelled.state(),"cancelled")
                    with self.assertRaises(zipzi.SdkFailure) as stopped:
                        cancelled.wait()
                    self.assertEqual(stopped.exception.code,"CANCELLED")
                    self.assertEqual(session.info(),before)
                    edit = session.edit("Publish while read jobs are queued")
                    edit.apply(meshes.SetPositions(mesh, GEOMETRY.vertex_ids, tuple((x+5.,y) for x,y in GEOMETRY.positions)))
                    receipts, failures = [], []
                    def publish():
                        try: receipts.append(edit.commit())
                        except BaseException as error: failures.append(error)
                    thread = threading.Thread(target=publish)
                    thread.start()
                    deadline = time.monotonic()+5.
                    while session.info().state != "busy" and time.monotonic()<deadline:
                        time.sleep(.01)
                    self.assertEqual(session.info().state,"busy")
                finally:
                    self.control(runtime,"resume_dispatch")
                thread.join(5.)
                self.assertFalse(thread.is_alive())
                self.assertEqual(failures,[])
                self.assertEqual(len(receipts),1)
                receipts[0].wait()
                sampled = sample_job.wait()
                self.assertEqual(sampled.source.version,before.version)
                self.assertEqual(sampled.value.frames[0].drawables[0].positions[0],(0.,0.))
                self.assertEqual(session.meshes.get(mesh).value.geometry.positions[0],(5.,0.))
                with plan_job.wait() as plan:
                    self.assertEqual(plan.report.source.version,before.version)
                    with self.assertRaises(zipzi.SdkFailure) as stale:
                        with session.edit("Old plan") as edit:
                            edit.apply(plan)
                    self.assertEqual(stale.exception.code,"STALE_PLAN")
                self.assertEqual(session.info().history,(2,0))
                self.control(runtime,"panic_next")
                failed = session.modeling.sample_async(({},))
                with self.assertRaises(zipzi.SdkFailure) as panicked:
                    failed.wait()
                self.assertEqual(panicked.exception.code,"WORKER_PANIC")
                self.assertEqual(session.info().state,"ready")
                self.assertEqual(session.modeling.sample(({},)).value.frames[0].drawables[0].positions[0],(5.,0.))

    def test_pending_sample_poll_recovery_continues_waiting(self):
        runtime = self.runtime("connected", drop="job")
        session = runtime.sessions.create(uid(1))
        self.control(runtime, "pause_dispatch")
        job = session.modeling.sample_async(({},))
        with self.assertRaises(zipzi.OutcomeUnknown) as unknown:
            job.wait()
        # Resume through another connection: the original owner must first
        # recover its cached, still-pending poll before it can poll again.
        with zipzi.connect(endpoint=runtime._backend._endpoint) as peer:
            self.control(peer, "resume_dispatch")
        sampled = unknown.exception.recover()
        self.assertEqual(sampled.source.version, session.info().version)
        self.assertEqual(job.wait(), sampled)
        with self.assertRaises(zipzi.SdkFailure):
            unknown.exception.recover()

    def test_project_open_preserves_missing_resource_diagnostics(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                with runtime.resources.prepare_image(1, 1, b"rgba") as image:
                    with session.edit("Image") as edit:
                        asset = edit.resources.create_image("Texture", image, id=uid(2))
                path = self.folder / mode / "project.zipzi"
                path.parent.mkdir()
                session.project.save(path).wait()
                images = tuple(path.parent.rglob("*.png"))
                self.assertTrue(images)
                for image in images:
                    image.unlink()
                report = session.project.open(path).wait()
                self.assertTrue(report.published)
                self.assertTrue(any(d.object_id == asset for d in report.diagnostics))
                self.assertNotEqual(report.before, report.after)

    def test_read_job_start_and_result_response_recovery_keeps_original_sources(self):
        for operation in ("request_modeling_samples", "request_modeling_plan", "job"):
            with self.subTest(operation=operation):
                runtime = self.runtime("connected",drop=operation)
                session = runtime.sessions.create(uid(1))
                if operation == "request_modeling_plan":
                    with self.assertRaises(zipzi.OutcomeUnknown) as unknown:
                        session.plan_async(m.ModelingCommandDocument(intent=m.DocumentAuthoringEditPatch(
                            expected=m.DocumentValueLayout(value=None),value=m.DocumentValueLayout(value={"CenterX":2.}))))
                    job = unknown.exception.recover()
                    with job.wait() as plan:
                        self.assertEqual(plan.report.source.version,session.info().version)
                elif operation == "request_modeling_samples":
                    with self.assertRaises(zipzi.OutcomeUnknown) as unknown:
                        session.modeling.sample_async(({},))
                    job = unknown.exception.recover()
                    self.assertEqual(job.wait().source.version,session.info().version)
                else:
                    job = session.modeling.sample_async(({},))
                    with self.assertRaises(zipzi.OutcomeUnknown) as unknown:
                        job.wait()
                    sampled = unknown.exception.recover()
                    self.assertEqual(sampled.source.version,session.info().version)
                    self.assertEqual(job.wait(),sampled)
                self.assertEqual(session.info().history,(0,0))

    def test_all_modeling_address_domains_keep_sdk_epochs_after_undo(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                with runtime.resources.prepare_image(1,1,b"rgba") as pixels:
                    with session.edit("Address fixtures") as edit:
                        edit.apply(scene.CreatePart(scene.PartSpec("Part", id=uid(10))))
                        edit.apply(scene.CreateParameterGroup(scene.ParameterGroupSpec("Group", id=uid(11))))
                        edit.meshes.create(meshes.MeshSpec("Mesh",GEOMETRY,id=uid(2)))
                        edit.meshes.create(meshes.MeshSpec("Mask",GEOMETRY,id=uid(3)))
                        edit.resources.create_image("Tex",pixels,id=uid(5))
                        edit.parameters.create(parameters.ParameterSpec("P",-1.,1.,0.,id=uid(4)))
                        binding = edit.bindings.create(bindings.MeshBindingSpec(uid(2),(bindings.Axis(uid(4),(-1.,1.)),),(
                            bindings.Keyform((-1.,),GEOMETRY.positions),bindings.Keyform((1.,),GEOMETRY.positions)),id=uid(30)))
                        edit.apply(m.ModelingCommandAttachment(intent=m.AttachmentEditCreate(id=uid(70),path="extras/a.txt")))
                        edit.apply(m.ModelingCommandReferences(intent=m.ReferenceEditCreateModelHitArea(id=uid(80),name="Hit",before=None)))
                sample = session.bindings.get(binding).value.axes[0].samples[0].id
                cases = (
                    (m.AddressSelectorScene(id=uid(2),field=m.SceneField.ENABLED), m.AddressPatchScene(value=m.SceneFieldValueEnabled(value=False),clear_glue_pairs=False)),
                    (m.AddressSelectorDrawingGroup(owner=""),m.AddressPatchDrawingRange(range=(-100,100))),
                    (m.AddressSelectorBinding(id=sample),m.AddressPatchBinding(patch=m.BindingFieldPatchSampleValue(value=-.5))),
                    (m.AddressSelectorOrganization(id=uid(4)),m.AddressPatchOrganization(intent=m.OrganizationEditSetParameterGroup(parameter_id=uid(4),group_id=uid(11)))),
                    (m.AddressSelectorContent(id=uid(2),target=m.SceneContentTargetVertexUv(vertex_id=42)),m.AddressPatchContent(value=m.SceneContentValuePoint(value=m.Vec2(x=.25,y=.5)))),
                    (m.AddressSelectorReference(id=uid(80)),m.AddressPatchReference(fields=m.ReferenceFieldsModelHitArea(name="New hit",mesh=m.ModelTargetRefResolved(object_id=uid(3))))),
                    (m.AddressSelectorSelection(owner=uid(2),field=m.ReferenceSelectionField.MESH_MASKS),m.AddressPatchSelection(targets=(uid(3),))),
                    (m.AddressSelectorResourceName(id=uid(5)),m.AddressPatchResourceName(name="Renamed")),
                    (m.AddressSelectorAttachmentPath(id=uid(70)),m.AddressPatchAttachmentPath(path="extras/new.txt")),
                    (m.AddressSelectorDocument(field=m.DocumentField.LAYOUT),m.AddressPatchDocument(value=m.DocumentValueLayout(value={"CenterX":1.}))),
                )
                for selector,patch_value in cases:
                    with self.subTest(selector=selector):
                        before = session.info()
                        with session.address(selector) as address:
                            self.assertEqual(session.info(),before)
                            with address.plan(patch_value) as plan:
                                self.assertEqual(session.info(),before)
                                with session.edit("Patch through SDK address") as edit:
                                    edit.apply(plan)
                            session.undo().wait()
                            with self.assertRaises(zipzi.SdkFailure) as stale:
                                address.plan(patch_value)
                            self.assertIn(stale.exception.code, {"STALE_FIELD_ADDRESS", "STALE_FIELD", "STALE_OBJECT", "STALE_CONTENT_ADDRESS"})
                with session.reference("mesh",uid(2)) as reference:
                    caps = reference.capabilities(m.CapabilityDomain.CONTENT)
                    self.assertEqual(caps.source.version,session.info().version)
                    self.assertIsInstance(caps.value,m.CapabilitiesContent)
                    self.assertIsInstance(caps.value.value.topology,m.CapabilityStateAvailable)
                collection = session.capabilities(m.CollectionQueryResources())
                self.assertIsInstance(collection.value.value.create,m.CapabilityStateAvailable)

    def test_canonical_modeling_records_plans_and_bound_topology_recipe(self):
        results = []
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                with session.edit("Canonical modeling") as edit:
                    edit.apply(scene.CreatePart(scene.PartSpec("Root", id=uid(10))))
                    mesh = edit.meshes.create(meshes.MeshSpec("Mesh", GEOMETRY, id=uid(2)))
                    parameter = edit.parameters.create(parameters.ParameterSpec("P", -1., 1., 0., id=uid(4)))
                    binding = edit.bindings.create(bindings.MeshBindingSpec(mesh, (bindings.Axis(parameter, (-1., 1.)),), (
                        bindings.Keyform((-1.,), GEOMETRY.positions),
                        bindings.Keyform((1.,), tuple((x+10., y) for x,y in GEOMETRY.positions))), id=uid(30)))
                    batch = edit.modeling.records(m.ModelingQuery(meshes=(mesh,), bindings=(binding,), parts=(uid(10),), parameters=(parameter,)))
                    self.assertEqual(batch.value.meshes[0].vertex_ids, (42, 7, 99))
                    self.assertEqual(batch.value.bindings[0].mesh_id, mesh)
                    self.assertEqual(batch.value.meshes[0].raw_blend_mode, None)
                    self.assertEqual(batch.source.sequence, 4)
                    bound = batch.value.bindings[0]
                    form = replace(bound.keyforms[1], positions=tuple(m.Vec2(x=x+20., y=y) for x,y in GEOMETRY.positions))
                    edit.apply(m.ModelingCommandSetMeshKeyform(id=binding, keyform=form))
                    first, second = uid(61), uid(62)
                    axis = m.BindingAxis(id=uid(60), parameter_id=parameter, samples=(m.BindingSample(id=first, value=-1.), m.BindingSample(id=second, value=1.)))
                    scene_binding = m.SceneBinding(id=uid(63), axes=(axis,), track=m.SceneTrackPart(target_id=m.PartId(value=uid(10)), keyforms=(
                        m.PartKeyform(samples=(first,), draw_order=-2.), m.PartKeyform(samples=(second,), draw_order=2.))))
                    edit.apply(m.ModelingCommandCreateSceneBinding(binding=scene_binding))
                    with edit.plan(m.ModelingCommandDocument(intent=m.DocumentAuthoringEditPatch(
                        expected=m.DocumentValueLayout(value=None), value=m.DocumentValueLayout(value={"CenterX": 1.5})))) as plan:
                        edit.apply(plan)
                    edit.apply(m.ModelingCommandAttachment(intent=m.AttachmentEditCreate(id=uid(70), path="extras/note.txt")))
                    old = edit.modeling.records(m.ModelingQuery(attachments=(uid(70),))).value.attachments[0]
                    edit.apply(m.ModelingCommandAttachment(intent=m.AttachmentEditReplace(expected=old, attachment=replace(old, bytes=b"model attachment"))))
                    swept = edit.modeling.sample(({parameter:-1.}, {parameter:0.}, {parameter:1.}))
                    self.assertEqual([f.drawables[0].positions[0][0] for f in swept.value.frames], [0.,10.,20.])
                    final = edit.modeling.records(m.ModelingQuery(meshes=(mesh,), bindings=(binding,), scene_bindings=(uid(63),), attachments=(uid(70),)))
                    self.assertEqual(final.value.attachments[0].bytes, b"model attachment")
                    self.assertEqual(final.value.scene_bindings[0].track, scene_binding.track)
                self.assertEqual(session.info().history, (1,0))
                base = session.modeling.records(m.ModelingQuery(meshes=(mesh,),bindings=(binding,)))
                mesh_value, binding_value = base.value.meshes[0], base.value.bindings[0]
                # An explicit stable vertex map and all existing dependencies are
                # required by the SDK, even when only triangle winding changes.
                source = m.GeometrySnapshot(version=base.source.version, mesh_id=mesh, vertex_ids=mesh_value.vertex_ids,
                    positions=mesh_value.base_positions, uvs=mesh_value.uvs, triangles=mesh_value.triangles, space=m.GeometrySpaceModel())
                replacement = m.TopologyReplacement(mesh=replace(mesh_value, triangles=((99,7,42),)), binding=binding_value,
                    blend_bindings=(), glues=(), vertex_mapping={42:42,7:7,99:99})
                with session.edit("Bound topology") as edit:
                    edit.apply(m.ModelingCommandReplaceTopology(source=source,replacement=replacement))
                self.assertEqual(session.meshes.get(mesh).value.geometry.triangles, ((99,7,42),))
                self.assertEqual(session.info().history, (2,0))
                results.append((final.value, swept.value.frames))
                session.undo().wait()
                self.assertEqual(session.meshes.get(mesh).value.geometry.triangles, ((42,7,99),))
        # SDK-created sample IDs are intentionally independent per Runtime.
        self.assertEqual(results[0][0].meshes, results[1][0].meshes)
        self.assertEqual(results[0][1], results[1][1])

    def test_reusable_buffers_large_images_and_captured_binary_pixels(self):
        data = bytes((17, 38, 59, 255)) * (1024 * 513)
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                with runtime.resources.prepare_buffer(data) as buffer:
                    self.assertEqual(buffer.size, len(data))
                    self.assertEqual(buffer.read(), data)
                    self.assertEqual(buffer.read((1 << 20) - 3, 17), data[(1 << 20) - 3:(1 << 20) + 14])
                    with runtime.resources.prepare_image(1024, 513, buffer) as image:
                        with session.edit("binary pixels") as edit:
                            asset = edit.resources.create_image("Large", image)
                            snapshot = edit.resources.get(asset)
                            self.assertEqual(snapshot.value.pixels.rgba, data)
                        self.assertEqual(image.pixels().rgba, data)
                self.assertEqual(snapshot.value.pixels.rgba, data)
                self.assertEqual(session.resources.get(asset).value.pixels.rgba, data)
                self.assertEqual(session.info().history, (1, 0))
                with self.assertRaises(zipzi.SdkFailure):
                    buffer.read()

    def test_binary_image_transfer_recovers_every_phase_and_old_snapshot(self):
        data = bytes((19, 40, 61, 255)) * (257 * 257)
        for operation in ("create_blob", "append_blob_binary", "seal_blob", "prepare_image_blob", "release_blob"):
            with self.subTest(operation=operation):
                runtime = self.runtime("connected", drop=operation)
                with self.assertRaises(zipzi.OutcomeUnknown) as unknown:
                    runtime.resources.prepare_image(257, 257, data)
                image = unknown.exception.recover()
                self.assertIsInstance(image, zipzi.PreparedImage)
                self.assertEqual(image.pixels().rgba, data)
                image.close()
                self.assertEqual(runtime.resources.usage().prepared_images, 0)
        for operation in ("read_selection", "read_blob_binary", "release_blob"):
            with self.subTest(read=operation):
                runtime = self.runtime("connected", drop=operation, drop_after=1 if operation == "release_blob" else 0)
                session = runtime.sessions.create(uid(1))
                with runtime.resources.prepare_buffer(data) as buffer:
                    with runtime.resources.prepare_image(257, 257, buffer) as image:
                        with session.edit("create") as edit:
                            asset = edit.resources.create_image("Old", image)
                old_version = session.info().version
                with self.assertRaises(zipzi.OutcomeUnknown) as unknown:
                    session.resources.get(asset)
                # Another owner may not replace the in-flight read, even when
                # using the same facade collection object.
                with self.assertRaises(zipzi.Busy):
                    session.resources.get(uid(99))
                with zipzi.connect(endpoint=runtime._backend._endpoint) as peer:
                    shared = peer.sessions.attach(session.id)
                    with peer.resources.prepare_image(1, 1, b"new!") as replacement:
                        with shared.edit("Replace during interrupted read") as edit:
                            edit.resources.replace_image(asset, replacement)
                snapshot = unknown.exception.recover()
                self.assertEqual(snapshot.value.pixels.rgba, data)
                self.assertEqual(snapshot.value.name, "Old")
                self.assertEqual(snapshot.source.version, old_version)
                self.assertNotEqual(snapshot.source.version, session.info().version)
                self.assertEqual(session.resources.get(asset).value.pixels.rgba, b"new!")

    def test_failed_binary_preparation_retains_cleanup_recovery(self):
        runtime = self.runtime("connected", drop="release_blob")
        data = b"rgba" * (257 * 257)
        with self.assertRaises(zipzi.SdkFailure) as failed:
            runtime.resources.prepare_image(257, 256, data)
        self.assertEqual(failed.exception.code, "INVALID_IMAGE")
        self.assertIsInstance(failed.exception.cleanup, zipzi.OutcomeUnknown)
        failed.exception.cleanup.recover()
        self.assertEqual(runtime.resources.usage().preparing_bytes, 0)
        self.assertEqual(runtime.resources.usage().prepared_images, 0)

    def test_buffer_creation_response_recovery_and_foreign_owner_refusal(self):
        for operation in ("create_blob", "append_blob_binary", "seal_blob"):
            with self.subTest(operation=operation):
                runtime = self.runtime("connected", drop=operation)
                with self.assertRaises(zipzi.OutcomeUnknown) as unknown:
                    runtime.resources.prepare_buffer(b"immutable payload")
                buffer = unknown.exception.recover()
                self.assertIsInstance(buffer, zipzi.Buffer)
                self.assertEqual(buffer.read(), b"immutable payload")
                with self.assertRaises(zipzi.SdkFailure):
                    unknown.exception.recover()
                buffer.close()
        first = self.runtime("local")
        second = self.runtime("local")
        with first.resources.prepare_buffer(b"rgba") as buffer:
            with self.assertRaises(zipzi.SdkFailure) as foreign:
                second.resources.prepare_image(1, 1, buffer)
            self.assertEqual(foreign.exception.code, "WRONG_OWNER")

    def test_scene_organization_plan_and_metadata_recipe(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                with session.edit("Scene") as edit:
                    edit.apply_many((
                        scene.CreatePart(scene.PartSpec("Root", id=uid(10))),
                        scene.CreatePart(scene.PartSpec("Child", id=uid(11), parent_id=uid(10))),
                        scene.CreateParameterGroup(scene.ParameterGroupSpec("Controls", id=uid(12))),
                        parameters.Create(parameters.ParameterSpec("X", 0., 1., 0., id=uid(4))),
                        parameters.Create(parameters.ParameterSpec("Y", 0., 1., 0., id=uid(5))),
                        scene.MoveParameters((uid(5), uid(4)), group_id=uid(12)),
                        meshes.Create(meshes.MeshSpec("A", GEOMETRY, id=uid(2))),
                        meshes.Create(meshes.MeshSpec("B", GEOMETRY, id=uid(3))),
                        scene.MoveToPart((uid(2), uid(3)), part_id=uid(10)),
                        scene.CreateWarp("Warp", scene.Placement(part_id=uid(10)), (-1., -1., 2., 2.), id=uid(20)),
                        scene.CreateRotation("Rotation", scene.Placement(part_id=uid(10), parent_id=uid(20)), id=uid(21)),
                        scene.CreateGlue("Glue", uid(2), uid(3), id=uid(22)),
                        scene.CreateOffscreen("Composite", uid(10), id=uid(23)),
                        scene.InsertGluePair(uid(22), scene.GluePair(.5, .5, id=uid(24), vertex_a=42, vertex_b=7)),
                        scene.InsertOffscreenKeyform(uid(23), scene.OffscreenForm(1.)),
                        scene.PatchOffscreenKeyform(uid(23), 0, scene.OffscreenForm(.4)),
                        scene.SetOffscreenStaticKeyform(uid(23), 0),
                    ))
                    batch = edit.scene.get_many((uid(2), uid(10), uid(11), uid(12), uid(20), uid(21), uid(22), uid(23)))
                    self.assertEqual(batch.source.sequence, 1)
                    self.assertEqual(batch.items[0].part_id, uid(10))
                    self.assertEqual(batch.items[2].parent_id, uid(10))
                    self.assertEqual(batch.items[5].parent_id, uid(20))
                    self.assertEqual(tuple(v.object_kind for v in batch.items[4:]), ("transform", "transform", "glue", "offscreen"))
                    with edit.plan(scene.SetSceneLabel(uid(21), "Rotation", "Turn")) as plan:
                        edit.apply(plan)
                        self.assertIn(uid(21), plan.report.affected_ids)
                    self.assertEqual(edit.scene.get(uid(21)).value.name, "Turn")
                self.assertEqual(session.info().history, (1, 0))
                self.assertAlmostEqual(session.modeling.sample(({},)).value.frames[0].offscreens[0].opacity, .4)
                with session.plan(scene.DuplicateScene((uid(2),), False, part_id=uid(10))) as plan:
                    copies = dict(plan.report.copied_ids)
                    self.assertIn(uid(2), copies)
                    with session.edit("Copy") as edit:
                        edit.apply(plan)
                self.assertEqual(session.scene.get(copies[uid(2)]).value.part_id, uid(10))
                session.undo().wait()
                with self.assertRaises(zipzi.SdkFailure):
                    session.scene.get(copies[uid(2)])
                session.redo().wait()
                destination = self.folder / (mode + "-scene")
                session.project.save(destination).wait()
                before = session.scene.get_many((uid(20), uid(21), uid(22), uid(23))).items
                with zipzi.local() as reopened_runtime:
                    reopened = reopened_runtime.sessions.open(destination).wait()
                    self.assertEqual(reopened.scene.get_many((uid(20), uid(21), uid(22), uid(23))).items, before)

    def test_blendshape_bound_topology_and_candidate_cpu_sweep(self):
        results = []
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                with session.edit("Bound topology") as edit:
                    edit.apply_many((
                        meshes.Create(meshes.MeshSpec("Face", GEOMETRY, id=uid(2))),
                        parameters.Create(parameters.ParameterSpec("X", 0., 1., 0., id=uid(3))),
                        parameters.Create(parameters.ParameterSpec("Blend", 0., 1., 0., id=uid(4), kind=parameters.Kind.BLEND_SHAPE)),
                        scene.CreateRotation("Parent", scene.Placement(), id=uid(20)),
                    ))
                    edit.bindings.create(bindings.MeshBindingSpec(uid(2), (bindings.Axis(uid(3), (0., 1.)),), (
                        bindings.Keyform((0.,), GEOMETRY.positions),
                        bindings.Keyform((1.,), ((10., 0.), (11., 0.), (10., 1.))),
                    ), id=uid(5)))
                    edit.apply_many((
                        scene.CreateBlendTable(uid(4), (scene.Sample(uid(40), 0.), scene.Sample(uid(41), 1.)), uid(40), id=uid(30)),
                        scene.CreateMeshBlend(uid(2), uid(30), id=uid(31)),
                        scene.SetMeshBlendVertices(uid(2), uid(31), uid(41), tuple(scene.VertexPatch(v, position=(2., 0.)) for v in GEOMETRY.vertex_ids)),
                        scene.InsertMeshVertex(uid(2), 50, (.5, 0.), (.5, 0.), (scene.VertexWeight(42, .5), scene.VertexWeight(7, .5))),
                        scene.ReplaceTriangles(uid(2), ((42, 50, 99), (50, 7, 99))),
                        scene.ReorderMeshVertices(uid(2), (50, 99, 42, 7)),
                    ))
                    with edit.plan(scene.ReparentLocal((uid(2),), parent_id=uid(20))) as plan:
                        self.assertIsNotNone(plan.report.cost)
                        edit.apply(plan)
                    geometry = edit.meshes.get(uid(2))
                    self.assertEqual(geometry.value.geometry.vertex_ids, (50, 99, 42, 7))
                    self.assertEqual(geometry.value.geometry.triangles, ((42, 50, 99), (50, 7, 99)))
                    poses = tuple({uid(3): x, uid(4): x} for x in (0., .5, 1.))
                    batch = edit.modeling.sample(poses, expected=geometry.source)
                    xs = tuple(frame.drawables[0].positions[0][0] for frame in batch.value.frames)
                    self.assertEqual(xs, (.5, 6.5, 12.5))
                    self.assertEqual(edit.meshes.get(uid(2)).source, geometry.source)
                    self.assertEqual(session.info().history, (0, 0))
                before = session.info()
                formal = session.modeling.sample(poses)
                self.assertEqual(tuple(f.drawables[0].positions[0][0] for f in formal.value.frames), xs)
                self.assertEqual(session.info(), before)
                self.assertEqual(before.history, (1, 0))
                results.append((xs, geometry.value.geometry, tuple(d.kind for d in formal.value.diagnostics)))
                with self.assertRaises(zipzi.SdkFailure):
                    with session.edit("Invalid triangle") as edit:
                        edit.apply(scene.ReplaceTriangles(uid(2), ((42, 7, 1000),)))
                self.assertEqual(session.info().version, before.version)
                self.assertEqual(session.info().history, (1, 0))
        self.assertEqual(results[0], results[1])

    def test_scene_failure_and_query_admission_preserve_formal_history(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                edit = session.edit("Invalid parent")
                edit.apply(scene.CreatePart(scene.PartSpec("temporary", id=uid(10))))
                with self.assertRaises(zipzi.SdkFailure):
                    edit.apply(scene.CreateRotation("broken", scene.Placement(parent_id=uid(99))))
                with self.assertRaises(zipzi.SdkFailure):
                    edit.commit()
                self.assertEqual(session.info().history, (0, 0))
                with session.edit("Valid") as edit:
                    edit.meshes.create(meshes.MeshSpec("A", GEOMETRY, id=uid(2)))
                    with self.assertRaises(zipzi.SdkFailure) as refused:
                        edit.scene.get_many((uid(2),) * 257)
                    self.assertEqual(refused.exception.code, "BUDGET_EXCEEDED")
                    self.assertEqual(edit.scene.get(uid(2)).source.sequence, 1)
                self.assertEqual(session.info().history, (1, 0))

    def test_sdk_references_addresses_and_undo_invalidation(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                with session.edit("Create") as edit:
                    edit.meshes.create(meshes.MeshSpec("A", GEOMETRY, id=uid(2)))
                    edit.apply(scene.CreatePart(scene.PartSpec("Folder", id=uid(10))))
                with session.reference("mesh", uid(2)) as ref:
                    self.assertEqual(ref.id, uid(2))
                    self.assertEqual(ref.resolve().version, session.info().version)
                    self.assertEqual(ref.scene_capabilities().patch.state, "available")
                    self.assertEqual(ref.scene_capabilities().dissolve.state, "unsupported")
                    with ref.label() as address:
                        self.assertEqual(address.expected, "A")
                        with address.plan("Named") as plan:
                            with session.edit("Name") as edit:
                                edit.apply(plan)
                        self.assertEqual(session.scene.get(uid(2)).value.name, "Named")
                        with self.assertRaises(zipzi.SdkFailure) as stale:
                            address.plan("Again")
                        self.assertEqual(stale.exception.code, "STALE_FIELD_ADDRESS")
                    with ref.label() as address:
                        with session.edit("Move") as edit:
                            edit.apply(scene.MoveToPart((uid(2),), part_id=uid(10)))
                        ref.resolve()
                        with self.assertRaises(zipzi.SdkFailure) as stale:
                            address.plan("Moved")
                        self.assertEqual(stale.exception.code, "STALE_FIELD_ADDRESS")
                        session.undo().wait()
                        with self.assertRaises(zipzi.SdkFailure) as stale:
                            address.plan("Undone")
                        self.assertEqual(stale.exception.code, "STALE_FIELD_ADDRESS")
                    with session.edit("Delete") as edit:
                        edit.apply(scene.DeleteScene((uid(2),), False))
                    session.undo().wait()
                    with self.assertRaises(zipzi.SdkFailure) as stale:
                        ref.resolve()
                    self.assertEqual(stale.exception.code, "STALE_HANDLE")
                with session.reference("mesh", uid(2)) as fresh:
                    fresh.resolve()

    def test_reference_creation_and_label_plan_response_recovery(self):
        for operation in ("capture_object_ref", "capture_label_address", "plan_label_patch", "release_object_ref"):
            with self.subTest(operation=operation):
                runtime = self.runtime("connected", drop=operation)
                session = runtime.sessions.create(uid(1))
                with session.edit("Create") as edit:
                    edit.meshes.create(meshes.MeshSpec("A", GEOMETRY, id=uid(2)))
                if operation == "capture_object_ref":
                    with self.assertRaises(zipzi.OutcomeUnknown) as unknown:
                        session.reference("mesh", uid(2))
                    ref = unknown.exception.recover()
                else:
                    ref = session.reference("mesh", uid(2))
                if operation == "capture_label_address":
                    with self.assertRaises(zipzi.OutcomeUnknown) as unknown:
                        ref.label()
                    address = unknown.exception.recover()
                else:
                    address = ref.label()
                if operation == "plan_label_patch":
                    with self.assertRaises(zipzi.OutcomeUnknown) as unknown:
                        address.plan("B")
                    plan = unknown.exception.recover()
                else:
                    plan = address.plan("B")
                with plan:
                    with session.edit("Rename") as edit:
                        edit.apply(plan)
                self.assertEqual(session.info().history, (2, 0))
                self.assertEqual(session.scene.get(uid(2)).value.name, "B")
                address.close()
                if operation == "release_object_ref":
                    with self.assertRaises(zipzi.OutcomeUnknown) as unknown:
                        ref.close()
                    unknown.exception.recover()
                ref.close()

    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zzpy1-")
        self.addCleanup(self.temporary.cleanup)
        self.folder = Path(self.temporary.name)
        self.counter = 0

    def runtime(self, mode, *, drop=None, drop_after=0, settings=None, directory=None):
        if mode == "local":
            runtime = zipzi.local()
        else:
            self.counter += 1
            directory = directory or self.folder / str(self.counter)
            directory.mkdir(mode=0o700, exist_ok=True)
            env = dict(os.environ, ZIPZI_RUNTIME_DIR=str(directory), ZIPZI_RUNTIME_PROFILE="python2")
            env.update(settings or {})
            if drop:
                env["ZIPZI_TEST_DROP_RESPONSE"] = drop
                env["ZIPZI_TEST_DROP_RESPONSE_AFTER"] = str(drop_after)
            host = subprocess.Popen([os.environ["ZIPZI_PYTHON2_HOST"]], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.PIPE)
            def stop():
                host.terminate()
                host.wait(timeout=5)
                if host.stderr:
                    host.stderr.close()
            self.addCleanup(stop)
            self.stop_host = stop
            endpoint = runtime_endpoint(directory)
            deadline = time.monotonic() + 10
            while True:
                if host.poll() is not None:
                    self.fail(host.stderr.read().decode() if host.stderr else "Host failed")
                try:
                    runtime = zipzi.connect(endpoint=endpoint)
                    break
                except (FileNotFoundError, ConnectionRefusedError):
                    if time.monotonic() >= deadline:
                        self.fail("Host startup timed out")
                    time.sleep(.01)
        self.addCleanup(lambda: runtime.close(abandon_pending=True))
        return runtime

    def recipe(self, runtime, destination):
        session = runtime.sessions.create(uid(1))
        self.assertEqual(session.info().history, (0, 0))
        with session.edit("face") as edit:
            face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
            self.assertEqual(UUID(face).version, 4)
            snapshot = edit.meshes.get(face)
            self.assertEqual(snapshot.source.sequence, 1)
            self.assertIsNotNone(snapshot.source.edit)
            self.assertEqual(snapshot.value.geometry.vertex_ids, (42, 7, 99))
            with self.assertRaises(zipzi.SdkFailure) as error:
                session.meshes.get(face)
            self.assertEqual(error.exception.code, "MISSING_MESH")
            positions = ((2., 3.), (3., 3.), (2., 4.))
            result = edit.meshes.set_positions(face, vertex_ids=snapshot.value.geometry.vertex_ids, positions=positions)
            self.assertEqual(result.source.sequence, 2)
            self.assertEqual(result.source.edit, snapshot.source.edit)
            self.assertEqual(edit.meshes.get(face).value.geometry.positions, positions)
            self.assertEqual(session.info().history, (0, 0))
            with self.assertRaises(FrozenInstanceError):
                snapshot.value.name = "mutated"
        formal = session.meshes.get(face)
        self.assertEqual(formal.source.version, session.info().version)
        self.assertIsNone(formal.source.edit)
        self.assertEqual(formal.source.runtime, snapshot.source.runtime)
        self.assertEqual(formal.source.session, snapshot.source.session)
        self.assertEqual(session.info().history, (1, 0))
        with session.edit("batch") as edit:
            result = edit.apply_many((meshes.Create(meshes.MeshSpec("New", GEOMETRY, id=uid(2))),
                meshes.Rename(uid(2), "Batch"), meshes.Rename(face, "Final")))
            self.assertEqual(result.ids, (uid(2), uid(2), face))
            self.assertEqual(result.source.sequence, 1)
            self.assertEqual(edit.meshes.get(uid(2)).value.name, "Batch")
        self.assertEqual(session.info().history, (2, 0))
        self.assertTrue(session.undo().wait().changed)
        self.assertEqual(session.meshes.get(face).value.name, "Face")
        self.assertEqual(session.info().history, (1, 1))
        with session.edit("no op"):
            pass
        with session.edit("cancel") as edit:
            edit.meshes.rename(face, "Discard")
            edit.cancel()
        self.assertEqual(session.info().history, (1, 1))
        session.redo().wait()
        saved = session.project.save(destination).wait()
        self.assertTrue(saved.manifest.is_file())
        self.assertFalse(session.info().modified)
        return face, saved.manifest, (session.meshes.get(face).value.name, formal.value.geometry.positions, session.info().history)

    def test_session_listing_identity_and_attach_use_the_same_runtime(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(711))
                info, = runtime.sessions.list()
                self.assertEqual(info.id, session.id)
                self.assertEqual(info.state, "ready")
                self.assertIsNone(info.path)
                attached = runtime.sessions.attach(info.id)
                with attached.edit("same session") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("Resident", GEOMETRY))
                self.assertEqual(session.meshes.get(face).value.name, "Resident")
                self.assertEqual(session.info().history, (1, 0))
                self.assertEqual(session.info().id, info.id)
                self.assertEqual(session.meshes.get(face).source.runtime, info.id.runtime)
                with zipzi.local() as foreign:
                    with self.assertRaises(zipzi.SdkFailure) as error:
                        foreign.sessions.attach(info.id)
                    self.assertEqual(error.exception.code, "INVALID_REFERENCE")
                if mode == "local":
                    self.assertIsNone(runtime.connection)
                else:
                    self.assertEqual(runtime.connection.runtime, info.id.runtime)

    def binding_spec(self, shift=10.):
        return bindings.MeshBindingSpec(uid(2), (bindings.Axis(uid(3), (0., 1.)),), (
            bindings.Keyform((0.,), GEOMETRY.positions),
            bindings.Keyform((1.,), tuple((x+shift, y) for x, y in GEOMETRY.positions))), id=uid(4))

    def create_bound_mesh(self, edit):
        return edit.apply_many((meshes.Create(meshes.MeshSpec("Face", GEOMETRY, id=uid(2))),
            parameters.Create(parameters.ParameterSpec("Open", 0., 1., 0., id=uid(3))),
            bindings.Create(self.binding_spec())))

    def test_mesh_binding_cpu_sweep_uses_exact_candidate_and_formal_sources(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode), self.runtime(mode) as runtime:
                session = runtime.sessions.create(uid(1))
                stream = session.subscribe()
                with session.edit("Bound face") as edit:
                    result = self.create_bound_mesh(edit)
                    self.assertEqual(result.ids, (uid(2), uid(3), uid(4)))
                    self.assertEqual(result.source.sequence, 1)
                    binding = edit.bindings.get(uid(4))
                    self.assertEqual(binding.source, result.source)
                    self.assertEqual(binding.value.axes[0].parameter_id, uid(3))
                    self.assertEqual(tuple(s.value for s in binding.value.axes[0].samples), (0., 1.))
                    for identity in (binding.value.axes[0].id, *(s.id for s in binding.value.axes[0].samples)):
                        self.assertEqual(UUID(identity).version, 4)
                    self.assertEqual(binding.value.keyforms[1].samples, (binding.value.axes[0].samples[1].id,))
                    self.assertEqual(binding.value.keyforms[1].appearance, bindings.Appearance(1., (1., 1., 1.), (0., 0., 0.)))
                    queried = edit.bindings.get_many((uid(4), uid(4)))
                    self.assertEqual(queried.items, (binding.value, binding.value))
                    self.assertEqual(tuple(item.id for item in edit.bindings.list().items), (binding.value.id,))
                    poses = [{uid(3): x} for x in (0., .5, 1., 2.)]
                    sampled = edit.modeling.sample(poses, expected=result.source, checks=sampling.Checks(.75))
                    poses[0][uid(3)] = 1.
                    self.assertEqual(sampled.source, result.source)
                    self.assertEqual(sampled.value.version, result.source.version)
                    self.assertEqual(sampled.value.state, "ready")
                    self.assertEqual([f.drawables[0].positions[0][0] for f in sampled.value.frames], [0., 5., 10., 10.])
                    self.assertEqual(sampled.value.parameters[0].values, ((uid(3), 0.),))
                    self.assertTrue(sampled.value.frames[-1].parameters[0].clamped)
                    self.assertEqual(sampled.value.frames[-1].parameters[0].value, 1.)
                    self.assertEqual(sampled.value.diagnostics, (sampling.Diagnostic(sampling.DiagnosticKind.SMALL_TRIANGLE, uid(2), 0),))
                    self.assertEqual(sampled.value.frames[0].drawables[0].indices, (0, 1, 2))
                    self.assertEqual(sampled.value.frames[0].drawables[0].uvs, GEOMETRY.uvs)
                    self.assertEqual(sampled.value.frames[0].render_plan, ())
                    self.assertFalse(sampled.value.frames[0].drawables[0].renderable)
                    self.assertEqual(sampled.value.frames[0].deformers, ())
                    self.assertEqual(sampled.value.frames[0].offscreens, ())
                    with self.assertRaises(FrozenInstanceError):
                        sampled.value.frames[0].drawables[0].opacity = .5
                    self.assertEqual(session.info().history, (0, 0))
                    self.assertEqual(session.modeling.sample(({},)).value.frames[0].drawables, ())
                    colored = replace(self.binding_spec(20.).keyforms[1], opacity=.5,
                        multiply=(.25,.5,1.), screen=(.5,.25,.75), draw_order=12.)
                    changed = edit.bindings.set_keyform(uid(4), colored)
                    self.assertEqual(edit.bindings.get(uid(4)).value.keyforms[1].appearance,
                        bindings.Appearance(.5,(.25,.5,1.),(.5,.25,.75)))
                    posed = edit.modeling.sample(({uid(3):1.},)).value.frames[0].drawables[0]
                    self.assertEqual(posed.opacity,.5)
                    self.assertEqual(posed.multiply_color[:3],(.25,.5,1.))
                    self.assertEqual(posed.screen_color[:3],(.5,.25,.75))
                    self.assertEqual(posed.draw_order,12)
                    self.assertEqual(changed.source.sequence, 2)
                    self.assertEqual(edit.bindings.get(uid(4)).value.axes, binding.value.axes)
                    with self.assertRaises(zipzi.SdkFailure) as stale:
                        edit.modeling.sample(({uid(3): .5},), expected=result.source)
                    self.assertEqual(stale.exception.code, "INVALID_REFERENCE")
                    self.assertEqual(edit.modeling.sample(({uid(3): .5},)).value.frames[0].drawables[0].positions[0][0], 10.)
                self.assertEqual(session.info().history, (1, 0))
                self.assertEqual(len([e for e in stream.poll() if isinstance(e, events.Published)]), 1)
                before = session.info()
                formal = session.modeling.sample(({uid(3): .25},))
                self.assertIsNone(formal.source.edit)
                self.assertEqual(formal.value.frames[0].drawables[0].positions[0][0], 5.)
                self.assertEqual(session.info(), before)
                self.assertEqual(stream.poll(), ())
                session.undo().wait()
                self.assertEqual(session.modeling.sample(({},)).value.frames[0].drawables, ())
                self.assertEqual(session.bindings.list().items, ())
                session.redo().wait()
                self.assertEqual(session.bindings.get(uid(4)).value.axes, binding.value.axes)
                saved = session.project.save(self.folder / (mode+"-binding")).wait()
                with self.runtime(mode) as fresh:
                    reopened = fresh.sessions.open(saved.manifest).wait()
                    self.assertEqual(reopened.bindings.get(uid(4)).value.axes, binding.value.axes)
                    self.assertEqual(reopened.modeling.sample(({uid(3): .5},)).value.frames[0].drawables[0].positions[0][0], 10.)

    def test_binding_failure_aborts_mixed_batch_and_noop_preserves_redo(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode), self.runtime(mode) as runtime:
                session = runtime.sessions.create(uid(1))
                with session.edit("base") as edit:
                    self.create_bound_mesh(edit)
                with session.edit("shift") as edit:
                    edit.bindings.set_keyform(uid(4), self.binding_spec(20.).keyforms[1])
                session.undo().wait()
                version = session.info().version
                with session.edit("noop") as edit:
                    edit.bindings.set_keyform(uid(4), self.binding_spec().keyforms[1])
                self.assertEqual(session.info().history, (1, 1))
                self.assertEqual(session.info().version, version)
                with self.assertRaises(zipzi.SdkFailure):
                    with session.edit("caught") as edit:
                        edit.meshes.rename(uid(2), "Never publish")
                        with self.assertRaises(zipzi.SdkFailure):
                            edit.bindings.set_keyform(uid(4), bindings.Keyform((1.,), ((0., 0.),)))
                self.assertEqual(session.meshes.get(uid(2)).value.name, "Face")
                self.assertEqual(session.info().history, (1, 1))
                with self.assertRaises(zipzi.SdkFailure):
                    with session.edit("invalid grid") as edit:
                        spec = replace(self.binding_spec(), id=uid(5), axes=(bindings.Axis(uid(3), (0., 0.)),))
                        edit.bindings.create(spec)
                self.assertEqual(len(session.bindings.list().items), 1)
                with self.assertRaises(zipzi.SdkFailure):
                    with session.edit("caught encoding") as edit:
                        with self.assertRaises(ValueError):
                            edit.bindings.set_keyform(uid(4), bindings.Keyform((1.,), ((float("inf"), 0.),)))
                self.assertEqual(session.info().version, version)

    def test_modeling_read_limits_and_invalid_checks_do_not_poison_candidate(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode), self.runtime(mode) as runtime:
                session = runtime.sessions.create(uid(1))
                with session.edit("bounded") as edit:
                    result = self.create_bound_mesh(edit)
                    for call, code in (
                        (lambda: edit.modeling.sample(({},)*65), "BUDGET_EXCEEDED"),
                        (lambda: edit.modeling.sample(({uid(i): 0. for i in range(257)},)), "BUDGET_EXCEEDED"),
                        (lambda: edit.bindings.get_many((uid(4),)*257), "BUDGET_EXCEEDED"),
                        (lambda: edit.bindings.get(uid(99)), "MISSING_BINDING"),
                        (lambda: edit.modeling.sample(({},), checks=sampling.Checks(-1.)), "INVALID_DIAGNOSTIC_OPTIONS")):
                        with self.assertRaises(zipzi.SdkFailure) as rejected:
                            call()
                        self.assertEqual(rejected.exception.code, code)
                    self.assertEqual(len(edit.bindings.get_many((uid(4),)*256).items),256)
                    self.assertEqual(edit.bindings.get(uid(4)).source, result.source)
                    self.assertEqual(edit.modeling.sample(()).value.frames, ())
                    self.assertEqual(edit.modeling.sample(({},)).source, result.source)
                self.assertEqual(session.info().history, (1, 0))

    def test_binding_apply_and_read_response_losses_keep_original_owners(self):
        for dropped in ("apply_authoring", "read_selection", "sample_modeling"):
            with self.subTest(dropped=dropped), self.runtime("connected", drop=dropped) as runtime:
                session = runtime.sessions.create(uid(1))
                with session.edit("recover binding") as edit:
                    if dropped == "apply_authoring":
                        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                            self.create_bound_mesh(edit)
                        result = lost.exception.recover()
                        self.assertIsInstance(result, zipzi.WriteResult)
                        self.assertEqual(result.source.sequence, 1)
                    else:
                        self.create_bound_mesh(edit)
                    if dropped == "read_selection":
                        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                            edit.bindings.get(uid(4))
                        binding = lost.exception.recover()
                        self.assertIsInstance(binding, zipzi.Snapshot)
                        self.assertEqual(binding.source.sequence, 1)
                    if dropped == "sample_modeling":
                        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                            edit.modeling.sample(({uid(3): .5},))
                        sampled = lost.exception.recover()
                        self.assertIsInstance(sampled, zipzi.Snapshot)
                        self.assertEqual(sampled.value.frames[0].drawables[0].positions[0][0], 5.)
                self.assertEqual(session.info().history, (1, 0))
                self.assertEqual(session.modeling.sample(({uid(3): .5},)).value.frames[0].drawables[0].positions[0][0], 5.)

    def test_lost_cpu_sample_retains_old_frame_after_peer_publication(self):
        with self.runtime("connected", drop="sample_modeling") as runtime:
            session = runtime.sessions.create(uid(1))
            with session.edit("first") as edit:
                self.create_bound_mesh(edit)
            old = session.info().version
            with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                session.modeling.sample(({uid(3): .5},))
            with zipzi.connect(endpoint=runtime._backend._endpoint) as peer:
                attached = peer.sessions.attach(session.id)
                with attached.edit("later") as edit:
                    edit.bindings.set_keyform(uid(4), self.binding_spec(20.).keyforms[1])
            restored = lost.exception.recover()
            self.assertIsInstance(restored, zipzi.Snapshot)
            self.assertEqual(restored.value.version, old)
            self.assertEqual(restored.value.frames[0].drawables[0].positions[0][0], 5.)
            self.assertEqual(session.modeling.sample(({uid(3): .5},)).value.frames[0].drawables[0].positions[0][0], 10.)
            self.assertEqual(session.info().history, (2, 0))

    def test_typed_parameters_share_a_mixed_batch_and_preserve_unspecified_fields(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(801))
                spec = parameters.ParameterSpec("Angle", -30., 30., 0., id=uid(802), runtime_id="ParamAngle",
                    kind=parameters.Kind.BLEND_SHAPE, decimal_places=2, repeat=True)
                with session.edit("mesh and parameters") as edit:
                    result = edit.apply_many((meshes.Create(meshes.MeshSpec("Face", GEOMETRY, id=uid(803))),
                        parameters.Create(spec), parameters.Patch(uid(802), parameters.ParameterPatch(name="Final")), meshes.Rename(uid(803), "Mixed")))
                    self.assertEqual(result.ids, (uid(803), uid(802), uid(802), uid(803)))
                    batch = edit.parameters.get_many((uid(802), uid(802)))
                    old = batch.items[0]
                    self.assertEqual(batch.source, result.source)
                    self.assertEqual(batch.source.sequence, 1)
                    self.assertTrue(all(isinstance(item, parameters.Parameter) and not hasattr(item, "source") for item in batch.items))
                    self.assertEqual((old.name, old.runtime_id, old.kind, old.decimal_places, old.repeat),
                        ("Final", "ParamAngle", parameters.Kind.BLEND_SHAPE, 2, True))
                    edit.parameters.patch(uid(802), parameters.ParameterPatch(default_value=5., repeat=False, decimal_places=0))
                    changed = edit.parameters.get(uid(802))
                    self.assertEqual((changed.value.runtime_id, changed.value.kind, changed.value.decimal_places, changed.value.repeat, changed.value.default_value),
                        ("ParamAngle", parameters.Kind.BLEND_SHAPE, 0, False, 5.))
                    self.assertEqual(changed.source.sequence, 2)
                    normal = edit.parameters.create(parameters.ParameterSpec("Normal", -1., 1., 0.))
                    self.assertEqual(UUID(normal).version, 4)
                    value = edit.parameters.get(normal)
                    self.assertEqual((value.value.runtime_id, value.value.kind, value.value.decimal_places, value.value.repeat),
                        (normal, parameters.Kind.NORMAL, 6, False))
                    self.assertEqual(tuple(p.id for p in edit.parameters.list().items), (uid(802), normal))
                    self.assertEqual(edit.parameters.get_many(()).items, ())
                self.assertEqual(session.info().history, (1, 0))
                formal = session.parameters.get(uid(802))
                self.assertEqual(formal.source.version, session.info().version)
                self.assertIsNone(formal.source.edit)
                self.assertEqual(old.default_value, 0.)
                self.assertTrue(old.repeat)
                with self.assertRaises(FrozenInstanceError):
                    formal.value.name = "mutable"
                self.assertEqual(session.meshes.get(uid(803)).value.name, "Mixed")

    def test_parameter_query_admission_and_missing_records_do_not_poison_candidate(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                session = self.runtime(mode).sessions.create(uid(811))
                with session.edit("bounded reads") as edit:
                    id = edit.parameters.create(parameters.ParameterSpec("A", -1., 1., 0.))
                    source = edit.parameters.get(id).source
                    with self.assertRaises(zipzi.SdkFailure) as missing:
                        edit.parameters.get(uid(999))
                    self.assertEqual(missing.exception.code, "MISSING_PARAMETER")
                    self.assertEqual(missing.exception.object_ids, (uid(999),))
                    with self.assertRaises(zipzi.SdkFailure) as refused:
                        edit.parameters.get_many((id,) * 257)
                    self.assertEqual(refused.exception.code, "BUDGET_EXCEEDED")
                    self.assertEqual(edit.parameters.get(id).source, source)
                    edit.parameters.patch(id, parameters.ParameterPatch(name="Still editable"))
                self.assertEqual(session.parameters.get(id).value.name, "Still editable")
                self.assertEqual(session.info().history, (1, 0))

    def test_parameter_errors_abort_and_noop_cancel_keep_redo(self):
        for mode in ("local", "connected"):
            session = self.runtime(mode).sessions.create(uid(821))
            for bad in (parameters.ParameterPatch(minimum=2.), parameters.ParameterPatch(decimal_places=10),
                parameters.ParameterPatch(default_value=float("nan")), parameters.ParameterPatch(repeat=1)):
                with self.subTest(mode=mode, bad=bad):
                    with self.assertRaises(zipzi.SdkFailure) as error:
                        with session.edit("caught parameter error") as edit:
                            id = edit.parameters.create(parameters.ParameterSpec("Unpublished", -1., 1., 0.))
                            try:
                                edit.parameters.patch(id, bad)
                            except (zipzi.SdkFailure, TypeError, ValueError):
                                pass
                    self.assertEqual(error.exception.code, "EDIT_ABORTED")
                    self.assertEqual(session.parameters.list().items, ())
                    self.assertEqual(session.info().history, (0, 0))
            with session.edit("base") as edit:
                id = edit.parameters.create(parameters.ParameterSpec("Base", -1., 1., 0.))
            with session.edit("change") as edit:
                edit.parameters.patch(id, parameters.ParameterPatch(name="Changed"))
            session.undo().wait()
            with session.edit("no-op") as edit:
                edit.parameters.patch(id, parameters.ParameterPatch(name="Base"))
            self.assertEqual(session.info().history, (1, 1))
            with session.edit("cancel") as edit:
                edit.parameters.patch(id, parameters.ParameterPatch(name="Cancelled"))
                edit.cancel()
            self.assertEqual(session.info().history, (1, 1))
            session.redo().wait()
            self.assertEqual(session.parameters.get(id).value.name, "Changed")

    def test_event_subscriptions_are_paged_immutable_independent_and_source_exact(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                registry = runtime.sessions.subscribe()
                other_registry = runtime.sessions.subscribe()
                self.assertEqual(registry.initial, ())
                session = runtime.sessions.create(uid(831))
                created, = registry.poll(limit=1)
                self.assertIsInstance(created, events.Created)
                self.assertEqual(created.info.id, session.id)
                self.assertEqual(other_registry.poll(limit=1), (created,))
                first, second = session.subscribe(), session.subscribe()
                self.assertEqual(first.initial.version, session.info().version)
                self.assertEqual(first.cursor, second.cursor)
                with session.edit("publication") as edit:
                    id = edit.parameters.create(parameters.ParameterSpec("Angle", -1., 1., 0.))
                    self.assertEqual(first.poll(), ())
                delivered = []
                while page := first.poll(limit=1):
                    delivered.extend(page)
                self.assertEqual(second.poll(), tuple(delivered))
                published = [e for e in delivered if isinstance(e, events.Published)]
                self.assertEqual(len(published), 1)
                event = published[0]
                self.assertEqual(event.receipt.before, first.initial.version)
                self.assertEqual(event.receipt.after, session.info().version)
                self.assertIn(id, event.receipt.object_ids)
                self.assertEqual(event.cursor.session, session.id)
                self.assertEqual(event.operation.runtime, session.id.runtime)
                self.assertEqual(event.operation.serial, event.job.serial)
                with self.assertRaises(FrozenInstanceError):
                    event.receipt.label = "mutable"
                saved = session.project.save(self.folder / f"event-save-{mode}").wait()
                save_events = [e for e in first.poll() if isinstance(e, events.Saved)]
                self.assertEqual([e.receipt for e in save_events], [saved])
                self.assertEqual(first.resync(), session.info())
                self.assertEqual(first.poll(), ())
                with self.assertRaises(zipzi.SdkFailure) as future:
                    runtime.events_since(replace(first.cursor, sequence=first.cursor.sequence+100))
                self.assertEqual(future.exception.code, "RESYNC_REQUIRED")
                with zipzi.local() as foreign:
                    other = foreign.sessions.create(uid(832))
                    for cursor in (foreign.sessions.subscribe().cursor, other.subscribe().cursor):
                        with self.assertRaises(zipzi.SdkFailure) as wrong:
                            runtime.events_since(cursor)
                        self.assertEqual(wrong.exception.code, "RESYNC_REQUIRED")
                self.assertEqual(registry.resync(), runtime.sessions.list())

    def test_event_retention_requires_explicit_resync_and_rejects_bad_page_limits(self):
        runtime = self.runtime("connected", settings={"ZIPZI_TEST_EVENT_COUNT": "2"})
        session = runtime.sessions.create(uid(841))
        subscription = session.subscribe()
        before = subscription.cursor
        with session.edit("overflow events") as edit:
            edit.parameters.create(parameters.ParameterSpec("Angle", -1., 1., 0.))
        with self.assertRaises(zipzi.SdkFailure) as expired:
            subscription.poll()
        self.assertEqual(expired.exception.code, "RESYNC_REQUIRED")
        self.assertEqual(subscription.cursor, before)
        self.assertEqual(subscription.resync().history, (1, 0))
        self.assertEqual(subscription.poll(), ())
        for limit in (0, 257, True):
            with self.assertRaises(ValueError):
                subscription.poll(limit=limit)

    def test_lost_subscription_poll_and_parameter_reads_keep_exact_recovery_owner(self):
        for registry in (False, True):
            with self.subTest(registry=registry):
                runtime = self.runtime("connected", drop="subscribe")
                session = runtime.sessions.create(uid(851))
                with self.assertRaises(zipzi.OutcomeUnknown) as error:
                    runtime.sessions.subscribe() if registry else session.subscribe()
                subscription = error.exception.recover()
                self.assertIsInstance(subscription, events.RegistrySubscription if registry else events.SessionSubscription)
                self.assertEqual(subscription.poll(), ())
        runtime = self.runtime("connected", drop="events")
        session = runtime.sessions.create(uid(852))
        stream = session.subscribe()
        with session.edit("first") as edit:
            id = edit.parameters.create(parameters.ParameterSpec("First", -1., 1., 0.))
        before = stream.cursor
        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
            stream.poll()
        self.assertEqual(stream.cursor, before)
        with self.assertRaises(zipzi.Busy):
            stream.resync()
        with zipzi.connect(endpoint=runtime_endpoint(self.folder / str(self.counter))) as peer:
            same = peer.sessions.attach(session.id)
            with same.edit("second") as edit:
                edit.parameters.patch(id, parameters.ParameterPatch(name="Second"))
        recovered = lost.exception.recover()
        first_receipts = [e.receipt for e in recovered if isinstance(e, events.Published)]
        self.assertEqual([r.after.revision for r in first_receipts], [1])
        second_receipts = [e.receipt for e in stream.poll() if isinstance(e, events.Published)]
        self.assertEqual([r.after.revision for r in second_receipts], [2])
        with self.assertRaises(zipzi.SdkFailure) as resolved:
            lost.exception.recover()
        self.assertEqual(resolved.exception.code, "NO_PENDING_REQUEST")
        for many in (False, True):
            runtime = self.runtime("connected", drop="read_selection")
            session = runtime.sessions.create(uid(853))
            with session.edit("parameter") as edit:
                id = edit.parameters.create(parameters.ParameterSpec("Read", -1., 1., 0.))
            with self.assertRaises(zipzi.OutcomeUnknown) as error:
                session.parameters.get_many((id,)) if many else session.parameters.get(id)
            result = error.exception.recover()
            self.assertEqual(result.items[0].name if many else result.value.name, "Read")

    def test_parameter_batch_response_loss_commits_once(self):
        runtime = self.runtime("connected", drop="apply_authoring")
        session = runtime.sessions.create(uid(861))
        edit = session.edit("parameter recovery")
        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
            edit.apply_many((parameters.Create(parameters.ParameterSpec("Once", -1., 1., 0., id=uid(862))),
                parameters.Patch(uid(862), parameters.ParameterPatch(name="Final"))))
        result = lost.exception.recover()
        self.assertEqual(result.source.sequence, 1)
        self.assertEqual(result.ids, (uid(862), uid(862)))
        edit.commit().wait()
        self.assertEqual(session.parameters.get(uid(862)).value.name, "Final")
        self.assertEqual(session.info().history, (1, 0))

    def test_mesh_batch_admits_the_whole_selection_and_keeps_source_and_order(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(871))
                with session.edit("batch reads") as edit:
                    a = edit.meshes.create(meshes.MeshSpec("A", GEOMETRY))
                    b = edit.meshes.create(meshes.MeshSpec("B", GEOMETRY))
                    values = edit.meshes.get_many((b, a, b))
                    self.assertEqual(tuple(v.name for v in values.items), ("B", "A", "B"))
                    self.assertTrue(all(isinstance(v, meshes.Mesh) and not hasattr(v, "source") for v in values.items))
                    self.assertEqual(values.source.sequence, 2)
                    self.assertEqual(edit.meshes.get_many(()).items, ())
                    self.assertEqual(len(edit.meshes.get_many((a,)*256).items),256)
                    for ids in ((a,) * 257, (a, uid(999))):
                        with self.assertRaises(zipzi.SdkFailure):
                            edit.meshes.get_many(ids)
                    self.assertEqual(edit.meshes.get(a).source, values.source)
                    edit.meshes.rename(a, "After read refusal")
                self.assertEqual(session.info().history, (1, 0))
                self.assertEqual(session.meshes.get_many((a, b)).items[0].name, "After read refusal")
        runtime = self.runtime("connected", drop="read_selection")
        session = runtime.sessions.create(uid(872))
        with session.edit("read recovery") as edit:
            id = edit.meshes.create(meshes.MeshSpec("Recovered", GEOMETRY))
        with self.assertRaises(zipzi.OutcomeUnknown) as error:
            session.meshes.get_many((id,))
        self.assertEqual(error.exception.recover().items[0].name, "Recovered")

    def test_shared_recipe(self):
        outputs = []
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                face, manifest, result = self.recipe(runtime, self.folder / mode)
                runtime.close()
                if mode == "connected":
                    # Disconnect retains sessions; stop this fixture explicitly
                    # to release its project lock before a fresh disk open.
                    self.stop_host()
                reopened = self.runtime(mode).sessions.open(manifest).wait()
                self.assertEqual(reopened.meshes.get(face).value.name, "Final")
                self.assertEqual(reopened.meshes.get(face).value.geometry.positions, ((2., 3.), (3., 3.), (2., 4.)))
                self.assertEqual(reopened.info().history, (0, 0))
                self.assertFalse(reopened.info().modified)
                outputs.append(result)
        self.assertEqual(outputs[0], outputs[1])

    def test_caught_domain_and_conversion_errors_poison_the_entire_candidate(self):
        for mode in ("local", "connected"):
            runtime = self.runtime(mode)
            session = runtime.sessions.create(uid(1))
            for failure in ("domain", "encoding", "nan"):
                with self.subTest(mode=mode, failure=failure):
                    with self.assertRaises(zipzi.SdkFailure) as error:
                        with session.edit("fail") as edit:
                            edit.meshes.create(meshes.MeshSpec("Draft", GEOMETRY, id=uid(2)))
                            try:
                                if failure == "domain":
                                    edit.apply_many((meshes.Rename(uid(2), "Partial"), meshes.Rename(uid(999), "Missing")))
                                else:
                                    edit.meshes.set_positions(uid(2), vertex_ids=(2**40 if failure == "encoding" else 42,),
                                        positions=((float("nan") if failure == "nan" else 1., 0.),))
                            except (zipzi.SdkFailure, ValueError):
                                pass
                    self.assertEqual(error.exception.code, "EDIT_ABORTED")
                    self.assertEqual(session.info().history, (0, 0))
                    with self.assertRaises(zipzi.SdkFailure):
                        session.meshes.get(uid(2))

    def test_admission_rejection_retains_candidate_and_source(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                session = self.runtime(mode).sessions.create(uid(1))
                with session.edit("admission") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("A", GEOMETRY))
                    before = edit.meshes.get(face).source
                    with self.assertRaises(zipzi.SdkFailure) as error:
                        edit.apply_many((meshes.Rename(face, "x"),) * 1025)
                    self.assertEqual(error.exception.code, "BUDGET_EXCEEDED")
                    self.assertEqual(edit.meshes.get(face).source, before)
                    edit.meshes.rename(face, "B")
                self.assertEqual(session.meshes.get(face).value.name, "B")

    def test_connected_apply_and_commit_loss_recover_same_request_once(self):
        for lost in ("apply_authoring", "commit", "job"):
            with self.subTest(lost=lost):
                session = self.runtime("connected", drop=lost).sessions.create(uid(1))
                edit = session.edit("recover")
                with self.assertRaises(zipzi.OutcomeUnknown) as error:
                    with edit:
                        edit.apply(meshes.Create(meshes.MeshSpec("Once", GEOMETRY, id=uid(2))))
                if lost != "job":
                    pending = edit.pending_request
                    self.assertIsNotNone(pending)
                    self.assertEqual(error.exception.pending, pending)
                result = error.exception.recover()
                self.assertIsNone(edit.pending_request)
                if lost == "apply_authoring":
                    self.assertEqual(result.source.sequence, 1)
                    edit.commit().wait()
                elif lost == "commit":
                    self.assertIsInstance(result, zipzi.Job)
                    result.wait()
                self.assertEqual(session.info().history, (1, 0))
                self.assertEqual(session.meshes.get(uid(2)).value.name, "Once")

    def test_prepared_images_plans_mixed_batch_history_save_and_reopen(self):
        outputs = []
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                a = runtime.resources.prepare_image(1, 1, bytearray((3, 5, 7, 255)))
                b = runtime.resources.prepare_image(1, 1, memoryview(bytes((11, 13, 17, 255))))
                self.assertEqual((a.width, a.height), (1, 1))
                self.assertEqual(runtime.resources.usage().rgba_bytes, 8)
                with session.edit("texture and mesh") as edit:
                    result = edit.apply_many((resources.CreateImage("A", a, id=uid(2)),
                        meshes.Create(meshes.MeshSpec("Face", GEOMETRY, texture_asset_id=uid(2), id=uid(3)))))
                    self.assertEqual(result.ids, (uid(2), uid(3)))
                    self.assertEqual(result.source.sequence, 1)
                    snapshot = edit.resources.get(uid(2))
                    self.assertEqual(snapshot.value.pixels.rgba, bytes((3, 5, 7, 255)))
                    a.close()
                    a.close()
                    plan = edit.plan(resources.RenameImage(uid(2), "Texture", expected_name="A"))
                    report = plan.report
                    self.assertEqual(report.source, snapshot.source)
                    self.assertEqual(report.affected_ids, (uid(2),))
                    self.assertFalse(report.no_op)
                    with self.assertRaises(FrozenInstanceError):
                        report.no_op = True
                    result = edit.apply(plan)
                    self.assertEqual(result.source.sequence, 2)
                    plan.close()
                    self.assertIs(plan.report, report)
                    self.assertEqual(edit.resources.get(uid(2)).value.name, "Texture")
                self.assertEqual(session.info().history, (1, 0))
                self.assertEqual(b.pixels().rgba, bytes((11, 13, 17, 255)))
                b.close()
                self.assertEqual(runtime.resources.usage().rgba_bytes, 4)
                with session.plan(meshes.Rename(uid(3), "Final", expected_name="Face")) as plan:
                    self.assertEqual(plan.report.source, session.meshes.get(uid(3)).source)
                    with session.edit("formal plan and trailing operation") as edit:
                        result = edit.apply_many((plan, meshes.Rename(uid(3), "Final batch")))
                        self.assertEqual(result.source.sequence, 1)
                session.undo().wait()
                self.assertEqual(session.meshes.get(uid(3)).value.name, "Face")
                self.assertEqual(session.resources.get(uid(2)).value.pixels.rgba, snapshot.value.pixels.rgba)
                session.redo().wait()
                self.assertEqual(session.info().history, (2, 0))
                saved = session.project.save(self.folder / mode).wait()
                outputs.append((session.meshes.get(uid(3)).value.name, session.resources.get(uid(2)).value.pixels))
                runtime.close()
                if mode == "connected":
                    self.stop_host()
                reopened = self.runtime(mode).sessions.open(saved.manifest).wait()
                self.assertEqual(reopened.meshes.get(uid(3)).value.name, "Final batch")
                self.assertEqual(reopened.resources.get(uid(2)).value.pixels.rgba, snapshot.value.pixels.rgba)
                self.assertEqual(reopened.info().history, (0, 0))
        self.assertEqual(outputs[0], outputs[1])

    def test_replacing_image_retains_old_snapshots_and_exact_undo_redo_pixels(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                a, b = (runtime.resources.prepare_image(1, 1, pixels) for pixels in (b"abcd", b"efgh"))
                with session.edit("A") as edit:
                    asset = edit.resources.create_image("Texture", a)
                    self.assertEqual(UUID(asset).version, 4)
                old = session.resources.get(asset)
                a.close()
                with session.edit("B") as edit:
                    edit.resources.replace_image(asset, b)
                    b.close()
                    self.assertEqual(edit.resources.get(asset).value.pixels.rgba, b"efgh")
                    self.assertEqual(session.resources.get(asset).value.pixels.rgba, b"abcd")
                self.assertEqual(session.resources.get(asset).value.name, "Texture")
                self.assertEqual(session.resources.get(asset).value.pixels.rgba, b"efgh")
                session.undo().wait()
                self.assertEqual(session.resources.get(asset).value.pixels.rgba, b"abcd")
                session.redo().wait()
                self.assertEqual(session.resources.get(asset).value.pixels.rgba, b"efgh")
                self.assertEqual(old.value.pixels.rgba, b"abcd")
                self.assertEqual(session.info().history, (2, 0))

    def test_stale_plans_keep_reports_and_abort_without_partial_publication(self):
        for mode in ("local", "connected"):
            runtime = self.runtime(mode)
            session = runtime.sessions.create(uid(1))
            with session.edit("base") as edit:
                face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
            for source in ("formal", "candidate", "other", "batch"):
                with self.subTest(mode=mode, source=source):
                    plan = session.plan(meshes.Rename(face, "Planned", expected_name="Face")) if source in {"formal", "batch"} else None
                    edit = session.edit("stale")
                    if source in {"candidate", "other"}:
                        plan = edit.plan(meshes.Rename(face, "Planned", expected_name="Face"))
                    if source == "other":
                        edit.cancel()
                        edit = session.edit("another candidate")
                    elif source != "batch":
                        edit.meshes.rename(face, "Draft")
                    report = plan.report
                    if source == "batch":
                        with self.assertRaises(ValueError):
                            edit.apply_many((meshes.Rename(face, "Partial"), plan))
                    else:
                        with self.assertRaises(zipzi.SdkFailure) as error:
                            edit.apply(plan)
                        self.assertEqual(error.exception.code, "STALE_PLAN")
                    with self.assertRaises(zipzi.SdkFailure):
                        edit.commit()
                    self.assertIs(plan.report, report)
                    plan.close()
                    self.assertIs(plan.report, report)
                    self.assertEqual(session.meshes.get(face).value.name, "Face")
                    self.assertEqual(session.info().history, (1, 0))
                    edit.cancel()

    def test_foreign_and_closed_handles_poison_the_candidate(self):
        for mode in ("local", "connected"):
            runtime = self.runtime(mode)
            other = self.runtime(mode)
            session = runtime.sessions.create(uid(1))
            image = other.resources.prepare_image(1, 1, bytes((1, 2, 3, 255)))
            for state in ("foreign", "closed"):
                with self.subTest(mode=mode, state=state):
                    if state == "closed":
                        image = runtime.resources.prepare_image(1, 1, bytes((4, 5, 6, 255)))
                        image.close()
                    with self.assertRaises(zipzi.SdkFailure) as error:
                        with session.edit("invalid lease") as edit:
                            edit.resources.create_image("Invalid", image)
                    self.assertEqual(error.exception.code, "WRONG_OWNER" if state == "foreign" else "INVALID_REFERENCE")
                    self.assertEqual(session.info().history, (0, 0))
                    image.close()

    def test_independent_preparation_and_hundred_cancelled_leases_return_to_zero(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                for _ in range(100):
                    edit = session.edit("cancel")
                    with self.assertRaises(zipzi.SdkFailure):
                        runtime.resources.prepare_image(1, 1, b"bad")
                    image = runtime.resources.prepare_image(1, 1, b"rgba")
                    asset = edit.resources.create_image("Draft", image)
                    plan = edit.plan(resources.RenameImage(asset, "Planned", expected_name="Draft"))
                    pixels = edit.resources.get(asset).value.pixels
                    image.close()
                    edit.cancel()
                    # A candidate Plan owns its source snapshot until release.
                    self.assertEqual(runtime.resources.usage().rgba_bytes, 4)
                    plan.close()
                    usage = runtime.resources.usage()
                    self.assertEqual((usage.prepared_images, usage.plans, usage.rgba_bytes, usage.preparing_bytes), (0, 0, 0, 0))
                    self.assertEqual(pixels.rgba, b"rgba")
                self.assertEqual(session.info().history, (0, 0))

    def test_resource_and_plan_response_losses_keep_exact_recovery_owner(self):
        for lost in ("prepare_image", "prepared_pixels", "read_selection", "plan_authoring", "release_prepared_image", "release_modeling_plan", "statistics"):
            with self.subTest(lost=lost):
                runtime = self.runtime("connected", drop=lost)
                session = runtime.sessions.create(uid(1))
                if lost == "prepare_image":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        runtime.resources.prepare_image(1, 1, b"rgba")
                    image = error.exception.recover()
                    self.assertIsInstance(image, zipzi.PreparedImage)
                else:
                    image = runtime.resources.prepare_image(1, 1, b"rgba")
                if lost == "prepared_pixels":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        image.pixels()
                    with self.assertRaises(zipzi.Busy):
                        image.close()
                    self.assertEqual(error.exception.recover().rgba, b"rgba")
                    with self.assertRaises(zipzi.SdkFailure) as resolved:
                        error.exception.recover()
                    self.assertEqual(resolved.exception.code, "NO_PENDING_REQUEST")
                edit = session.edit("recover handles")
                asset = edit.resources.create_image("Draft", image)
                if lost == "read_selection":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        edit.resources.get(asset)
                    self.assertEqual(error.exception.recover().value.pixels.rgba, b"rgba")
                if lost == "plan_authoring":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        edit.plan(resources.RenameImage(asset, "Final", expected_name="Draft"))
                    plan = error.exception.recover()
                    self.assertIsInstance(plan, zipzi.Plan)
                else:
                    plan = edit.plan(resources.RenameImage(asset, "Final", expected_name="Draft"))
                report = plan.report
                with patch.object(runtime._backend, "plan", side_effect=AssertionError("Report made an RPC")):
                    self.assertIs(plan.report, report)
                edit.apply(plan)
                edit.commit().wait()
                for handle, target in ((image, "release_prepared_image"), (plan, "release_modeling_plan")):
                    if lost == target:
                        with self.assertRaises(zipzi.OutcomeUnknown) as error:
                            handle.close()
                        self.assertIsNone(error.exception.recover())
                        handle.close()
                        with self.assertRaises(zipzi.SdkFailure):
                            handle._check(runtime._backend)
                    else:
                        handle.close()
                if lost == "statistics":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        runtime.resources.usage()
                    usage = error.exception.recover()
                else:
                    usage = runtime.resources.usage()
                self.assertEqual((usage.prepared_images, usage.plans), (0, 0))
                self.assertEqual(session.resources.get(asset).value.name, "Final")
                self.assertEqual(session.info().history, (1, 0))

    def test_plan_apply_loss_and_foreign_closed_plan_references(self):
        runtime = self.runtime("connected", drop="apply_authoring", drop_after=1)
        session = runtime.sessions.create(uid(1))
        with session.edit("base") as edit:
            face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
        with session.plan(meshes.Rename(face, "Final", expected_name="Face")) as plan:
            edit = session.edit("lost plan apply")
            with self.assertRaises(zipzi.OutcomeUnknown) as error:
                edit.apply(plan)
            result = error.exception.recover()
            self.assertEqual(result.source.sequence, 1)
            edit.commit().wait()
        self.assertEqual(session.meshes.get(face).value.name, "Final")
        self.assertEqual(session.info().history, (2, 0))
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime, other = self.runtime(mode), self.runtime(mode)
                session, other_session = runtime.sessions.create(uid(1)), other.sessions.create(uid(2))
                with other_session.edit("other") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("Other", GEOMETRY))
                plan = other_session.plan(meshes.Rename(face, "Planned", expected_name="Other"))
                with self.assertRaises(zipzi.SdkFailure) as error:
                    with session.edit("foreign") as edit:
                        edit.apply(plan)
                self.assertEqual(error.exception.code, "WRONG_OWNER")
                plan.close()
                with session.edit("base") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                plan = session.plan(meshes.Rename(face, "Planned", expected_name="Face"))
                report = plan.report
                plan.close()
                with self.assertRaises(zipzi.SdkFailure) as error:
                    with session.edit("closed") as edit:
                        edit.apply(plan)
                self.assertEqual(error.exception.code, "INVALID_REFERENCE")
                self.assertIs(plan.report, report)
                self.assertEqual(session.meshes.get(face).value.name, "Face")
                self.assertEqual(session.info().history, (1, 0))

    def test_failed_read_or_plan_does_not_poison_candidate(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                session = self.runtime(mode).sessions.create(uid(1))
                with session.edit("read failures") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                    source = edit.meshes.get(face).source
                    with self.assertRaises(zipzi.SdkFailure):
                        edit.resources.get(uid(999))
                    with self.assertRaises(zipzi.SdkFailure):
                        edit.plan(meshes.Rename(face, "Planned", expected_name="Wrong expectation"))
                    self.assertEqual(edit.meshes.get(face).source, source)
                    edit.meshes.rename(face, "Final")
                self.assertEqual(session.meshes.get(face).value.name, "Final")
                self.assertEqual(session.info().history, (1, 0))

    def test_mixed_resource_batch_response_loss_does_not_repeat_a_write(self):
        runtime = self.runtime("connected", drop="apply_authoring")
        session = runtime.sessions.create(uid(1))
        image = runtime.resources.prepare_image(1, 1, b"rgba")
        edit = session.edit("mixed lost reply")
        with self.assertRaises(zipzi.OutcomeUnknown) as error:
            edit.apply_many((resources.CreateImage("Draft", image, uid(2)), meshes.Create(meshes.MeshSpec("Face", GEOMETRY, id=uid(3)))))
        result = error.exception.recover()
        self.assertEqual(result.source.sequence, 1)
        self.assertEqual(result.ids, (uid(2), uid(3)))
        edit.commit().wait()
        image.close()
        self.assertEqual(session.info().history, (1, 0))
        self.assertEqual(session.resources.get(uid(2)).value.pixels.rgba, b"rgba")

    def control(self, runtime, action):
        return runtime._backend._test_control(action)

    def await_io(self, runtime):
        deadline = time.monotonic() + 5
        while not self.control(runtime, "entered"):
            self.assertLess(time.monotonic(), deadline, "Worker did not enter IO")
            time.sleep(.005)

    def test_queued_cancel_releases_candidate_and_retains_terminal_error(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                image = runtime.resources.prepare_image(1, 1, b"rgba")
                edit = session.edit("queued")
                asset = edit.resources.create_image("Draft", image)
                image.close()
                self.control(runtime, "pause_dispatch")
                job = edit.commit()
                self.assertEqual(job.state(), "queued")
                job.cancel()
                self.assertEqual(job.state(), "cancelling")
                self.control(runtime, "resume_dispatch")
                with self.assertRaises(zipzi.SdkFailure) as error:
                    job.wait()
                self.assertEqual(error.exception.code, "CANCELLED")
                with self.assertRaises(zipzi.SdkFailure) as again:
                    job.close()
                self.assertIs(again.exception, error.exception)
                self.assertEqual(job.state(), "cancelled")
                self.assertEqual(session.info().history, (0, 0))
                with self.assertRaises(zipzi.SdkFailure):
                    session.resources.get(asset)
                self.assertEqual(runtime.resources.usage().rgba_bytes, 0)

    def test_cancellation_after_entering_io_reports_actual_save(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                with session.edit("base") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                self.control(runtime, "hold_io")
                job = session.project.save(self.folder / mode)
                self.await_io(runtime)
                job.cancel()
                self.assertEqual(job.state(), "cancelling")
                self.control(runtime, "release_io")
                saved = job.wait()
                self.assertTrue(saved.manifest.is_file())
                self.assertEqual(job.state(), "succeeded")
                self.assertIs(job.wait(), saved)
                self.assertEqual(session.info().history, (1, 0))
                self.assertFalse(session.info().modified)
                self.assertEqual(session.meshes.get(face).value.name, "Face")

    def test_worker_failure_and_panic_have_distinct_recovery_scopes(self):
        for mode in ("local", "connected"):
            for fault, expected in (("fail_next", "EXECUTOR_FAILURE"), ("panic_next", "WORKER_PANIC")):
                with self.subTest(mode=mode, fault=fault):
                    runtime = self.runtime(mode)
                    session = runtime.sessions.create(uid(1))
                    with session.edit("base") as edit:
                        face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                    old = session.meshes.get(face)
                    self.control(runtime, fault)
                    job = session.project.save(self.folder / f"{mode}-{fault}")
                    with self.assertRaises(zipzi.SdkFailure) as error:
                        job.wait()
                    self.assertEqual(error.exception.code, expected)
                    self.assertEqual(job.state(), "failed")
                    with self.assertRaises(zipzi.SdkFailure) as again:
                        job.wait()
                    self.assertIs(again.exception, error.exception)
                    self.assertEqual(session.meshes.get(face), old)
                    self.assertEqual(session.info().history, (1, 0))
                    if fault == "panic_next":
                        with self.assertRaises(zipzi.SdkFailure) as faulted:
                            session.edit("unsafe")
                        self.assertEqual(faulted.exception.code, "FAULTED")
                    else:
                        with session.edit("retry") as edit:
                            edit.meshes.rename(face, "After rejection")
                        self.assertEqual(session.info().history, (2, 0))

    def test_cancel_and_state_response_loss_cannot_be_consumed_by_wait(self):
        for lost in ("cancel_job", "job_status"):
            with self.subTest(lost=lost):
                runtime = self.runtime("connected", drop=lost)
                session = runtime.sessions.create(uid(1))
                edit = session.edit("pending")
                edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                self.control(runtime, "pause_dispatch")
                job = edit.commit()
                with self.assertRaises(zipzi.OutcomeUnknown) as error:
                    job.cancel() if lost == "cancel_job" else job.state()
                with self.assertRaises(zipzi.Busy):
                    job.wait()
                self.assertEqual(error.exception.recover(), None if lost == "cancel_job" else "queued")
                self.control(runtime, "resume_dispatch")
                if lost == "cancel_job":
                    with self.assertRaises(zipzi.SdkFailure) as cancelled:
                        job.wait()
                    self.assertEqual(cancelled.exception.code, "CANCELLED")
                else:
                    self.assertTrue(job.wait().changed)

    def test_expired_request_never_replays_a_write_under_a_new_id(self):
        for lost in ("apply_authoring", "prepare_image", "plan_authoring", "commit", "job"):
            with self.subTest(lost=lost):
                runtime = self.runtime("connected", drop=lost, settings={"ZIPZI_TEST_RESULT_TTL_MS": "30"})
                session = runtime.sessions.create(uid(1))
                edit = session.edit("expiry")
                if lost in {"plan_authoring", "commit", "job"}:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                with self.assertRaises(zipzi.OutcomeUnknown) as error:
                    if lost == "apply_authoring":
                        edit.meshes.create(meshes.MeshSpec("Once", GEOMETRY))
                    elif lost == "prepare_image":
                        runtime.resources.prepare_image(1, 1, b"rgba")
                    elif lost == "plan_authoring":
                        edit.plan(meshes.Rename(face, "Planned", expected_name="Face"))
                    elif lost == "commit":
                        edit.commit()
                    else:
                        pending_job = edit.commit()
                        pending_job.wait()
                request_id = runtime._backend._next
                time.sleep(.08)
                with self.assertRaises(zipzi.SdkFailure) as expired:
                    error.exception.recover()
                self.assertEqual(expired.exception.code, "REQUEST_EXPIRED")
                self.assertEqual(runtime._backend._next, request_id)
                if lost == "job":
                    with self.assertRaises(zipzi.SdkFailure) as unknown_state:
                        pending_job.state()
                    self.assertEqual(unknown_state.exception.code, "REQUEST_EXPIRED")
                self.assertEqual(session.info().history, (1, 0) if lost in {"commit", "job"} else (0, 0))

    def test_client_expiry_reclaims_prepared_plan_and_candidate_without_new_identity(self):
        runtime = self.runtime("connected", drop="plan_authoring", settings={"ZIPZI_TEST_GRACE_MS": "30"})
        session = runtime.sessions.create(uid(1))
        image = runtime.resources.prepare_image(1, 1, b"rgba")
        edit = session.edit("expired client")
        asset = edit.resources.create_image("Draft", image)
        snapshot = edit.resources.get(asset)
        with self.assertRaises(zipzi.OutcomeUnknown) as error:
            edit.plan(resources.RenameImage(asset, "Planned", expected_name="Draft"))
        old_client, old_id = runtime._backend._client, runtime._backend._next
        time.sleep(.1)
        with self.assertRaises(zipzi.SdkFailure) as expired:
            error.exception.recover()
        self.assertEqual(expired.exception.code, "CLIENT_EXPIRED")
        self.assertEqual((runtime._backend._client, runtime._backend._next), (old_client, old_id))
        observer = zipzi.connect(endpoint=runtime._backend._endpoint)
        self.addCleanup(observer.close)
        usage = observer.resources.usage()
        self.assertEqual((usage.prepared_images, usage.plans, usage.rgba_bytes), (0, 0, 0))
        self.assertEqual(snapshot.value.pixels.rgba, b"rgba")
        observer._backend._request(dict(op="attach_session", session=session._token), observer, "test.attach")
        self.assertEqual(zipzi.Session(observer, session._token).info().history, (0, 0))

    def test_host_restart_refuses_original_pending_request(self):
        runtime = self.runtime("connected", drop="apply_authoring")
        session = runtime.sessions.create(uid(1))
        edit = session.edit("old runtime")
        with self.assertRaises(zipzi.OutcomeUnknown) as error:
            edit.meshes.create(meshes.MeshSpec("Once", GEOMETRY))
        old_id = runtime._backend._next
        self.stop_host()
        with self.assertRaises(zipzi.OutcomeUnknown) as unavailable:
            error.exception.recover()
        self.assertEqual(unavailable.exception.pending, error.exception.pending)
        fresh = self.runtime("connected", directory=self.folder / str(self.counter))
        with self.assertRaises(zipzi.SdkFailure) as restarted:
            error.exception.recover()
        self.assertEqual(restarted.exception.code, "HOST_RESTARTED")
        self.assertEqual(runtime._backend._next, old_id)
        self.assertEqual(fresh.resources.usage().prepared_images, 0)

    def test_idle_completion_and_close_during_entered_io(self):
        for mode in ("local", "connected"):
            runtime = self.runtime(mode)
            session = runtime.sessions.create(uid(1))
            edit = session.edit("idle")
            face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
            job = edit.commit()
            time.sleep(.08)
            self.assertEqual(job.state(), "succeeded")
            self.assertEqual(session.info().history, (1, 0))
            self.assertEqual(session.meshes.get(face).value.name, "Face")
            job.wait()
        runtime = self.runtime("local")
        session = runtime.sessions.create(uid(2))
        self.control(runtime, "hold_io")
        session.project.save(self.folder / "close-inflight")
        self.await_io(runtime)
        thread = threading.Thread(target=runtime.close)
        thread.start()
        time.sleep(.02)
        self.assertTrue(thread.is_alive(), "close returned before entered IO finished")
        self.control(runtime, "release_io")
        thread.join(timeout=5)
        self.assertFalse(thread.is_alive())
        manifest = self.folder / "close-inflight/project.zipzi.json"
        self.assertTrue(manifest.is_file(), "Entered save was silently rolled back by close")
        reopened = self.runtime("local").sessions.open(manifest).wait()
        self.assertFalse(reopened.info().modified)

    def test_retained_job_budget_refusal_keeps_edit_retryable(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                jobs = [runtime.sessions.create(uid(100 + n)).project.save(self.folder / mode / str(n)) for n in range(8)]
                session = runtime.sessions.create(uid(999))
                edit = session.edit("quota")
                face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                source = edit.meshes.get(face).source
                with self.assertRaises(zipzi.SdkFailure) as error:
                    edit.commit()
                self.assertEqual(error.exception.code, "BUDGET_EXCEEDED")
                self.assertEqual(error.exception.limit_name, "jobs and retained results")
                self.assertEqual(error.exception.recovery_action, "release_resources")
                self.assertIsNone(error.exception.requested)
                self.assertIsNone(error.exception.allowed)
                self.assertEqual(edit.meshes.get(face).source, source)
                jobs[0].wait()
                edit.commit().wait()
                self.assertEqual(session.info().history, (1, 0))
                for job in jobs[1:]:
                    job.wait()

    def test_real_save_failure_releases_result_and_path_without_publication(self):
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime = self.runtime(mode)
                session = runtime.sessions.create(uid(1))
                with session.edit("base") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                before = session.info()
                invalid = self.folder / f"{mode}-file"
                self.control(runtime, "hold_io")
                job = session.project.save(invalid)
                self.await_io(runtime)
                invalid.write_bytes(b"keep")
                self.control(runtime, "release_io")
                try:
                    with self.assertRaises(zipzi.SdkFailure) as error:
                        job.wait()
                    with self.assertRaises(zipzi.SdkFailure) as again:
                        job.wait()
                    self.assertIs(again.exception, error.exception)
                except zipzi.SdkFailure as error:
                    self.fail(f"Expected admitted IO failure, got preflight: {error}")
                self.assertEqual(invalid.read_bytes(), b"keep")
                self.assertEqual(session.info(), before)
                self.assertEqual(session.meshes.get(face).value.name, "Face")
                self.assertTrue(session.project.save(self.folder / f"{mode}-retry").wait().manifest.is_file())

    def test_last_native_owner_releases_project_lock_and_unsaved_memory(self):
        runtime = zipzi.local()
        session = runtime.sessions.create(uid(1))
        with runtime.resources.prepare_image(1, 1, b"rgba") as image:
            with session.edit("memory") as edit:
                asset = edit.resources.create_image("Texture", image)
        saved = session.project.save(self.folder / "last-owner").wait()
        pixels = session.resources.get(asset).value.pixels
        unused = runtime.resources.prepare_image(1, 1, b"abcd")
        edit = session.edit("never publish or autosave")
        draft = edit.resources.create_image("Unsaved", unused)
        unused.close()
        # Saved-image caches may be evicted; the unsaved candidate must still
        # pin its own actual RGBA allocation until the host is dropped.
        self.assertGreaterEqual(runtime.resources.usage().rgba_bytes, 4)
        self.assertEqual(edit.resources.get(draft).value.pixels.rgba, b"abcd")
        backend = weakref.ref(runtime._backend)
        del image, unused, edit, session, runtime
        gc.collect()
        self.assertIsNone(backend(), "A final Python owner still retains the native host")
        reopened = self.runtime("local").sessions.open(saved.manifest).wait()
        self.assertEqual(reopened.resources.get(asset).value.pixels, pixels)
        with self.assertRaises(zipzi.SdkFailure) as unsaved:
            reopened.resources.get(draft)
        self.assertEqual(unsaved.exception.code, "MISSING_IMAGE")

    def test_connected_close_releases_client_leases_and_recovers_lost_ack(self):
        for lost in (None, "release_client"):
            with self.subTest(lost=lost):
                runtime = self.runtime("connected", drop=lost)
                session = runtime.sessions.create(uid(1))
                with session.edit("formal") as edit:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                image = runtime.resources.prepare_image(1, 1, b"rgba")
                edit = session.edit("discard")
                asset = edit.resources.create_image("Draft", image)
                plan = edit.plan(resources.RenameImage(asset, "Planned", expected_name="Draft"))
                observer = zipzi.connect(endpoint=runtime._backend._endpoint)
                self.addCleanup(observer.close)
                if lost:
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        runtime.close()
                    request_id = runtime._backend._next
                    self.assertIsNone(error.exception.recover())
                    self.assertEqual(runtime._backend._next, request_id)
                else:
                    runtime.close()
                runtime.close()
                usage = observer.resources.usage()
                self.assertEqual((usage.prepared_images, usage.plans, usage.rgba_bytes), (0, 0, 0))
                observer._backend._request(dict(op="attach_session", session=session._token), observer, "test.attach")
                retained = zipzi.Session(observer, session._token)
                self.assertEqual(retained.meshes.get(face).value.name, "Face")
                self.assertEqual(retained.info().history, (1, 0))
                self.assertEqual(plan.report.source.sequence, 1)

    def test_failed_terminal_reply_recovery_caches_original_error(self):
        for fault, code in (("fail_next", "EXECUTOR_FAILURE"), ("panic_next", "WORKER_PANIC")):
            with self.subTest(fault=fault):
                runtime = self.runtime("connected", drop="job")
                session = runtime.sessions.create(uid(1))
                self.control(runtime, fault)
                job = session.project.save(self.folder / fault)
                with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                    job.wait()
                with self.assertRaises(zipzi.SdkFailure) as failed:
                    lost.exception.recover()
                self.assertEqual(failed.exception.code, code)
                with self.assertRaises(zipzi.SdkFailure) as again:
                    job.wait()
                self.assertIs(again.exception, failed.exception)

    def test_connected_close_does_not_cancel_entered_publication(self):
        runtime = self.runtime("connected")
        session = runtime.sessions.create(uid(1))
        with session.edit("base") as edit:
            face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
        observer = zipzi.connect(endpoint=runtime._backend._endpoint)
        self.addCleanup(observer.close)
        observer._backend._request(dict(op="attach_session", session=session._token), observer, "test.attach")
        retained = zipzi.Session(observer, session._token)
        self.control(runtime, "hold_io")
        session.project.save(self.folder / "detached-save")
        self.await_io(runtime)
        runtime.close()
        self.control(observer, "release_io")
        deadline = time.monotonic() + 5
        while retained.info().modified:
            self.assertLess(time.monotonic(), deadline)
            time.sleep(.005)
        self.assertTrue((self.folder / "detached-save/project.zipzi.json").is_file())
        self.assertEqual(retained.meshes.get(face).value.name, "Face")
        self.assertEqual(retained.info().history, (1, 0))

    def test_confirmed_host_busy_commit_keeps_candidate_retryable(self):
        session = self.runtime("connected", settings={"ZIPZI_TEST_BUSY_COMMIT_ONCE": "1"}).sessions.create(uid(1))
        edit = session.edit("busy admission")
        face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
        source = edit.meshes.get(face).source
        with self.assertRaises(zipzi.Busy):
            edit.commit()
        self.assertEqual(edit.meshes.get(face).source, source)
        edit.commit().wait()
        self.assertEqual(session.info().history, (1, 0))

    def test_recovered_busy_commit_preserves_candidate_for_retry_or_cancel(self):
        for finish in ("commit", "cancel"):
            with self.subTest(finish=finish):
                session = self.runtime("connected", drop="commit",
                    settings={"ZIPZI_TEST_BUSY_COMMIT_ONCE": "1"}).sessions.create(uid(1))
                edit = session.edit("lost admission refusal")
                with self.assertRaises(zipzi.OutcomeUnknown) as lost:
                    with edit:
                        face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                        source = edit.meshes.get(face).source
                with self.assertRaises(zipzi.Busy):
                    lost.exception.recover()
                self.assertIsNone(edit.pending_request)
                self.assertEqual(edit.meshes.get(face).source, source)
                self.assertEqual(session.info().history, (0, 0))
                if finish == "commit":
                    edit.meshes.rename(face, "Retried")
                    self.assertEqual(edit.meshes.get(face).source.sequence, source.sequence + 1)
                    edit.commit().wait()
                    self.assertEqual(session.meshes.get(face).value.name, "Retried")
                    self.assertEqual(session.info().history, (1, 0))
                else:
                    edit.cancel()
                    self.assertEqual(session.info().history, (0, 0))
                with self.assertRaises(zipzi.SdkFailure) as resolved:
                    lost.exception.recover()
                self.assertEqual(resolved.exception.code, "NO_PENDING_REQUEST")

    def test_formal_plan_rejects_other_session_and_undo_redo_versions(self):
        for mode in ("local", "connected"):
            for changed in ("other_session", "undo", "redo"):
                with self.subTest(mode=mode, changed=changed):
                    runtime = self.runtime(mode)
                    session = runtime.sessions.create(uid(1))
                    with session.edit("base") as edit:
                        face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                    with session.plan(meshes.Rename(face, "Planned", expected_name="Face")) as plan:
                        source = plan.report.source
                        target = runtime.sessions.create(uid(2)) if changed == "other_session" else session
                        if changed in {"undo", "redo"}:
                            session.undo().wait()
                        if changed == "redo":
                            session.redo().wait()
                        edit = target.edit("stale formal")
                        with self.assertRaises(zipzi.SdkFailure) as error:
                            edit.apply(plan)
                        self.assertEqual(error.exception.code, "STALE_PLAN")
                        self.assertEqual(plan.report.source, source)
                        self.assertEqual(session.info().history, (0, 1) if changed == "undo" else (1, 0))
                        if changed != "undo":
                            self.assertEqual(session.meshes.get(face).value.name, "Face")

    def test_old_exception_cannot_consume_a_later_request_from_the_same_owner(self):
        from zipzi.backends import connected
        exchange = connected._exchange
        def lose_reply(stream, body):
            exchange(stream, body)
            raise EOFError("Lost after actual execution")
        for operation in ("create", "info", "apply"):
            with self.subTest(operation=operation):
                runtime = self.runtime("connected")
                session = runtime.sessions.create(uid(1))
                edit = session.edit("same owner") if operation == "apply" else None
                if edit is not None:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                def call(n):
                    if operation == "create":
                        return runtime.sessions.create(uid(n))
                    if operation == "info":
                        return session.info()
                    return edit.meshes.rename(face, f"Name {n}")
                with patch.object(connected, "_exchange", side_effect=lose_reply):
                    with self.assertRaises(zipzi.OutcomeUnknown) as first:
                        call(2)
                first.exception.recover()
                with patch.object(connected, "_exchange", side_effect=lose_reply):
                    with self.assertRaises(zipzi.OutcomeUnknown) as second:
                        call(3)
                with self.assertRaises(zipzi.SdkFailure) as resolved:
                    first.exception.recover()
                self.assertEqual(resolved.exception.code, "NO_PENDING_REQUEST")
                result = second.exception.recover()
                if edit is not None:
                    self.assertEqual(result.source.sequence, 3)
                    edit.commit().wait()
                    self.assertEqual(session.meshes.get(face).value.name, "Name 3")

    def test_local_wait_releases_gil_and_close_is_idempotent(self):
        runtime = self.runtime("local")
        session = runtime.sessions.create(uid(1))
        edit = session.edit("background")
        edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
        job = edit.commit()
        result = []
        thread = threading.Thread(target=lambda: result.append(job.wait()))
        thread.start()
        thread.join(timeout=5)
        self.assertFalse(thread.is_alive(), "Native wait held the GIL or failed to consume completion")
        self.assertTrue(result[0].changed)
        self.assertEqual(job.wait(), result[0])
        runtime.close()
        runtime.close()
        with self.assertRaises(zipzi.SdkFailure) as error:
            session.info()
        self.assertEqual(error.exception.code, "HOST_CLOSED")

    def test_connected_lifecycle_and_read_response_loss_keep_a_recovery_owner(self):
        for lost in ("create_session", "begin_edit", "read_selection", "session_info", "save", "undo", "redo"):
            with self.subTest(lost=lost):
                runtime = self.runtime("connected", drop=lost)
                if lost == "create_session":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        runtime.sessions.create(uid(1))
                    session = error.exception.recover()
                    self.assertIsInstance(session, zipzi.Session)
                else:
                    session = runtime.sessions.create(uid(1))
                if lost == "begin_edit":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        session.edit("begin")
                    edit = error.exception.recover()
                else:
                    edit = session.edit("begin")
                with edit:
                    face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                    if lost == "read_selection":
                        with self.assertRaises(zipzi.OutcomeUnknown) as error:
                            edit.meshes.get(face)
                        snapshot = error.exception.recover()
                        self.assertEqual(snapshot.source.sequence, 1)
                        self.assertEqual(snapshot.value.name, "Face")
                        edit.meshes.rename(face, "After query")
                if lost == "session_info":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        session.info()
                    self.assertEqual(error.exception.recover().history, (1, 0))
                if lost == "save":
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        session.project.save(self.folder / "save-loss")
                    job = error.exception.recover()
                    self.assertTrue(job.wait().manifest.is_file())
                if lost in {"undo", "redo"}:
                    if lost == "redo":
                        session.undo().wait()
                    with self.assertRaises(zipzi.OutcomeUnknown) as error:
                        session.undo() if lost == "undo" else session.redo()
                    job = error.exception.recover()
                    self.assertTrue(job.wait().changed)
                    self.assertEqual(session.info().history, (0, 1) if lost == "undo" else (1, 0))

    def test_connected_cleanup_response_loss_never_covers_the_original_error(self):
        session = self.runtime("connected", drop="release_edit").sessions.create(uid(1))
        edit = session.edit("bad conversion")
        with self.assertRaises(ValueError) as error:
            with edit:
                edit.meshes.create(meshes.MeshSpec("Partial", GEOMETRY, id=uid(2)))
                edit.meshes.set_positions(uid(2), vertex_ids=(2**40,), positions=((0., 0.),))
        self.assertTrue(error.exception.__notes__)
        self.assertIsNotNone(edit.pending_request)
        self.assertIsInstance(error.exception.__context__, zipzi.OutcomeUnknown)
        self.assertIsNone(error.exception.__context__.recover())
        self.assertEqual(session.info().history, (0, 0))
        with self.assertRaises(zipzi.SdkFailure):
            session.meshes.get(uid(2))

    def test_another_edit_cannot_recover_or_poison_the_pending_owner(self):
        runtime = self.runtime("connected", drop="apply_authoring")
        session = runtime.sessions.create(uid(1))
        other = runtime.sessions.create(uid(3))
        edit, other_edit = session.edit("owner"), other.edit("other")
        with self.assertRaises(zipzi.OutcomeUnknown) as lost:
            edit.apply(meshes.Create(meshes.MeshSpec("Once", GEOMETRY, id=uid(2))))
        with self.assertRaises(zipzi.Busy):
            other_edit.apply(meshes.Create(meshes.MeshSpec("Other", GEOMETRY, id=uid(4))))
        self.assertIsNone(other_edit.pending_request)
        self.assertFalse(hasattr(other_edit, "recover"))
        lost.exception.recover()
        edit.commit().wait()
        other_edit.apply(meshes.Create(meshes.MeshSpec("Other", GEOMETRY, id=uid(4))))
        other_edit.commit().wait()
        self.assertEqual(session.info().history, (1, 0))
        self.assertEqual(other.info().history, (1, 0))

    def test_generated_resource_values_preserve_maps_bytes_costs_and_public_identity(self):
        import pickle
        from typing import get_type_hints
        from zipzi._generated import plan_values as plans
        from zipzi._lifecycle import EditableImages
        self.assertIs(get_type_hints(resources.CreateImage)["image"], resources.PreparedImage)
        self.assertIs(get_type_hints(EditableImages.create_image)["image"], resources.PreparedImage)
        runtime = self.runtime("local")
        session = runtime.sessions.create(uid(910))
        with session.edit("source") as edit:
            face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
        source = session.meshes.get(face).source
        raw = runtime._backend._native._test_codegen_report()
        report = plans.PlanReport._read(source._wire(), raw)
        self.assertEqual(report.source, source)
        self.assertEqual((report.version.session_id, report.version.generation, report.version.revision), (9, 2, 3))
        self.assertEqual(report.affected_ids, ("first", "second"))
        self.assertEqual(report.copied_ids, (("a", "first"), ("z", "last")))
        self.assertEqual(report.cost, plans.PlanCost(7, 1 << 40, 11, 13))
        self.assertEqual(report.losses, ("fixed diagnostic",))
        self.assertIs(report.no_op, False)
        raw["no_op"] = 1
        with self.assertRaises(TypeError):
            plans.PlanReport._read(source._wire(), raw)
        self.assertEqual(resources.ImagePixels._read(dict(width=1, height=1, rgba=[0, 127, 255, 1])),
                         resources.ImagePixels(1, 1, bytes((0, 127, 255, 1))))
        for value in (meshes.Rename(face, "Next", expected_name="Face"), resources.RenameImage(face, "B", expected_name="A"), report):
            self.assertEqual(pickle.loads(pickle.dumps(value)), value)

    def test_values_import_without_extension_or_host(self):
        code = f"""import sys
sys.path.insert(0, {str(SOURCE)!r})
import zipzi
from zipzi import meshes, ops, resources
assert 'zipzi_native._native' not in sys.modules
assert zipzi.SdkFailure.__module__ == 'zipzi._errors'
assert meshes.Geometry((0,), ((0., 0.),), ((0., 0.),), ()).vertex_ids == (0,)
"""
        subprocess.run([sys.executable, "-I", "-c", code], check=True, cwd=tempfile.gettempdir())

    def test_local_bridge_does_not_encode_or_decode_python_json(self):
        runtime = self.runtime("local")
        with patch("json.dumps", side_effect=AssertionError("Local bridge encoded JSON")), \
             patch("json.loads", side_effect=AssertionError("Local bridge decoded JSON")):
            session = runtime.sessions.create(uid(1))
            stream = session.subscribe()
            with session.edit("direct") as edit:
                face = edit.meshes.create(meshes.MeshSpec("Face", GEOMETRY))
                edit.apply(meshes.Rename(face, "Direct"))
                self.assertEqual(edit.meshes.get(face).value.name, "Direct")
                with runtime.resources.prepare_image(1, 1, b"rgba") as image:
                    asset = edit.resources.create_image("Draft", image)
                    self.assertEqual(image.pixels().rgba, b"rgba")
                    with edit.plan(resources.RenameImage(asset, "Image", expected_name="Draft")) as plan:
                        edit.apply(plan)
                        self.assertEqual(plan.report.source.sequence, 3)
                    self.assertEqual(edit.resources.get(asset).value.pixels.rgba, b"rgba")
                parameter = edit.parameters.create(parameters.ParameterSpec("Angle", -1., 1., 0.))
                edit.parameters.patch(parameter, parameters.ParameterPatch(name="Direct parameter"))
                self.assertEqual(edit.parameters.get_many((parameter,)).items[0].name, "Direct parameter")
                self.assertEqual(edit.meshes.get_many((face,)).items[0].name, "Direct")
                bound = edit.bindings.create(bindings.MeshBindingSpec(face, (bindings.Axis(parameter, (-1., 1.)),), (
                    bindings.Keyform((-1.,), GEOMETRY.positions),
                    bindings.Keyform((1.,), tuple((x+10., y) for x,y in GEOMETRY.positions)))))
                self.assertEqual(edit.bindings.get(bound).value.mesh_id, face)
                self.assertEqual(edit.modeling.sample(({parameter: 0.},)).value.frames[0].drawables[0].positions[0][0], 5.)
            self.assertEqual(len([e for e in stream.poll() if isinstance(e, events.Published)]), 1)
            with session.project.save(self.folder / "direct-save") as job:
                saved = job.wait()
            self.assertEqual(job.wait(), saved)
            self.assertTrue(saved.manifest.is_file())

if __name__ == "__main__":
    unittest.main()
