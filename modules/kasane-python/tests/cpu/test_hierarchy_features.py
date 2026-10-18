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

class CpuHierarchyFeaturesTests(unittest.TestCase):
    def test_queries_geometry_history_and_events(self):
        model = session()
        self.assertEqual(model.document_id, DOCUMENT)
        self.assertEqual(model.source_canvas.width, 100)
        self.assertEqual(model.source_canvas.origin_x, 50)
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        self.assertEqual(model.validate_structure(), [])
        self.assertEqual(model.asset(ASSET).width, 2)
        self.assertEqual(model.asset(ASSET).height, 2)
        self.assertEqual(model.references_to(ASSET), [MESH])
        self.assertEqual(model.binding_ids(), [])
        self.assertEqual(model.part_ids(), [])
        self.assertEqual(model.transform_ids(), [])
        self.assertEqual(model.scene_binding_ids(), [])
        self.assertEqual(model.blend_key_table_ids(), [])
        self.assertEqual(model.blend_constraint_ids(), [])
        self.assertEqual(model.blend_binding_ids(), [])
        self.assertEqual(model.glue_ids(), [])
        self.assertEqual(model.offscreen_ids(), [])
        self.assertEqual(model.find_meshes_by_name("face")[0].id, MESH)
        self.assertEqual(model.require_unique_mesh("face").id, MESH)
        with self.assertRaises(kasane.SdkFailure) as failure:
            model.require_unique_mesh("missing")
        self.assertEqual(failure.exception.code, "NOT_FOUND")
        geometry = model.geometry(MESH)
        self.assertEqual(geometry.space, "model")
        self.assertEqual(geometry.parent_id, None)
        self.assertEqual(geometry.version, model.version)
        self.assertEqual(len(geometry.uvs), 4)
        self.assertEqual(len(geometry.triangles), 2)
        geometry.positions[0] = (999, 999)
        self.assertEqual(model.geometry(MESH).positions[0], (40, 40))
        self.assertEqual(model.history_lengths(), (1, 0))
        self.assertEqual(model.history_state().undo_steps, 1)
        self.assertGreater(model.estimated_content_bytes(), 0)
        self.assertGreaterEqual(model.evaluation_revision, 1)
        events = model.drain_events()
        self.assertEqual(events[0].label, "create")
        self.assertEqual(model.drain_events(), [])

    def test_new_project_rejects_stale_version_and_resets_session(self):
        model = session()
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        before = model.version
        stale = (before[0], before[1], before[2] + 1)
        with self.assertRaises(kasane.SdkFailure) as failure:
            model.new_project(DOCUMENT, 200, 200, (100, 100), stale)
        self.assertEqual(failure.exception.code, "STALE_VERSION")
        self.assertEqual(model.version, before)
        after = model.new_project(DOCUMENT, 200, 200, (100, 100), before)
        self.assertEqual(after[1], before[1] + 1)
        self.assertEqual(model.source_canvas.width, 200)
        self.assertEqual(model.asset_ids(), [])
        self.assertEqual(model.mesh_ids(), [])
        self.assertEqual(model.history_lengths(), (0, 0))
        self.assertEqual(model.preview_values, {})
        self.assertIsNone(model.project_path)

    def test_canvas_parameter_replace_and_erase_are_atomic(self):
        model = session()
        with model.edit("create") as edit:
            edit.create_parameter(PARAMETER, "old", 0, 1, 0)
        before = model.version
        with model.edit("replace") as edit:
            edit.replace_export_settings(kasane.ExportSettings(pixels_per_unit=20))
            edit.replace_source_canvas(kasane.CanvasSnapshot(200, 100, 100, 50, 20, 0))
            edit.replace_parameter(PARAMETER, "new", -1, 1, 0.5)
        self.assertEqual(model.version[2], before[2] + 1)
        self.assertEqual(model.source_canvas.width, 200)
        self.assertEqual(model.parameter(PARAMETER).name, "new")
        self.assertEqual(model.parameter(PARAMETER).minimum, -1)
        model.undo()
        self.assertEqual(model.source_canvas.width, 100)
        self.assertEqual(model.parameter(PARAMETER).name, "old")
        model.redo()
        with model.edit("erase") as edit:
            edit.erase_object(PARAMETER)
        self.assertIsNone(model.parameter(PARAMETER))
        model.undo()
        self.assertEqual(model.parameter(PARAMETER).name, "new")

    def test_parent_edit_errors_roll_back_entire_batch(self):
        operations = [
            lambda edit: edit.set_organization_parent("missing", ""),
            lambda edit: edit.set_transform_parent("missing", None),
            lambda edit: edit.set_transform_part("missing", None),
            lambda edit: edit.set_deform_parent("missing", "parent"),
            lambda edit: edit.set_mesh_part("missing", "part"),
        ]
        for operation in operations:
            with self.subTest(operation=operation):
                model = session()
                before = model.version
                with self.assertRaises(kasane.SdkFailure):
                    with model.edit("bad parent") as edit:
                        edit.create_parameter(PARAMETER, "temporary", 0, 1, 0)
                        operation(edit)
                self.assertEqual(model.version, before)
                self.assertEqual(model.parameter_ids(), [])

    def test_handles_keep_identity_and_expire_after_undo(self):
        model = session()
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        handle = model.handle("mesh", MESH)
        self.assertEqual(handle.id, MESH)
        self.assertEqual(handle.kind, "mesh")
        model.resolve_handle(handle)
        self.assertEqual(model.mesh_by_handle(handle), model.mesh(MESH))
        with model.edit("rename") as edit:
            edit.rename_mesh(MESH, "renamed")
        self.assertEqual(model.mesh_by_handle(handle).name, "renamed")
        with self.assertRaises(kasane.SdkFailure) as wrong_kind:
            model.mesh_by_handle(model.handle("asset", ASSET))
        self.assertEqual(wrong_kind.exception.code, "WRONG_OBJECT_KIND")
        with self.assertRaises(ValueError):
            model.handle("bad kind", MESH)
        model.undo()
        model.undo()
        model.redo()
        with self.assertRaises(kasane.SdkFailure) as stale:
            model.resolve_handle(handle)
        self.assertEqual(stale.exception.code, "STALE_HANDLE")

    def test_draw_order_groups_and_history_limits(self):
        model = kasane.Session.with_history_limits(DOCUMENT, 100, 100, (50, 50), 1, 10**7)
        self.assertEqual(model.history_state().max_steps, 1)
        self.assertIsNone(model.draw_order_groups)
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        group = kasane.DrawOrderGroup("", [MESH], -100, 100)
        with model.edit("order") as edit:
            edit.replace_draw_order_groups([group])
        self.assertEqual(model.draw_order_groups, [group])
        self.assertEqual(model.history_lengths(), (1, 0))
        with self.assertRaises(kasane.SdkFailure) as invalid:
            with model.edit("invalid order") as edit:
                edit.replace_draw_order_groups([kasane.DrawOrderGroup("", [], 0, 0)])
        self.assertEqual(invalid.exception.code, "INVALID_DRAW_GROUP")
        self.assertEqual(model.draw_order_groups, [group])
        model.undo()
        self.assertIsNone(model.draw_order_groups)

    def test_parts_and_organization_parent_publish(self):
        model = session()
        with model.edit("base") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        with model.edit("parts") as edit:
            edit.create_part(PART, "root")
            edit.create_part(CHILD_PART, "child")
            edit.set_organization_parent(CHILD_PART, PART)
            edit.set_mesh_part(MESH, CHILD_PART)
        self.assertEqual(model.part_ids(), [PART, CHILD_PART])
        child = model.part(CHILD_PART)
        self.assertEqual(child.parent_id, PART)
        self.assertEqual(child.runtime_id, CHILD_PART)
        self.assertEqual(model.validate_structure(), [])
        with model.edit("rename part") as edit:
            edit.replace_part(CHILD_PART, "renamed", PART, False, 2.5)
        self.assertEqual(model.part(CHILD_PART).name, "renamed")
        self.assertEqual(model.part(CHILD_PART).runtime_id, CHILD_PART)
        self.assertFalse(model.part(CHILD_PART).enabled)
        self.assertEqual(model.part(CHILD_PART).draw_order, 2.5)
        model.undo()
        self.assertEqual(model.part(CHILD_PART).name, "child")

    def test_transform_hierarchy_and_updates(self):
        model = session()
        with model.edit("base") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (0, 0), (1, 1))
        points = [(0, 0), (1, 0), (0, 1), (1, 1)]
        with model.edit("hierarchy") as edit:
            edit.create_part(PART, "root")
            edit.create_part(CHILD_PART, "child")
            edit.create_rotation_transform(
                ROTATION, "rotate", kasane.RotationData(0, kasane.RotationPose((0, 0))), PART
            )
            edit.create_warp_transform(
                WARP, "warp", kasane.WarpData(1, 1, True, points), PART
            )
            edit.set_transform_parent(WARP, ROTATION)
            edit.set_transform_part(WARP, CHILD_PART)
            edit.set_deform_parent(MESH, WARP)
            edit.set_mesh_part(MESH, CHILD_PART)
        self.assertEqual(model.transform_ids(), [ROTATION, WARP])
        self.assertEqual(model.transform(WARP).parent_id, ROTATION)
        self.assertEqual(model.transform(WARP).part_id, CHILD_PART)
        self.assertEqual(model.transform(WARP).warp.points, points)
        self.assertEqual(model.geometry(MESH).space, "warp_parameter")
        self.assertEqual(model.geometry(MESH).parent_id, WARP)
        self.assertEqual(model.validate_structure(), [])
        with model.edit("update") as edit:
            edit.update_rotation(
                ROTATION, kasane.RotationData(5, kasane.RotationPose((0, 0), angle=10))
            )
            edit.update_warp_points(WARP, [(x + 0.1, y) for x, y in points])
        self.assertEqual(model.transform(ROTATION).rotation.base_angle, 5)
        self.assertEqual(model.transform(ROTATION).rotation.pose.angle, 10)
        self.assertAlmostEqual(model.transform(WARP).warp.points[0][0], 0.1, places=5)
        model.undo()
        self.assertEqual(model.transform(ROTATION).rotation.base_angle, 0)

    def test_replace_transform_preserves_identity_and_full_fields(self):
        model = session()
        points = [(0, 0), (1, 0), (0, 1), (1, 1)]
        with model.edit("transforms") as edit:
            edit.create_rotation_transform(
                ROTATION, "rotate", kasane.RotationData(0, kasane.RotationPose((0, 0)))
            )
            edit.create_warp_transform(
                WARP, "warp", kasane.WarpData(1, 1, True, points)
            )
        original = model.transform(ROTATION)
        warp = model.transform(WARP)
        with model.edit("replace transforms") as edit:
            edit.replace_transform(original._replace(
                name="turned", enabled=False,
                appearance=kasane.Appearance(0.5),
                rotation=kasane.RotationData(15, kasane.RotationPose((2, 3), angle=20)),
            ))
            edit.replace_transform(warp._replace(
                name="curved", warp=kasane.WarpData(1, 1, True, [
                    (0.25, 0), (1, 0), (0, 1), (1, 1)
                ]),
            ))
        replaced = model.transform(ROTATION)
        self.assertEqual(replaced.runtime_id, original.runtime_id)
        self.assertEqual(replaced.name, "turned")
        self.assertFalse(replaced.enabled)
        self.assertAlmostEqual(replaced.appearance.opacity, 0.5)
        self.assertEqual(replaced.rotation.base_angle, 15)
        self.assertEqual(replaced.rotation.pose.origin, (2, 3))
        self.assertAlmostEqual(model.transform(WARP).warp.points[0][0], 0.25)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.transform(ROTATION).runtime_id, original.runtime_id)
            self.assertAlmostEqual(reopened.transform(ROTATION).appearance.opacity, 0.5)
        version = model.version
        with self.assertRaises(ValueError):
            with model.edit("invalid transform") as edit:
                edit.replace_transform(original._replace(kind="warp"))
        self.assertEqual(model.version, version)
        model.undo()
        self.assertEqual(model.transform(ROTATION).name, original.name)

    def test_offscreen_create_replace_and_rollback(self):
        model = session()
        with model.edit("part and offscreen") as edit:
            edit.create_part(PART, "owner")
            edit.create_offscreen(kasane.OffscreenSpec(
                OFFSCREEN, "layer", PART, keyforms=[kasane.OffscreenKeyform(0.5)],
            ))
        original = model.offscreen(OFFSCREEN)
        self.assertEqual(original.runtime_id, OFFSCREEN)
        self.assertEqual(original.keyforms[0].opacity, 0.5)
        original.keyforms[0] = kasane.OffscreenKeyform(0)
        self.assertEqual(model.offscreen(OFFSCREEN).keyforms[0].opacity, 0.5)
        with model.edit("replace offscreen") as edit:
            edit.replace_offscreen(original._replace(
                name="soft layer", flags=0, part_keyform_indices=[0],
                keyforms=[kasane.OffscreenKeyform(0.75, (0.8, 1, 1), (0, 0.1, 0))],
            ))
        replaced = model.offscreen(OFFSCREEN)
        self.assertEqual(replaced.name, "soft layer")
        self.assertEqual(replaced.runtime_id, OFFSCREEN)
        self.assertEqual(replaced.part_keyform_indices, [0])
        self.assertAlmostEqual(replaced.keyforms[0].opacity, 0.75)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.offscreen(OFFSCREEN).part_keyform_indices, [0])
        version = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("invalid index") as edit:
                edit.replace_offscreen(replaced._replace(part_keyform_indices=[9]))
        self.assertEqual(error.exception.code, "INDEX_OUT_OF_BOUNDS")
        self.assertEqual(model.version, version)
        model.undo()
        self.assertEqual(model.offscreen(OFFSCREEN).name, "layer")

    def test_part_binding_and_offscreen_resize_atomically(self):
        model = session()
        with model.edit("base") as edit:
            edit.create_parameter(PARAMETER, "switch", 0, 1, 0)
            edit.create_part(PART, "owner")
            edit.create_scene_binding(
                SCENE_PART, "part", PART, [kasane.Axis(PARAMETER, [0, 1])], [
                    kasane.ScenePartKeyform([0], 0),
                    kasane.ScenePartKeyform([1], 10),
                ],
            )
            edit.create_offscreen(kasane.OffscreenSpec(
                OFFSCREEN, "layer", PART, part_keyform_indices=[0, 1],
                keyforms=[kasane.OffscreenKeyform(0.5), kasane.OffscreenKeyform(1)],
            ))
        binding = model.scene_binding(SCENE_PART)
        offscreen = model.offscreen(OFFSCREEN)
        resized_binding = binding._replace(
            axes=[kasane.Axis(PARAMETER, [0, 0.5, 1])],
            keyforms=[
                kasane.ScenePartKeyform([0], 0),
                kasane.ScenePartKeyform([0.5], 5),
                kasane.ScenePartKeyform([1], 10),
            ],
        )
        resized_offscreen = offscreen._replace(part_keyform_indices=[0, -1, 1])
        old_version = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("invalid independent resize") as edit:
                edit.replace_scene_binding(
                    resized_binding.id, resized_binding.kind, resized_binding.target_id,
                    resized_binding.axes, resized_binding.keyforms,
                )
        self.assertEqual(error.exception.code, "INVALID_LENGTH")
        self.assertEqual(model.version, old_version)
        with model.edit("joint resize") as edit:
            edit.replace_part_binding_with_offscreen(resized_binding, resized_offscreen)
        self.assertEqual(len(model.scene_binding(SCENE_PART).keyforms), 3)
        self.assertEqual(model.offscreen(OFFSCREEN).part_keyform_indices, [0, -1, 1])
        model.undo()
        self.assertEqual(len(model.scene_binding(SCENE_PART).keyforms), 2)
        self.assertEqual(model.offscreen(OFFSCREEN).part_keyform_indices, [0, 1])

    def test_glue_create_replace_binding_and_rollback(self):
        model = session()
        with model.edit("meshes") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "a", ASSET, (0, 0), (1, 1))
            edit.create_rectangle(MESH_B, "b", ASSET, (0, 0), (1, 1))
            edit.create_parameter(PARAMETER, "intensity", 0, 1, 0)
            edit.create_glue(kasane.GlueSpec(
                GLUE, "seam", MESH, MESH_B,
                [kasane.GlueVertexPair(0, 0, 1, 1)],
            ))
        glue = model.glue(GLUE)
        self.assertEqual(glue.runtime_id, GLUE)
        self.assertEqual(glue.pairs[0].vertex_a, 0)
        glue.pairs.clear()
        self.assertEqual(len(model.glue(GLUE).pairs), 1)
        with model.edit("bind glue") as edit:
            edit.replace_glue(glue._replace(
                name="animated seam", intensity=0.5,
                pairs=[kasane.GlueVertexPair(0, 0, 0.75, 1)],
                binding=kasane.GlueBinding([kasane.Axis(PARAMETER, [0, 1])], [0, 1]),
            ))
        updated = model.glue(GLUE)
        self.assertEqual(updated.name, "animated seam")
        self.assertEqual(updated.runtime_id, GLUE)
        self.assertEqual(updated.binding.intensities, [0, 1])
        self.assertAlmostEqual(updated.pairs[0].weight_a, 0.75)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.glue(GLUE).binding.intensities, [0, 1])
        version = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("invalid glue") as edit:
                edit.replace_glue(updated._replace(pairs=[kasane.GlueVertexPair(99, 0, 1, 1)]))
        self.assertEqual(error.exception.code, "MISSING_VERTEX")
        self.assertEqual(model.version, version)
        model.undo()
        self.assertEqual(model.glue(GLUE).name, "seam")

    def test_blend_table_constraint_and_parameter_kind(self):
        model = session()
        with model.edit("parameters and blend metadata") as edit:
            edit.create_parameter(BLEND_PARAMETER, "shape", 0, 1, 0, kind="blend_shape")
            edit.create_parameter(PARAMETER, "limit", 0, 1, 0)
            edit.create_blend_key_table(kasane.BlendKeyTableSpec(
                BLEND_TABLE, BLEND_PARAMETER, [0, 1], 0,
            ))
            edit.create_blend_constraint(kasane.BlendConstraintSpec(
                BLEND_CONSTRAINT, PARAMETER, [0, 1], [1, 1],
            ))
        self.assertEqual(model.parameter(BLEND_PARAMETER).kind, "blend_shape")
        table = model.blend_key_table(BLEND_TABLE)
        constraint = model.blend_constraint(BLEND_CONSTRAINT)
        table.keys[0] = 0.25
        self.assertEqual(model.blend_key_table(BLEND_TABLE).keys, [0, 1])
        with model.edit("replace blend metadata") as edit:
            edit.replace_blend_key_table(table._replace(keys=[0, 0.5, 1], base_key_idx=1))
            edit.replace_blend_constraint(constraint._replace(weights=[0.5, 1]))
        self.assertEqual(model.blend_key_table(BLEND_TABLE).base_key_idx, 1)
        self.assertEqual(model.blend_constraint(BLEND_CONSTRAINT).weights, [0.5, 1])
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.blend_key_table(BLEND_TABLE).keys, [0, 0.5, 1])
            self.assertEqual(reopened.parameter(BLEND_PARAMETER).kind, "blend_shape")
        version = model.version
        with self.assertRaises(kasane.SdkFailure) as error:
            with model.edit("invalid constraint") as edit:
                edit.replace_blend_constraint(constraint._replace(weights=[1]))
        self.assertEqual(error.exception.code, "INVALID_LENGTH")
        self.assertEqual(model.version, version)
        model.undo()
        self.assertEqual(model.blend_key_table(BLEND_TABLE).keys, [0, 1])
