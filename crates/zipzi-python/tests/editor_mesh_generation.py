"""Detached mesh generation and publication on a production Editor host."""
import sys
import unittest
from pathlib import Path
from uuid import uuid4

import zipzi
from zipzi import values as v

sys.path.insert(0, str(Path(__file__).resolve().parent))
from test_kernel import connected


class EditorMeshGeneration(unittest.TestCase):
    def test_service_generates_detached_geometry_then_draft_publishes_it(self):
        alpha = bytes(255 if 8 <= x < 56 and 8 <= y < 56 else 0
                      for y in range(64) for x in range(64))
        settings = dict(edge_spacing=12, interior_spacing=12)
        with connected() as runtime:
            session = runtime.sessions.create()
            before = session.info()
            if 'editor_mesh_generate' not in runtime.capabilities():
                with self.assertRaises(zipzi.SdkFailure) as error:
                    runtime.generate_editor_mesh(64, 64, alpha=alpha)
                self.assertEqual(error.exception.code, 'UNSUPPORTED_OPERATION')
                return
            for edge_mode in ('single', 'double', 'triple'):
                self.assertTrue(runtime.generate_editor_mesh(64, 64, alpha=alpha,
                    settings=dict(settings, edge_mode=edge_mode)).triangles)
            edges = runtime.generate_editor_mesh(64, 64, alpha=alpha, settings=settings, stage='edges')
            for algorithm in ('graded_poisson', 'simple_triangles', 'adaptive_quadtree',
                              'triangle_fractal', 'contour_paving'):
                options = dict(settings, fill_algorithm=algorithm)
                mesh = runtime.generate_editor_mesh(64, 64, alpha=alpha, settings=options)
                self.assertTrue(mesh.triangles)
                self.assertEqual(mesh, runtime.generate_editor_mesh(64, 64, alpha=alpha, settings=options))
                interior = runtime.generate_editor_mesh(64, 64, settings=options, stage='interior', geometry=edges)
                self.assertTrue(set(edges.vertex_ids).issubset(interior.vertex_ids))
            for field in ('edge_spacing', 'interior_spacing', 'wrap'):
                with self.assertRaises(zipzi.SdkFailure) as error:
                    runtime.generate_editor_mesh(64, 64, alpha=alpha, settings=dict(settings, **{field: 0 if field != 'wrap' else -1}))
                self.assertEqual(error.exception.code, 'INVALID_ARGUMENT')
            self.assertEqual(session.info(), before)
            mesh_id = str(uuid4())
            with session.draft('Generated mesh') as draft:
                draft.apply(v.CommandCreateMesh(spec=v.MeshCreateSpec(id=mesh_id, name='Generated', geometry=mesh)))
            item = session.query(v.QueryObjects(addresses=(v.ObjectAddress(kind=v.ObjectKind.MESH,id=mesh_id),))).value['records'][0]['value']
            authored = v.Mesh.from_wire(item)
            self.assertEqual(authored.vertex_ids, mesh.vertex_ids)
            self.assertEqual(authored.base_positions, mesh.positions)
            self.assertEqual(authored.triangles, mesh.triangles)
            self.assertEqual(session.info()['history']['undo_steps'], 1)


if __name__ == "__main__":
    unittest.main()
