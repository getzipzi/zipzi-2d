"""Generation workflow regressions; no running Editor or GPU required."""
import unittest
import os
import sys
from pathlib import Path
import tempfile
import typing

# Use the same copied native fixture as the SDK gate when run with -I.
native_env = os.environ.get("ZIPZI_PYTHON_NATIVE") or os.environ.get("ZIPZI_PYTHON2_NATIVE")
if native_env:
    sys.path[:0] = [str(Path(__file__).resolve().parents[1] / "client/python"), native_env]
from unittest.mock import patch
from dataclasses import replace
import zipzi
from zipzi import meshes, parameters, bindings, scene, modeling as m
from zipzi.mesh_generation import MeshSettings, generate_for_mesh, inspect_geometry

SQUARE = meshes.Geometry((42,7,99,12),((10.,20.),(30.,20.),(30.,60.),(10.,60.)),
    ((0.,0.),(1.,0.),(1.,1.),(0.,1.)),((42,7,99),(42,99,12)))
GENERATED = meshes.Geometry((0,1,2),((0.,0.),(4.,0.),(0.,4.)),((0.,0.),(1.,0.),(0.,1.)),((0,1,2),))

class MeshGenerationWorkflow(unittest.TestCase):
    def test_discovery_annotations_and_missing_host_diagnostic(self):
        for name in ('MeshDraft','GeometryQuality','generate_for_mesh','inspect_geometry'):
            entry=zipzi.describe('mesh_generation.'+name)
            self.assertTrue(entry.documentation)
        typing.get_type_hints(generate_for_mesh)
        typing.get_type_hints(zipzi.mesh_generation.MeshDraft)
        if os.name == 'nt':
            return  # This diagnostic belongs to Unix endpoint discovery.
        with tempfile.TemporaryDirectory() as folder:
            path=Path(folder)/'missing.sock'
            with self.assertRaises(FileNotFoundError) as error:zipzi.connect(endpoint=path)
            self.assertIn('ZIPZI_RUNTIME_PROFILE',str(error.exception))
            self.assertIn(str(path),str(error.exception))

    def test_spacing_rejected_before_request(self):
        for name in ('edge_spacing','interior_spacing'):
            for value in (2.,0.,float('nan'),float('inf'),True):
                with self.subTest(name=name,value=value),self.assertRaises(ValueError):MeshSettings(**{name:value})

    def test_cpu_coverage_uses_uvs_and_pixel_centers(self):
        full=meshes.Geometry(SQUARE.vertex_ids,SQUARE.positions,SQUARE.uvs,SQUARE.triangles)
        report=inspect_geometry(full,4,4,bytes([255]*16))
        self.assertEqual((report.opaque_pixels,report.uncovered_opaque_pixels,report.covered_pixels),(16,0,16))
        self.assertEqual(report.degenerate_triangles,0)
        triangle=inspect_geometry(GENERATED,4,4,bytes([255]*16))
        self.assertEqual(triangle.uncovered_opaque_pixels,6)
        self.assertEqual(triangle.uncovered_opaque_fraction,6/16)
        malformed=meshes.Geometry((0,1,2),GENERATED.positions,GENERATED.uvs,((0,1,9),))
        with self.assertRaises(ValueError):inspect_geometry(malformed,4,4,bytes([255]*16))
        empty=inspect_geometry(full,4,4,bytes(16))
        self.assertIsNone(empty.uncovered_opaque_fraction)

    def fixture(self, runtime):
        s=runtime.sessions.create()
        with runtime.resources.prepare_image(4,4,bytes([255]*64)) as image:
            with s.edit('Fixture') as e:
                asset=e.resources.create_image('Texture',image)
                mesh=e.meshes.create(meshes.MeshSpec('Target',SQUARE,asset))
        return s,mesh

    def test_affine_mapping_apply_and_stale_source(self):
        with zipzi.local() as rt:
            s,mesh=self.fixture(rt);before=s.info()
            with patch('zipzi.mesh_generation.generate',return_value=GENERATED) as generate:
                draft=generate_for_mesh(s,mesh)
                self.assertEqual(generate.call_args.args[1:3],(4,4))
            self.assertEqual(draft.geometry.positions,((10.,20.),(30.,20.),(10.,60.)))
            self.assertEqual(s.info(),before)
            mapping={v:None for v in SQUARE.vertex_ids}
            with s.edit('Apply generated') as e:draft.apply(e,vertex_mapping=mapping)
            self.assertEqual(s.meshes.get(mesh).value.geometry.positions,draft.geometry.positions)
            with self.assertRaises(zipzi.SdkFailure):
                with s.edit('Stale') as e:draft.apply(e,vertex_mapping=mapping)

    def test_non_affine_mapping_and_dependencies_are_not_discarded(self):
        with zipzi.local() as rt:
            s,mesh=self.fixture(rt)
            with s.edit('Bind') as e:
                param=e.parameters.create(parameters.ParameterSpec('P',0.,1.,0.))
                e.bindings.create(bindings.MeshBindingSpec(mesh,(bindings.Axis(param,(0.,1.)),),
                    (bindings.Keyform((0.,),SQUARE.positions),bindings.Keyform((1.,),SQUARE.positions))))
            before=s.info()
            with patch('zipzi.mesh_generation.generate',return_value=GENERATED):draft=generate_for_mesh(s,mesh)
            with self.assertRaises(zipzi.SdkFailure):
                with s.edit('Missing dependency') as e:draft.apply(e,vertex_mapping={v:None for v in SQUARE.vertex_ids})
            self.assertEqual(s.info(),before)
            other,identity=self.fixture(rt)
            with other.edit('Non affine') as e:e.apply(meshes.SetPositions(identity,SQUARE.vertex_ids,((10.,20.),(30.,20.),(30.,60.),(11.,61.))))
            with patch('zipzi.mesh_generation.generate') as generate:
                with self.assertRaises(zipzi.SdkFailure) as error:generate_for_mesh(other,identity)
                self.assertEqual(error.exception.code,'NON_AFFINE_MAPPING');generate.assert_not_called()



    def test_recovery_resumes_each_generation_step_without_replay(self):
        with zipzi.local() as rt:
            for stage in ('mesh','image','generation'):
                s,mesh=self.fixture(rt)
                record=s.modeling.records(m.ModelingQuery(meshes=(mesh,)))
                image=s.resources.get(record.value.meshes[0].texture_asset_id.object_id)
                value={'mesh':record,'image':image,'generation':GENERATED}[stage]
                error=zipzi.OutcomeUnknown(zipzi.PendingRequest(17,'digest','test'),'Lost reply')
                error._resume=lambda:value
                target,attribute=(s.modeling,'records') if stage=='mesh' else (s.resources,'get') if stage=='image' else (zipzi.mesh_generation,'generate')
                with patch('zipzi.mesh_generation.generate',return_value=GENERATED):
                    with patch.object(target,attribute,side_effect=error) as request:
                        with self.assertRaises(zipzi.OutcomeUnknown) as caught:generate_for_mesh(s,mesh)
                        draft=caught.exception.recover()
                        self.assertIsInstance(draft,zipzi.mesh_generation.MeshDraft)
                        self.assertEqual(draft.geometry.positions,((10.,20.),(30.,20.),(10.,60.)))
                        self.assertEqual(request.call_count,1)
                        with self.assertRaises(zipzi.SdkFailure):caught.exception.recover()


    def test_apply_recovers_the_write_result_without_replaying(self):
        with zipzi.local() as rt:
            s,mesh=self.fixture(rt)
            with patch('zipzi.mesh_generation.generate',return_value=GENERATED):draft=generate_for_mesh(s,mesh)
            history=s.info().history
            with s.edit('Recovered mesh') as edit:
                original=edit.apply
                def lose_reply(operation):
                    result=original(operation)
                    error=zipzi.OutcomeUnknown(zipzi.PendingRequest(23,'digest','apply'),'Lost reply')
                    error._resume=lambda:result
                    raise error
                with patch.object(edit,'apply',side_effect=lose_reply) as request:
                    with self.assertRaises(zipzi.OutcomeUnknown) as caught:draft.apply(edit,vertex_mapping={v:None for v in SQUARE.vertex_ids})
                    self.assertIsInstance(caught.exception.recover(),zipzi.WriteResult)
                    self.assertEqual(request.call_count,1)
            self.assertEqual(s.info().history,(history[0]+1,0))

    def test_parent_spaces_and_changed_candidate(self):
        with zipzi.local() as rt:
            for kind in ('rotation','warp'):
                s,mesh=self.fixture(rt)
                with s.edit('Parent') as e:
                    operation=scene.CreateRotation('Parent',scene.Placement()) if kind=='rotation' else scene.CreateWarp('Parent',scene.Placement(),bounds=(0.,0.,100.,100.))
                    parent=e.apply(operation).ids[0]
                    e.apply(scene.ReparentLocal((mesh,),parent))
                with patch('zipzi.mesh_generation.generate',return_value=GENERATED):draft=generate_for_mesh(s,mesh)
                expected=m.sampling.GeometrySpaceRotationLocal if kind=='rotation' else m.sampling.GeometrySpaceWarpParameter
                self.assertIsInstance(draft.space,expected)
                before=s.info()
                with s.edit('Candidate change') as e:
                    e.meshes.rename(mesh,'Changed')
                    with self.assertRaises(zipzi.SdkFailure) as error:draft.apply(e,vertex_mapping={v:None for v in SQUARE.vertex_ids})
                    self.assertEqual(error.exception.code,'STALE_SOURCE')
                    e.cancel()
                self.assertEqual(s.info(),before)

if __name__=='__main__':unittest.main()
