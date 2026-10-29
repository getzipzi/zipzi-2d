"""Explicit GPU acceptance; CPU test selection never initializes a device."""
from pathlib import Path
import sys
import time
import unittest
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parent))
from test_api import PythonSdk,zipzi,uid,GEOMETRY,meshes

class Observation(PythonSdk):
    def test_topology_selection_anomalies_opacity_and_invalid_options(self):
        from dataclasses import replace
        from zipzi import observation as o, scene
        import runpy
        geometry=runpy.run_path(str(Path(__file__).with_name("r5_observation_recipe.py")))['GEOMETRY']
        self.assertEqual((o.Overlay().heat_opacity,o.Overlay().label_scale,o.Thresholds().min_angle_degrees),(.25,2,10.))
        for mode in ("local","connected"):
            with self.subTest(mode=mode):
                runtime=self.runtime(mode);session=runtime.sessions.create(uid(1))
                with runtime.resources.prepare_image(1,1,bytes((200,100,50,255))) as image:
                    with session.edit("Selected topology") as edit:
                        edit.apply_many((scene.CreatePart(scene.PartSpec("Root",id=uid(10))),
                            scene.CreatePart(scene.PartSpec("Child",id=uid(11),parent_id=uid(10)))))
                        asset=edit.resources.create_image("Color",image)
                        left=edit.meshes.create(meshes.MeshSpec("Left",geometry,texture_asset_id=asset))
                        right=edit.meshes.create(meshes.MeshSpec("Right",replace(geometry,positions=tuple((x+1,y) for x,y in geometry.positions)),texture_asset_id=asset))
                        edit.apply(scene.MoveToPart((left,),part_id=uid(11)))
                before=session.info();source=session.meshes.get(left).source
                view=o.View(width=128,height=64,roi=(0.,0.,2.,1.),quality=True)
                def render(overlay):
                    return session.modeling.observe(({},),replace(view,topology=overlay),expected=source).wait().value.frames[0]
                ordinary=render(None)
                selected=o.Overlay(targets=(o.PartTarget(value=uid(10)),o.MeshTarget(value=left)))
                group=render(selected)
                nested=render(replace(selected,targets=(o.PartTarget(value=uid(11)),)))
                self.assertEqual(group.rgba,nested.rgba)
                self.assertEqual(tuple(q.mesh_id for q in group.quality),(left,))
                pixel=lambda image,x,y:image.rgba[(y*128+x)*4:(y*128+x)*4+4]
                self.assertEqual(pixel(group,32,32),bytes((0,220,255,255)))
                self.assertEqual(pixel(group,96,32),pixel(ordinary,96,32))
                self.assertEqual(render(o.Overlay(targets=())).rgba,ordinary.rgba)
                self.assertEqual(render(o.Overlay(targets=())).quality,())
                self.assertEqual(render(replace(selected,anomalies_only=True,vertices=True,vertex_ids=True,strain=True)).rgba,ordinary.rgba)
                heat=replace(selected,wireframe=False,strain=True,heat_opacity=.25,
                    anomalies_only=True,thresholds=o.Thresholds(min_angle_degrees=50.))
                heated=render(heat)
                expected=bytes(((255*64+200*191+127)//255,(170*64+100*191+127)//255,(30*64+50*191+127)//255,255))
                self.assertEqual(pixel(heated,16,32),expected)
                self.assertEqual(render(replace(heat,heat_opacity=0.)).rgba,ordinary.rgba)
                with session.preview(expected=source) as preview:
                    frame=preview.seek(0.,view=replace(view,topology=heat),geometry=False).wait().value.frames[0]
                    self.assertEqual(frame.observation.rgba,heated.rgba)
                    self.assertEqual(tuple(q.mesh_id for q in frame.observation.quality),(left,))
                for overlay,code in ((o.Overlay(targets=(o.MeshTarget(value=uid(999)),)),"MISSING_MESH"),
                    (o.Overlay(targets=(o.PartTarget(value=uid(999)),)),"MISSING_PART"),
                    (o.Overlay(heat_opacity=-.1),"INVALID_TOPOLOGY_OPTIONS"),
                    (o.Overlay(label_scale=0),"INVALID_TOPOLOGY_OPTIONS")):
                    with self.assertRaises(zipzi.SdkFailure) as error:render(overlay)
                    self.assertEqual(error.exception.code,code)
                self.assertEqual(session.info(),before)
                self.assertEqual(session.meshes.get(right).source,source)

    def test_mesh_quality_topology_and_preview_use_the_same_frozen_view(self):
        import json
        from dataclasses import replace
        from zipzi import observation as o, inspection, sampling
        import runpy
        recipe=runpy.run_path(str(Path(__file__).with_name("r5_observation_recipe.py")))
        self.assertTrue(o.Overlay().wireframe)
        for mode in ("local", "connected"):
            with self.subTest(mode=mode):
                runtime=self.runtime(mode)
                session=runtime.sessions.create(uid(1))
                geometry=replace(recipe['GEOMETRY'], meshing=meshes.Meshing(
                    (10,20,30,40), (meshes.Constraint((10,30),meshes.ConstraintKind.USER),)))
                with runtime.resources.prepare_image(2,2,bytes((255,0,0,255, 255,0,0,0, 255,0,0,0, 255,0,0,255))) as image:
                    with session.edit("Topology fixture") as edit:
                        asset=edit.resources.create_image("Alpha",image)
                        mesh=edit.meshes.create(meshes.MeshSpec("Quad",geometry,texture_asset_id=asset))
                before=session.info()
                source=session.meshes.get(mesh).source
                view=o.View(width=64,height=32,roi=(0.,0.,2.,1.),quality=True)
                ordinary=session.modeling.observe(({},),view,expected=source).wait()
                wire_view=replace(view,topology=o.Overlay())
                wire=session.modeling.observe(({},{}),wire_view,expected=source).wait()
                self.assertEqual(wire.source,source)
                first,repeated=wire.value.frames
                self.assertEqual(first.rgba,repeated.rgba)
                self.assertEqual(first.quality,repeated.quality)
                self.assertEqual(first.scene_digest,ordinary.value.frames[0].scene_digest)
                self.assertNotEqual(first.render_digest,ordinary.value.frames[0].render_digest)
                self.assertNotEqual(first.input_sha256,ordinary.value.frames[0].input_sha256)
                self.assertEqual((first.scale,first.offset),(ordinary.value.frames[0].scale,ordinary.value.frames[0].offset))
                self.assertEqual(recipe['pixel'](first),bytes((0,220,255,255)))
                coverage=first.quality[0].texture_coverage
                self.assertEqual((coverage.covered_pixels,coverage.transparent_covered_pixels,coverage.uncovered_opaque_pixels),(4,2,0))
                self.assertEqual(coverage.transparent_coverage_fraction,.5)
                cpu=session.modeling.sample(({},),checks=sampling.Checks(quality=True),expected=source)
                self.assertEqual(first.quality[0].triangles,cpu.value.quality[0][0].triangles)
                constraints=replace(view,topology=o.Overlay(wireframe=False,constraints=True,topology_only=True))
                constrained=session.modeling.observe(({},),constraints,expected=source).wait().value.frames[0]
                self.assertEqual(recipe['pixel'](constrained),bytes((255,40,200,255)))
                self.assertEqual(recipe['pixel'](constrained,48,16),bytes(4))
                self.assertNotEqual(recipe['pixel'](ordinary.value.frames[0],8,16),bytes(4))
                self.assertEqual(recipe['pixel'](constrained,8,16),bytes(4))
                full=replace(view,topology=o.Overlay(vertices=True,vertex_ids=True,constraints=True,strain=True))
                overlay_only=session.modeling.observe(({},),replace(full,quality=False),expected=source).wait().value.frames[0]
                self.assertEqual(overlay_only.quality,())
                with session.preview(expected=source) as preview:
                    observed=preview.seek(0.,view=full,geometry=False).wait()
                    self.assertEqual(observed.source,source)
                    self.assertEqual(observed.value.frames[0].observation.quality,first.quality)
                    self.assertEqual(observed.value.frames[0].observation.rgba,overlay_only.rgba)
                exported=inspection.save(wire,self.folder/(mode+"-topology"))
                manifest=json.loads(exported.manifest.read_text())
                self.assertTrue(manifest['view']['topology']['wireframe'])
                self.assertEqual(manifest['frames'][0]['quality'][0]['texture_coverage']['covered_pixels'],4)
                self.assertEqual(session.info(),before)
                with session.edit("Candidate quality") as edit:
                    edit.meshes.set_positions(mesh,vertex_ids=geometry.vertex_ids,positions=tuple((x*2,y) for x,y in geometry.positions))
                    candidate=edit.meshes.get(mesh).source
                    job=edit.modeling.observe(({},),full,expected=candidate)
                    edit.cancel()
                    frozen=job.wait()
                    self.assertEqual(frozen.source,candidate)
                    self.assertAlmostEqual(frozen.value.frames[0].quality[0].triangles[0].area,1.)
                self.assertEqual(session.info(),before)

    def test_r5_candidate_preview_images_survive_later_replacement_and_cancel(self):
        import runpy
        recipe=runpy.run_path(str(Path(__file__).with_name("r5_observation_recipe.py")))
        for mode in ("local","connected"):
            runtime=self.runtime(mode);session=recipe['fixture'](runtime);before=session.info()
            edit=session.edit("Candidate Preview")
            recipe['replace_image'](runtime,edit,bytes((0,0,255,255)))
            captured=edit.resources.get(recipe['uid'](2)).source
            with edit.preview(expected=captured) as preview:
                recipe['replace_image'](runtime,edit,bytes((255,255,0,255)));edit.cancel()
                value=preview.seek(.25,geometry=False,view=recipe['VIEW']).wait()
                self.assertEqual(value.source,captured)
                self.assertEqual(recipe['pixel'](value.value.frames[0].observation),bytes((0,0,255,255)))
            self.assertEqual(session.info(),before)

    def test_r5_disk_resource_change_refuses_images_and_preserves_source(self):
        import runpy
        recipe=runpy.run_path(str(Path(__file__).with_name("r5_observation_recipe.py")))
        for mode in ("local","connected"):
            runtime=self.runtime(mode);session=recipe['fixture'](runtime)
            (self.folder/(mode+'-disk')).mkdir()
            saved=session.project.save(self.folder/(mode+'-disk')/'model.zipzi').wait()
            asset=session.modeling.records(zipzi.modeling.ModelingQuery(assets=(recipe['uid'](2),))).value.assets[0]
            disk=saved.manifest.parent/asset.content.source
            original=disk.read_bytes();before=session.info()
            job=None
            runtime._backend._test_control("hold_io")
            try:
                job=session.modeling.observe(({},{}),recipe['VIEW'])
                disk.write_bytes(b"external change after source capture")
            finally:runtime._backend._test_control("release_io")
            try:
                with self.assertRaises(zipzi.SdkFailure):job.wait()
                self.assertEqual(session.info(),before)
            finally:disk.write_bytes(original)
            result=session.modeling.observe(({},),recipe['VIEW']).wait()
            self.assertEqual(recipe['pixel'](result.value.frames[0]),bytes((255,0,0,255)))

    def test_r5_observation_frozen_pixels_animation_and_inspection(self):
        import runpy
        recipe=runpy.run_path(str(Path(__file__).with_name("r5_observation_recipe.py")))["run"]
        results=[recipe(self.runtime(mode),self.folder/(mode+"-observation")) for mode in ("local","connected")]
        self.assertEqual(results[0],results[1])

    def test_r5_observation_complete_call_and_preview_image_recovery(self):
        from zipzi import observation as o
        view=o.View(width=8,height=8,roi=(0.,0.,1.,1.))
        for phase in ("request_observation","job","read_blob_binary"):
            with self.subTest(phase=phase):
                runtime=self.runtime("connected",drop=phase);session=runtime.sessions.create(uid(1))
                job=zipzi.recover_call(session.modeling.observe,({},{}),view)
                result=zipzi.recover_call(job.wait)
                self.assertEqual(len(result.value.frames),2)
                self.assertEqual(result.value.frames[0].rgba,bytes(8*8*4))
        for phase in ("sample_preview","job","read_blob_binary"):
            with self.subTest(preview_phase=phase):
                runtime=self.runtime("connected",drop=phase);session=runtime.sessions.create(uid(1))
                with session.preview() as preview:
                    job=zipzi.recover_call(preview.seek,.25,view=view,geometry=False)
                    result=zipzi.recover_call(job.wait)
                    self.assertEqual(result.value.frames[0].observation.rgba,bytes(8*8*4))
                    self.assertEqual(preview.info().value.sequence,1)

    def test_r5_observation_cancel_and_local_direct_values(self):
        from zipzi import observation as o, inspection
        view=o.View(width=8,height=8,roi=(0.,0.,1.,1.))
        for mode in ("local","connected"):
            runtime=self.runtime(mode);session=runtime.sessions.create(uid(1));before=session.info();backend=runtime._backend
            backend._test_control("hold_io")
            job=session.modeling.observe(({},),view)
            deadline=time.monotonic()+3
            while not backend._test_control("entered"):
                self.assertLess(time.monotonic(),deadline);time.sleep(.005)
            job.cancel();backend._test_control("release_io")
            with self.assertRaises(zipzi.SdkFailure) as cancelled:job.wait()
            self.assertEqual(cancelled.exception.code,"CANCELLED");self.assertEqual(session.info(),before)
        runtime=self.runtime("local");session=runtime.sessions.create(uid(1))
        with patch("json.dumps",side_effect=AssertionError("Observation Python JSON encode")), \
             patch("json.loads",side_effect=AssertionError("Observation Python JSON decode")):
            value=session.modeling.observe(({},),view).wait()
            with session.preview() as preview:
                self.assertEqual(preview.seek(0.,view=view).wait().value.frames[0].observation.rgba,bytes(8*8*4))
        runtime.close()
        report=inspection.save(value,self.folder/"offline-inspection")
        self.assertEqual(report.images[0].read_bytes(),value.value.frames[0].png)
        with self.assertRaises(FileExistsError):inspection.save(value,report.directory)

    def test_r5_large_gpu_images_binary_recovery_and_invalid_candidate(self):
        from zipzi import observation as o
        for mode in ("local","connected"):
            runtime=self.runtime(mode,drop="read_blob_binary" if mode=="connected" else None,drop_after=1)
            session=runtime.sessions.create(uid(1))
            with runtime.resources.prepare_image(1,1,bytes((255,0,0,255))) as image:
                with session.edit("Image") as edit:
                    asset=edit.resources.create_image("Red",image)
                    edit.meshes.create(meshes.MeshSpec("Triangle",GEOMETRY,texture_asset_id=asset))
            view=o.View(width=512,height=512,roi=(0.,0.,1.,1.))
            before=session.info();job=session.modeling.observe(({},{}),view)
            value=zipzi.recover_call(job.wait)
            self.assertEqual(len(value.value.frames),2)
            self.assertEqual(len(value.value.frames[-1].rgba),512*512*4)
            index=(480*512+32)*4
            self.assertEqual(value.value.frames[-1].rgba[index:index+4],bytes((255,0,0,255)))
            self.assertEqual(session.info(),before)
            with session.edit("Untextured candidate") as edit:
                mesh=edit.meshes.create(meshes.MeshSpec("Empty texture",GEOMETRY))
                with self.assertRaises(zipzi.SdkFailure) as missing:edit.modeling.observe(({},),view).wait()
                self.assertEqual(missing.exception.code,"MISSING_ASSET")
                edit.meshes.rename(mesh,"Still editable")
                self.assertEqual(edit.meshes.get(mesh).value.name,"Still editable")
                edit.cancel()


def load_tests(loader,tests,pattern):
    return unittest.TestSuite(Observation(name) for name in Observation.__dict__ if name.startswith('test_'))

if __name__=='__main__':unittest.main()
