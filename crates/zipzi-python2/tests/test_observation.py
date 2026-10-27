"""Explicit GPU acceptance; CPU test selection never initializes a device."""
from pathlib import Path
import sys
import time
import unittest
from unittest.mock import patch
sys.path.insert(0,str(Path(__file__).resolve().parent))
from test_api import PythonSdk,zipzi,uid,GEOMETRY,meshes

class Observation(PythonSdk):
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
