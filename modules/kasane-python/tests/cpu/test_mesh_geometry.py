from __future__ import annotations
import inspect
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from tempfile import TemporaryDirectory
import unittest
from uuid import uuid4

import kasane
from .common import *

class CpuMeshGeometryTests(unittest.TestCase):
    def test_blend_bindings_cover_all_targets(self):
        model = session()
        zero = [(0, 0)] * 4
        warp_points = [(0, 0), (1, 0), (0, 1), (1, 1)]
        with model.edit("blend targets") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "mesh", ASSET, (0, 0), (1, 1))
            edit.create_rectangle(MESH_B, "second", ASSET, (0, 0), (1, 1))
            edit.create_part(PART, "part")
            edit.create_rotation_transform(
                ROTATION, "rotation", kasane.RotationData(0, kasane.RotationPose((0, 0)))
            )
            edit.create_warp_transform(WARP, "warp", kasane.WarpData(1, 1, True, warp_points))
            edit.create_glue(kasane.GlueSpec(
                GLUE, "seam", MESH, MESH_B, [kasane.GlueVertexPair(0, 0, 1, 1)]
            ))
            edit.create_offscreen(kasane.OffscreenSpec(
                OFFSCREEN, "layer", PART, keyforms=[kasane.OffscreenKeyform(1)]
            ))
            edit.create_parameter(BLEND_PARAMETER, "shape", 0, 1, 0, kind="blend_shape")
            edit.create_parameter(PARAMETER, "limit", 0, 1, 0)
            edit.create_blend_key_table(kasane.BlendKeyTableSpec(
                BLEND_TABLE, BLEND_PARAMETER, [0, 1], 0,
            ))
            edit.create_blend_constraint(kasane.BlendConstraintSpec(
                BLEND_CONSTRAINT, PARAMETER, [0, 1], [1, 1],
            ))
            for spec in [
                kasane.BlendBindingSpec(BLEND_MESH, MESH, "mesh", BLEND_TABLE,
                    [BLEND_CONSTRAINT], [kasane.BlendMeshDelta(zero),
                    kasane.BlendMeshDelta([(1, 0)] * 4, 0.2, 2, (0.8, 1, 1), (0, 0.1, 0))]),
                kasane.BlendBindingSpec(BLEND_WARP, WARP, "warp", BLEND_TABLE, [],
                    [kasane.BlendWarpDelta(zero), kasane.BlendWarpDelta([(0.1, 0)] * 4, 0.3)]),
                kasane.BlendBindingSpec(BLEND_ROTATION, ROTATION, "rotation", BLEND_TABLE, [],
                    [kasane.BlendRotationDelta(), kasane.BlendRotationDelta((1, 2), 30, 1.5, 0.4)]),
                kasane.BlendBindingSpec(BLEND_PART, PART, "part", BLEND_TABLE, [],
                    [kasane.BlendPartDelta(0), kasane.BlendPartDelta(5)]),
                kasane.BlendBindingSpec(BLEND_GLUE, GLUE, "glue", BLEND_TABLE, [],
                    [kasane.BlendGlueDelta(0), kasane.BlendGlueDelta(1)]),
                kasane.BlendBindingSpec(BLEND_OFFSCREEN, OFFSCREEN, "offscreen", BLEND_TABLE, [],
                    [kasane.BlendOffscreenDelta(0), kasane.BlendOffscreenDelta(0.5, (1, 0.9, 1))]),
            ]:
                edit.create_blend_binding(spec)
        self.assertEqual(len(model.blend_binding_ids()), 6)
        self.assertEqual(model.blend_binding(BLEND_MESH).keyforms[1].draw_order, 2)
        self.assertAlmostEqual(model.blend_binding(BLEND_WARP).keyforms[1].points[0][0], 0.1)
        self.assertEqual(model.blend_binding(BLEND_ROTATION).keyforms[1].angle, 30)
        self.assertEqual(model.blend_binding(BLEND_PART).keyforms[1].draw_order, 5)
        self.assertEqual(model.blend_binding(BLEND_GLUE).keyforms[1].intensity, 1)
        self.assertAlmostEqual(model.blend_binding(BLEND_OFFSCREEN).keyforms[1].opacity, 0.5)
        binding = model.blend_binding(BLEND_MESH)
        binding.keyforms[1].positions[0] = (99, 99)
        self.assertEqual(model.blend_binding(BLEND_MESH).keyforms[1].positions[0], (1, 0))
        with model.edit("replace blend binding") as edit:
            edit.replace_blend_binding(binding._replace(keyforms=[
                kasane.BlendMeshDelta(zero), kasane.BlendMeshDelta([(2, 0)] * 4),
            ]))
        self.assertEqual(model.blend_binding(BLEND_MESH).keyforms[1].positions[0], (2, 0))
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.blend_binding(BLEND_MESH).keyforms[1].positions[0], (2, 0))
        version = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("invalid blend binding") as edit:
                edit.replace_blend_binding(binding._replace(keyforms=[kasane.BlendMeshDelta(zero)]))
        self.assertEqual(error.exception.code, "INCOMPLETE_KEYFORMS")
        self.assertEqual(model.version, version)
        model.undo()
        self.assertEqual(model.blend_binding(BLEND_MESH).keyforms[1].positions[0], (1, 0))

    def test_custom_mesh_create_and_complete_replace(self):
        model = session()
        with model.edit("asset") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
        geometry = kasane.MeshGeometryData(
            [10, 11, 12], [(40, 40), (60, 40), (40, 60)],
            [(0, 0), (1, 0), (0, 1)], [(10, 11, 12)],
        )
        with model.edit("custom mesh") as edit:
            edit.create_mesh(kasane.MeshRecordSpec(
                MESH, "triangle", geometry, kasane.MeshDrawingData(ASSET),
            ))
        original = model.mesh_record(MESH)
        self.assertEqual(original.runtime_id, MESH)
        self.assertEqual(original.geometry.triangles, [(10, 11, 12)])
        original.geometry.positions[0] = (999, 999)
        self.assertEqual(model.mesh_record(MESH).geometry.positions[0], (40, 40))
        with model.edit("replace mesh") as edit:
            edit.replace_mesh(original._replace(
                name="triangle updated",
                geometry=geometry._replace(positions=[(41, 40), (60, 40), (40, 60)]),
                drawing=original.drawing._replace(
                    appearance=kasane.Appearance(0.5), draw_order=3,
                ),
            ))
        updated = model.mesh_record(MESH)
        self.assertEqual(updated.runtime_id, MESH)
        self.assertEqual(updated.name, "triangle updated")
        self.assertEqual(updated.geometry.positions[0], (41, 40))
        self.assertAlmostEqual(updated.drawing.appearance.opacity, 0.5)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.mesh_record(MESH).geometry.triangles, [(10, 11, 12)])
        version = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("invalid triangle") as edit:
                edit.replace_mesh(updated._replace(
                    geometry=updated.geometry._replace(triangles=[(10, 11, 99)]),
                ))
        self.assertEqual(error.exception.code, "MISSING_VERTEX")
        self.assertEqual(model.version, version)
        model.undo()
        self.assertEqual(model.mesh_record(MESH).name, "triangle")

    def test_topology_replace_rewrites_all_dependencies_atomically(self):
        model = session()
        with model.edit("topology dependencies") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "source", ASSET, (40, 40), (60, 60))
            edit.create_rectangle(MESH_B, "other", ASSET, (40, 40), (60, 60))
            edit.create_parameter(PARAMETER, "ordinary", 0, 1, 0)
            edit.create_parameter(BLEND_PARAMETER, "shape", 0, 1, 0, kind="blend_shape")
            edit.create_mesh_binding(BINDING, MESH, [kasane.Axis(PARAMETER, [0, 1])], [
                kasane.MeshKeyform([0], [(40, 40), (60, 40), (40, 60), (60, 60)]),
                kasane.MeshKeyform([1], [(41, 40), (61, 40), (41, 60), (61, 60)]),
            ])
            edit.create_blend_key_table(kasane.BlendKeyTableSpec(
                BLEND_TABLE, BLEND_PARAMETER, [0, 1], 0,
            ))
            edit.create_blend_binding(kasane.BlendBindingSpec(
                BLEND_MESH, MESH, "mesh", BLEND_TABLE, [], [
                    kasane.BlendMeshDelta([(0, 0)] * 4),
                    kasane.BlendMeshDelta([(1, 0)] * 4),
                ],
            ))
            edit.create_glue(kasane.GlueSpec(
                GLUE, "seam", MESH, MESH_B, [kasane.GlueVertexPair(0, 0, 1, 1)],
            ))
        source = model.geometry(MESH)
        record = model.mesh_record(MESH)
        mapping = {0: 10, 1: 11, 2: 12, 3: 13}
        new_geometry = record.geometry._replace(
            vertex_ids=[10, 11, 12, 13],
            triangles=[tuple(mapping[vertex] for vertex in triangle)
                       for triangle in record.geometry.triangles],
        )
        replacement = record._replace(geometry=new_geometry)
        binding = model.binding(BINDING)
        blend = model.blend_binding(BLEND_MESH)
        glue = model.glue(GLUE)._replace(
            pairs=[kasane.GlueVertexPair(10, 0, 1, 1)],
        )
        version = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("incomplete map") as edit:
                edit.replace_topology(source, replacement, {0: 10},
                    binding, [blend], [glue])
        self.assertEqual(error.exception.code, "INVALID_VERTEX_MAPPING")
        self.assertEqual(model.version, version)
        with model.edit("topology replacement") as edit:
            edit.replace_topology(source, replacement, mapping,
                binding, [blend], [glue])
        self.assertEqual(model.geometry(MESH).vertex_ids, [10, 11, 12, 13])
        self.assertEqual(model.glue(GLUE).pairs[0].vertex_a, 10)
        self.assertEqual(model.binding(BINDING).id, BINDING)
        self.assertEqual(model.blend_binding(BLEND_MESH).id, BLEND_MESH)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.geometry(MESH).vertex_ids, [10, 11, 12, 13])
        current = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("stale source") as edit:
                edit.replace_topology(source, replacement, mapping,
                    binding, [blend], [glue])
        self.assertEqual(error.exception.code, "STALE_TOPOLOGY")
        self.assertEqual(model.version, current)
        model.undo()
        self.assertEqual(model.geometry(MESH).vertex_ids, [0, 1, 2, 3])
        self.assertEqual(model.glue(GLUE).pairs[0].vertex_a, 0)

    def test_rectangle_grid_generation_and_bound_topology_migration(self):
        model = session()
        with model.edit("rectangle with dependencies") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
            edit.create_rectangle(MESH_B, "neighbor", ASSET, (60, 40), (80, 60))
            edit.create_parameter(PARAMETER, "ordinary", 0, 1, 0)
            edit.create_parameter(BLEND_PARAMETER, "shape", 0, 1, 0, kind="blend_shape")
            edit.create_mesh_binding(BINDING, MESH, [kasane.Axis(PARAMETER, [0, 1])], [
                kasane.MeshKeyform([0], [(40, 40), (60, 40), (60, 60), (40, 60)]),
                kasane.MeshKeyform([1], [(40, 40), (62, 39), (58, 62), (39, 61)]),
            ])
            edit.create_blend_key_table(kasane.BlendKeyTableSpec(
                BLEND_TABLE, BLEND_PARAMETER, [0, 1], 0,
            ))
            edit.create_blend_binding(kasane.BlendBindingSpec(
                BLEND_MESH, MESH, "mesh", BLEND_TABLE, [], [
                    kasane.BlendMeshDelta([(0, 0)] * 4),
                    kasane.BlendMeshDelta([(0, 0), (1, 0), (2, 1), (0, 1)]),
                ],
            ))
            edit.create_glue(kasane.GlueSpec(
                GLUE, "seam", MESH, MESH_B, [kasane.GlueVertexPair(1, 0, 1, 1)],
            ))
        source = model.mesh_record(MESH)
        grid = kasane.rectangle_grid_geometry(source.geometry, 4, 3)
        self.assertEqual(len(grid.vertex_ids), 20)
        self.assertEqual(len(grid.triangles), 24)
        self.assertEqual({0, 1, 2, 3}.intersection(grid.vertex_ids), {0, 1, 2, 3})
        self.assertEqual(grid.uvs[1], (0.25, 0.0))
        self.assertEqual(source.geometry.positions, model.mesh_record(MESH).geometry.positions)
        with self.assertRaisesRegex(ValueError, "equal columns and rows"):
            model.remesh_rectangle_grid(MESH, 4, 3)
        self.assertEqual(model.mesh_record(MESH), source)

        # The native operation must respect the Python edit lifecycle.
        with model.edit("active edit"):
            with self.assertRaises(kasane.SdkFailure) as active:
                model.remesh_rectangle_grid(MESH, 4, 4)
            self.assertEqual(active.exception.code, "EDIT_ACTIVE")
        stale_version = model.version
        with model.edit("rename") as edit:
            edit.rename_mesh(MESH, "renamed face")
        with self.assertRaises(kasane.SdkFailure) as stale:
            model.remesh_rectangle_grid(MESH, 4, 4, stale_version)
        self.assertEqual(stale.exception.code, "STALE_VERSION")
        self.assertEqual(model.mesh_record(MESH).geometry, source.geometry)
        before = model.evaluate({"ordinary": 0.5, "shape": 0.5})
        old_ids = source.geometry.vertex_ids
        old_positions = dict(zip(old_ids, next(d for d in before.drawables if d.id == MESH).positions))
        replacement = model.remesh_rectangle_grid(MESH, 4, 4)
        self.assertEqual(len(replacement.geometry.vertex_ids), 25)
        self.assertEqual(len(replacement.geometry.triangles), 32)
        self.assertEqual(model.binding_for_mesh(MESH).id, BINDING)
        self.assertEqual(len(model.binding(BINDING).keyforms[1].positions), 25)
        self.assertEqual(len(model.blend_binding(BLEND_MESH).keyforms[1].positions), 25)
        self.assertEqual(model.glue(GLUE).pairs[0].vertex_a, 1)
        self.assertEqual(model.validate_structure(), [])
        self.assertEqual(model.diagnose_geometry(), [])
        # The source diagonal remains a grid edge, preserving old poses at
        # every old corner even when the bound quadrilateral is non-affine.
        after = model.evaluate({"ordinary": 0.5, "shape": 0.5})
        new_positions = dict(zip(replacement.geometry.vertex_ids,
                                 next(d for d in after.drawables if d.id == MESH).positions))
        for vertex_id in old_ids:
            self.assertAlmostEqual(new_positions[vertex_id][0], old_positions[vertex_id][0], places=5)
            self.assertAlmostEqual(new_positions[vertex_id][1], old_positions[vertex_id][1], places=5)
        self.assertEqual(model.binding(BINDING).keyforms[1].positions[7], (50.0, 45.25))
        model.undo()
        self.assertEqual(len(model.mesh_record(MESH).geometry.vertex_ids), 4)
        model.redo()
        self.assertEqual(len(model.mesh_record(MESH).geometry.vertex_ids), 25)
        with TemporaryDirectory() as directory:
            saved = model.save(Path(directory).resolve() / "project")
            reopened = kasane.open_project(saved.manifest)
            self.assertEqual(reopened.mesh_record(MESH).geometry, replacement.geometry)
            self.assertEqual(len(reopened.binding(BINDING).keyforms[1].positions), 25)

    def test_png_base_relocation_and_replacement(self):
        model = session()
        with model.edit("asset") as edit:
            edit.add_png_asset_from_base(ASSET, "texture", TEXTURE.parent, Path(TEXTURE.name))
        original = model.asset(ASSET)
        with TemporaryDirectory() as directory:
            relocated = Path(directory).resolve() / "same.png"
            shutil.copyfile(TEXTURE, relocated)
            with self.assertRaises(kasane.SdkFailure) as mismatch:
                with model.edit("bad relocation") as edit:
                    edit.relocate_png_asset(ASSET, EXTERNAL / "texture_00.png")
            self.assertEqual(mismatch.exception.code, "RESOURCE_DIMENSIONS")
            self.assertEqual(model.asset(ASSET), original)
            with model.edit("relocate") as edit:
                edit.relocate_png_asset(ASSET, relocated)
            self.assertEqual(model.asset(ASSET).sha256, original.sha256)
            self.assertEqual(model.asset(ASSET).source, str(relocated))
            with model.edit("replace") as edit:
                edit.replace_png_asset(ASSET, "new texture", EXTERNAL / "texture_00.png")
            self.assertEqual(model.asset(ASSET).width, 4)
            self.assertEqual(model.asset(ASSET).name, "new texture")
            model.undo()
            self.assertEqual(model.asset(ASSET).sha256, original.sha256)

    def test_geometry_diagnostics_are_advisory(self):
        model = session()
        with model.edit("rectangle") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        version = model.version
        issues = model.diagnose_geometry(201, ((0, 0), (50, 50)))
        self.assertEqual(
            sum(issue.kind == "small_triangle" for issue in issues), 2
        )
        self.assertTrue(
            any(issue.kind == "outside_canvas_bounds" for issue in issues)
        )
        self.assertEqual({issue.mesh_id for issue in issues}, {MESH})
        self.assertEqual(model.version, version)
        with self.assertRaises(kasane.SdkFailure) as invalid:
            model.diagnose_geometry(-1)
        self.assertEqual(invalid.exception.code, "INVALID_DIAGNOSTIC_OPTIONS")
        self.assertEqual(model.version, version)

    def test_scene_bindings_cover_all_tracks_and_keyform_updates(self):
        model = session()
        points = [(0, 0), (1, 0), (0, 1), (1, 1)]
        axis = kasane.Axis(PARAMETER, [0, 1])
        with model.edit("scene") as edit:
            edit.create_parameter(PARAMETER, "pose", 0, 1, 0)
            edit.create_part(PART, "part")
            edit.create_rotation_transform(
                ROTATION, "rotation", kasane.RotationData(0, kasane.RotationPose((0, 0)))
            )
            edit.create_warp_transform(
                WARP, "warp", kasane.WarpData(1, 1, True, points)
            )
            edit.create_scene_binding(
                SCENE_PART, "part", PART, [axis], [
                    kasane.ScenePartKeyform([0], 0),
                    kasane.ScenePartKeyform([1], 10),
                ]
            )
            edit.create_scene_binding(
                SCENE_ROTATION, "rotation", ROTATION, [axis], [
                    kasane.SceneRotationKeyform([0], kasane.RotationPose((0, 0))),
                    kasane.SceneRotationKeyform(
                        [1], kasane.RotationPose((0, 0), angle=30),
                        kasane.Appearance(opacity=0.75),
                    ),
                ]
            )
            edit.create_scene_binding(
                SCENE_WARP, "warp", WARP, [axis], [
                    kasane.SceneWarpKeyform([0], points),
                    kasane.SceneWarpKeyform([1], [(x + 1, y) for x, y in points]),
                ]
            )
        self.assertEqual(model.scene_binding_ids(), [SCENE_PART, SCENE_ROTATION, SCENE_WARP])
        self.assertEqual(model.scene_binding(SCENE_PART).keyforms[1].draw_order, 10)
        rotation = model.scene_binding(SCENE_ROTATION)
        self.assertEqual(rotation.keyforms[1].rotation.angle, 30)
        self.assertEqual(rotation.keyforms[1].appearance.opacity, 0.75)
        self.assertEqual(model.binding_for_scene(ROTATION), rotation)
        warp = model.scene_binding(SCENE_WARP)
        self.assertEqual(warp.keyforms[1].positions[0], (1, 0))
        warp.keyforms[1].positions[0] = (999, 999)
        self.assertEqual(model.scene_binding(SCENE_WARP).keyforms[1].positions[0], (1, 0))
        before = model.version
        with model.edit("update scene form") as edit:
            edit.set_scene_keyform(SCENE_PART, kasane.ScenePartKeyform([1], 20))
        self.assertEqual(model.version[2], before[2] + 1)
        self.assertEqual(model.scene_binding(SCENE_PART).keyforms[1].draw_order, 20)
        with model.edit("replace scene") as edit:
            edit.replace_scene_binding(
                SCENE_ROTATION, "rotation", ROTATION, [axis], [
                    kasane.SceneRotationKeyform([0], kasane.RotationPose((0, 0))),
                    kasane.SceneRotationKeyform([1], kasane.RotationPose((0, 0), angle=45)),
                ]
            )
        self.assertEqual(model.scene_binding(SCENE_ROTATION).keyforms[1].rotation.angle, 45)
        snapshot = model.scene_binding(SCENE_ROTATION)
        with model.edit("replace scene from snapshot") as edit:
            edit.replace_scene_binding(snapshot._replace(keyforms=[
                snapshot.keyforms[0],
                snapshot.keyforms[1]._replace(
                    rotation=kasane.RotationPose((0, 0), angle=60)
                ),
            ]))
        self.assertEqual(model.scene_binding(SCENE_ROTATION).keyforms[1].rotation.angle, 60)
        before = model.version
        with self.assertRaises(TypeError):
            with model.edit("wrong scene form") as edit:
                edit.set_scene_keyform(SCENE_PART, object())
        self.assertEqual(model.version, before)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.scene_binding(SCENE_ROTATION).keyforms[1].rotation.angle, 60)
            self.assertEqual(reopened.scene_binding(SCENE_WARP).keyforms[1].positions[0], (1, 0))

    def test_mesh_binding_preserves_appearance_and_replaces_forms(self):
        model = session()
        with model.edit("base") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        base = model.mesh(MESH).positions
        shifted = [(x + 10, y) for x, y in base]
        axis = kasane.Axis(PARAMETER, [0, 1])
        with model.edit("binding") as edit:
            edit.create_parameter(PARAMETER, "open", 0, 1, 0)
            edit.create_mesh_binding(BINDING, MESH, [axis], [
                kasane.MeshKeyform([0], base),
                kasane.MeshKeyform([1], shifted, kasane.Appearance(0.5), 3),
            ])
        binding = model.binding(BINDING)
        self.assertEqual(binding.keyforms[1].appearance.opacity, 0.5)
        self.assertEqual(binding.keyforms[1].draw_order, 3)
        with model.edit("change form") as edit:
            edit.set_mesh_keyform(BINDING, kasane.MeshKeyform(
                [1], shifted, kasane.Appearance(0.75), 5
            ))
        self.assertEqual(model.binding(BINDING).keyforms[1].appearance.opacity, 0.75)
        self.assertEqual(model.binding(BINDING).keyforms[1].draw_order, 5)
        with model.edit("replace binding") as edit:
            edit.replace_mesh_binding(BINDING, MESH, [axis], [
                kasane.MeshKeyform([0], base),
                kasane.MeshKeyform([1], shifted, kasane.Appearance(1), 7),
            ])
        self.assertEqual(model.binding(BINDING).keyforms[1].draw_order, 7)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.binding(BINDING).keyforms[1].draw_order, 7)
            self.assertEqual(reopened.evaluate({PARAMETER: 0.5}).drawables[0].positions[0], (-5, 10))

    def test_mesh_properties_update_preserves_geometry(self):
        model = session()
        with model.edit("base") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        geometry = model.geometry(MESH)
        original = model.mesh_properties(MESH)
        self.assertEqual(original.texture_asset_id, ASSET)
        self.assertEqual(original.blend_mode, "normal")
        updated = kasane.MeshProperties(
            ASSET, kasane.Appearance(0.6, (0.8, 1, 1), (0, 0.1, 0)),
            5, "additive", True, False, True, [],
        )
        with model.edit("draw properties") as edit:
            edit.update_mesh_properties(MESH, updated)
        snapshot = model.mesh_properties(MESH)
        self.assertAlmostEqual(snapshot.appearance.opacity, updated.appearance.opacity)
        for actual, expected in zip(snapshot.appearance.multiply, updated.appearance.multiply):
            self.assertAlmostEqual(actual, expected)
        for actual, expected in zip(snapshot.appearance.screen, updated.appearance.screen):
            self.assertAlmostEqual(actual, expected)
        self.assertEqual(snapshot.draw_order, 5)
        self.assertEqual(snapshot.blend_mode, "additive")
        self.assertFalse(snapshot.double_sided)
        self.assertTrue(snapshot.inverted_mask)
        self.assertEqual(model.geometry(MESH).vertex_ids, geometry.vertex_ids)
        self.assertEqual(model.geometry(MESH).positions, geometry.positions)
        before = model.version
        with self.assertRaises(ValueError):
            with model.edit("invalid blend") as edit:
                edit.rename_mesh(MESH, "should roll back")
                edit.update_mesh_properties(MESH, updated._replace(blend_mode="unknown"))
        self.assertEqual(model.version, before)
        self.assertEqual(model.mesh(MESH).name, "face")
        model.undo()
        self.assertEqual(model.mesh_properties(MESH).appearance, original.appearance)

    def test_runner_reports_exception_line_and_committed_edit(self):
        with TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            script = root / "script.py"
            script.write_text(
                "import kasane\n"
                f"session = kasane.Session('{DOCUMENT}', 100, 100, (50, 50))\n"
                "with session.edit('asset') as edit:\n"
                f"    edit.add_png_asset('{ASSET}', 'texture', {str(TEXTURE)!r})\n"
                "raise ValueError('script failed')\n",
                encoding="utf-8",
            )
            report = root / "report.json"
            environment = os.environ.copy()
            environment["PYTHONPATH"] = ""
            process = subprocess.run(
                [sys.executable, "-m", "kasane", "run", str(script), "--report", str(report)],
                cwd=root,
                env=environment,
                text=True,
                capture_output=True,
                timeout=10,
            )
            self.assertEqual(process.returncode, 1)
            data = json.loads(report.read_text(encoding="utf-8"))
            self.assertEqual(data["status"], "failed")
            self.assertEqual(data["exception"]["line"], 5)
            self.assertEqual(data["exception"]["type"], "ValueError")
            self.assertEqual(data["sessions"][0]["version"][2], 1)
            self.assertTrue(data["sessions"][0]["modified"])


if __name__ == "__main__":
    unittest.main()
