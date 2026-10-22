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

    def test_binding_stable_source_addresses_pending_axes_and_history(self):
        model = session()
        with model.edit("base") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
            edit.create_parameter(PARAMETER, "Open", 0, 1, 0)
            edit.binding_authoring({"Grid": {"Create": {"id": BINDING, "target": {"Mesh": MESH}}}})
        self.assertEqual(model.binding_source(BINDING)["source"]["value"]["axes"], [])
        axis, low, high = (str(uuid4()) for _ in range(3))
        intent = {"Grid": {"Edit": {"owner": BINDING, "operation": {"AddAxis": {"axis": {
            "id": axis, "parameter_id": "", "samples": [{"id": low, "value": 0}, {"id": high, "value": 1}]
        }}}}}}
        plan = model.plan_binding_authoring(intent)
        self.assertEqual(plan.version, model.version)
        self.assertGreater(plan.cost[2], 0)
        self.assertIn(BINDING, plan.affected_ids)
        self.assertEqual(plan.losses, [])
        model.execute_binding_plan(plan)
        handle = model.handle("binding_sample", high)
        address = model.binding_field_address(axis)
        activate = model.plan_binding_field_patch(address, {"AxisParameter": {"parameter_id": PARAMETER, "reduction": None}})
        model.execute_binding_plan(activate)
        source = model.binding_source(BINDING)["source"]["value"]
        self.assertEqual(source["axes"][0]["id"], axis)
        self.assertEqual(source["keyforms"][1]["samples"], [high])
        model.resolve_handle(handle)
        before = model.version
        with self.assertRaises(kasane.SdkFailure) as stale:
            model.plan_binding_field_patch(address, {"AxisParameter": {"parameter_id": "", "reduction": {"Slice": 0}}})
        self.assertEqual(stale.exception.code, "STALE_FIELD_ADDRESS")
        self.assertEqual(model.version, before)
        with TemporaryDirectory() as root:
            result = model.save(Path(root) / "stable")
            reopened = kasane.open_project(result.manifest)
            self.assertEqual(reopened.binding_source(BINDING)["source"], model.binding_source(BINDING)["source"])
        sample_address = model.binding_field_address(high)
        model.execute_binding_plan(model.plan_binding_field_patch(sample_address, {"SampleValue": 0.8}))
        model.undo()
        with self.assertRaises(kasane.SdkFailure):
            model.plan_binding_field_patch(sample_address, {"SampleValue": 0.7})
        self.assertIsNone(model.binding_source(str(uuid4())))

    def test_shared_blend_table_conversion_and_stale_plan_are_atomic(self):
        model = session()
        with model.edit("base") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
            edit.create_parameter(BLEND_PARAMETER, "Blend", -1, 1, 0, kind="blend_shape")
            edit.binding_authoring({"Blend": {"CreateTable": {"table": {"id": BLEND_TABLE, "parameter_id": "", "samples": [], "base_sample_id": ""}}}})
            edit.binding_authoring({"Blend": {"CreateBinding": {"id": BLEND_MESH, "target_id": MESH, "target_kind": "mesh", "key_table_id": BLEND_TABLE}}})
        samples = [str(uuid4()), str(uuid4())]
        for sample, value in zip(samples, [-1, 1]):
            with model.edit("sample") as edit:
                edit.binding_authoring({"Blend": {"Table": {"id": BLEND_TABLE, "operation": {"Insert": {"sample": {"id": sample, "value": value}}}}}})
        activate = {"Blend": {"Table": {"id": BLEND_TABLE, "operation": {"SetParameter": {"parameter_id": BLEND_PARAMETER}}}}}
        plan = model.plan_binding_authoring(activate)
        with model.edit("unrelated") as edit:
            edit.rename_mesh(MESH, "new name")
        before = model.version
        with self.assertRaises(kasane.SdkFailure):
            model.execute_binding_plan(plan)
        self.assertEqual(model.version, before)
        model.execute_binding_plan(model.plan_binding_authoring(activate))
        self.assertEqual(model.binding_source(BLEND_TABLE)["source"]["value"]["base_sample_id"], samples[0])
        self.assertEqual(model.validate_structure(), [])

    def test_binding_conversion_report_and_cancelled_plan_do_not_publish(self):
        import json
        model = session()
        with model.edit("base") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        intent = {"Grid": {"Create": {"id": BINDING, "target": {"Mesh": MESH}}}}
        before = model.version
        control = model.binding_conversion_control()
        plan = model.plan_binding_authoring(intent, control=control)
        report = json.loads(plan.report_json)
        self.assertIn(BINDING, report["affected"])
        self.assertEqual(report["grid_guarantee"], "BaseShape")
        self.assertTrue(any(a["owner"] == BINDING and a["element"] == "Object" for a in report["generated"]))
        self.assertGreater(control.checkpoints, 0)
        control.cancel()
        with self.assertRaises(kasane.SdkFailure) as cancelled:
            model.execute_binding_plan(plan)
        self.assertEqual(cancelled.exception.code, "AUTHORING_CANCELLED")
        self.assertEqual(model.version, before)
        self.assertIsNone(model.binding_source(BINDING))
        with self.assertRaises(kasane.SdkFailure):
            model.plan_binding_authoring(intent, control=control)
        model.execute_binding_plan(model.plan_binding_authoring(intent))
        self.assertIsNotNone(model.binding_source(BINDING))

    def test_reparent_support_and_report_capture_native_snapshot_and_model_tolerance(self):
        model = session()
        with model.edit("base") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (0, 0), (1, 1))
            edit.create_rotation_transform(ROTATION, "parent", kasane.RotationData(0, kasane.RotationPose((0.2, -0.3))))
        edit = {"sources": [MESH], "parent_id": ROTATION, "strategy": "PreserveEditState", "tolerance": 1e-5, "relative_tolerance": 1e-6,
                "context": {"parameter_values": {}, "parameter_domain": [], "edit_time_seconds": 0.5}}
        self.assertEqual(model.reparent_support(edit), "RequiresConversionPlan")
        plan = model.plan_binding_authoring({"Reparent": edit})
        report = json.loads(plan.report_json)
        guarantee = report["reparent_guarantee"]
        self.assertEqual(guarantee["context"]["edit_time_seconds"], 0.5)
        self.assertGreaterEqual(guarantee["reference_scale"], 1)
        self.assertLessEqual(guarantee["error_bound"], guarantee["tolerance"] + guarantee["relative_tolerance"] * guarantee["reference_scale"])
        self.assertTrue(any(row["after"]["parent_id"] == ROTATION for row in report["remapped"]))
        model.execute_binding_plan(plan)
        self.assertEqual(model.geometry(MESH).parent_id, ROTATION)
        model.undo()
        self.assertIsNone(model.geometry(MESH).parent_id)

    def test_organization_collections_copy_dissolve_and_history(self):
        model = session()
        first, nested, destination = (str(uuid4()) for _ in range(3))
        for group, parent in [(first, None), (nested, first), (destination, None)]:
            with model.edit("group") as edit:
                edit.organization_authoring({"operation": "create_group", "group": {
                    "id": group, "runtime_id": "G" + group, "name": "Empty",
                    "parent_id": parent, "extensions": {},
                }, "before": None})
        with model.edit("parameter") as edit:
            edit.create_parameter(PARAMETER, "value", -1, 1, 0)
            edit.organization_authoring({"operation": "move", "ids": [PARAMETER],
                "destination": {"kind": "parameters", "group_id": nested}, "before": None})
        model.set_preview_parameter(PARAMETER, 0.6)
        handle = model.handle("parameter", PARAMETER)
        plan = model.plan_organization_authoring({"operation": "duplicate", "ids": [first],
            "destination": {"kind": "parameter_groups", "parent_id": destination}, "before": None})
        copies = dict(plan.copied_ids)
        self.assertEqual(len(copies), 3)
        self.assertEqual(plan.version, model.version)
        self.assertTrue(json.loads(plan.effects_json)["drawing_preserved"])
        model.execute_organization_plan(plan)
        self.assertNotEqual(model.parameter(copies[PARAMETER]).runtime_id, model.parameter(PARAMETER).runtime_id)
        self.assertAlmostEqual(model.preview_values[PARAMETER], 0.6)
        model.resolve_handle(handle)
        with self.assertRaises(kasane.SdkFailure):
            model.execute_organization_plan(plan)
        with model.edit("dissolve") as edit:
            edit.organization_authoring({"operation": "delete", "ids": [copies[nested]], "policy": "dissolve"})
        rows = model.display_info()["parameters"]
        row = next(row for row in rows if row.get("parameter_id") == copies[PARAMETER])
        self.assertEqual(row["group_id"], copies[first])
        model.undo()
        self.assertEqual(model.validate_structure(), [])
        with TemporaryDirectory() as root:
            saved = model.save(Path(root) / "organized")
            reopened = kasane.open_project(saved.manifest)
            self.assertEqual(reopened.display_info(), model.display_info())

    def test_part_organization_copy_keeps_external_asset_and_deformer(self):
        model = session()
        with model.edit("scene") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (0, 0), (1, 1))
            edit.create_part(PART, "container")
            edit.set_mesh_part(MESH, PART)
        plan = model.plan_organization_authoring({"operation": "duplicate", "ids": [PART],
            "destination": {"kind": "part_members", "part_id": None}, "before": None})
        copies = dict(plan.copied_ids)
        model.execute_organization_plan(plan)
        self.assertEqual(model.mesh_record(copies[MESH]).part_id, copies[PART])
        self.assertEqual(model.mesh_properties(copies[MESH]).texture_asset_id, ASSET)
        copied = model.handle("mesh", copies[MESH])
        with model.edit("delete contents") as edit:
            edit.organization_authoring({"operation": "delete", "ids": [copies[PART]], "policy": "contents"})
        self.assertIsNotNone(model.asset(ASSET))
        model.undo()
        with self.assertRaises(kasane.SdkFailure):
            model.resolve_handle(copied)
        self.assertEqual(model.validate_structure(), [])

    def test_scene_pending_references_projection_and_copy_lifecycle(self):
        model = session()
        mesh, warp, glue = (str(uuid4()) for _ in range(3))
        for object_id, obj in [(mesh, {"kind": "mesh"}), (warp, {"kind": "warp"}),
                               (glue, {"kind": "glue", "a": {"kind": "resolved", "object_id": mesh}})]:
            with model.edit("scene object") as edit:
                edit.scene_authoring({"operation": "create", "id": object_id, "name": "", "object": obj})
        projection = model.scene_projection()
        self.assertEqual(projection["geometry"], [mesh])
        self.assertEqual(projection["drawing"], [])
        self.assertEqual(projection["glues"], [])
        self.assertTrue(any(item["id"] == glue for item in projection["disabled"]))
        address = model.scene_field_address(mesh, "label")
        self.assertEqual(json.loads(address.expected_json)["field"], "label")
        plan = model.plan_scene_field_patch(address, {"field": "label", "value": "Named"})
        self.assertEqual(plan.version, model.version)
        model.execute_scene_plan(plan)
        self.assertEqual(model.mesh(mesh).name, "Named")
        model.undo()
        with self.assertRaises(kasane.SdkFailure):
            model.plan_scene_field_patch(address, {"field": "label", "value": "Other"})
        copy_plan = model.plan_scene_authoring({"operation": "duplicate", "ids": [mesh, glue], "destination": None})
        copies = dict(copy_plan.copied_ids)
        model.execute_scene_plan(copy_plan)
        endpoints = json.loads(model.scene_field_address(copies[glue], "glue_endpoints").expected_json)["value"]
        self.assertEqual(endpoints["a"]["object_id"], copies[mesh])
        self.assertEqual(endpoints["b"]["kind"], "unspecified")
        with TemporaryDirectory() as root:
            saved = model.save(Path(root) / "pending_scene")
            reopened = kasane.open_project(saved.manifest)
            self.assertEqual(reopened.scene_projection(), model.scene_projection())

    def test_drawing_group_role_history_and_glue_management_order(self):
        model = session()
        with model.edit("owner") as edit:
            edit.create_part(PART, "Part")
        roles = model.scene_collection({"kind": "drawing_groups"})
        self.assertTrue(any(role.id == PART and role.kind == "draw_order_group" for role in roles))
        role = model.handle("draw_order_group", PART)
        address = model.drawing_group_address(PART)
        model.execute_scene_plan(model.plan_drawing_range_patch(address, (-5, 5)))
        model.undo()
        with self.assertRaises(kasane.SdkFailure):
            model.plan_drawing_range_patch(address, (-2, 2))
        model.execute_scene_plan(model.plan_scene_authoring({"operation": "dissolve_drawing_group", "owner": PART}))
        self.assertIsNotNone(model.part(PART))
        model.undo()
        with self.assertRaises(kasane.SdkFailure):
            model.resolve_handle(role)
        glues = [str(uuid4()), str(uuid4())]
        for glue in glues:
            with model.edit("glue") as edit:
                edit.scene_authoring({"operation": "create", "id": glue, "name": "", "object": {"kind": "glue"}})
        before = model.evaluation_revision
        model.execute_scene_plan(model.plan_scene_authoring({"operation": "move", "ids": [glues[1]], "destination": {"kind": "glues"}, "before": glues[0]}))
        self.assertEqual([handle.id for handle in model.scene_collection({"kind": "glues"})], list(reversed(glues)))
        self.assertEqual(model.glue_ids(), glues)
        self.assertEqual(model.evaluation_revision, before)
        self.assertEqual(model.validate_structure(), [])

    def test_scene_content_vertex_uv_and_topology_address_lifecycle(self):
        model = session()
        with model.edit("mesh") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (0, 0), (10, 10))
        vertex = model.geometry(MESH).vertex_ids[0]
        address = model.scene_content_address(MESH, {"kind": "vertex_uv", "vertex_id": vertex})
        plan = model.plan_scene_content_patch(address, {"kind": "point", "value": {"x": 0.3, "y": 0.7}})
        self.assertFalse(plan.is_noop)
        model.execute_scene_content_plan(plan)
        self.assertEqual(model.geometry(MESH).uvs[0], (0.30000001192092896, 0.699999988079071))
        with model.edit("remove triangles") as edit:
            edit.scene_content({"operation": "delete_triangles", "mesh_id": MESH, "slots": [0]})
        model.undo()
        with self.assertRaises(kasane.SdkFailure):
            model.plan_scene_content_patch(address, json.loads(address.expected_json))
        control = model.binding_conversion_control()
        control.cancel()
        with self.assertRaises(kasane.SdkFailure):
            model.plan_scene_content({"operation": "delete_triangles", "mesh_id": MESH, "slots": [0]}, control=control)

    def test_glue_pair_pending_ids_copy_delete_save_and_reopen(self):
        model = session()
        glue, pair = str(uuid4()), str(uuid4())
        with model.edit("pending pair") as edit:
            edit.scene_authoring({"operation": "create", "id": glue, "name": "Glue", "object": {"kind": "glue"}})
            edit.scene_content({"operation": "insert_glue_pair", "glue_id": glue, "pair": {
                "id": pair, "vertex_a": None, "vertex_b": None, "weight_a": 0.5, "weight_b": 0.5}, "before": None})
        handle = model.glue_pair_handles(glue)[0]
        self.assertEqual(handle.id, pair)
        self.assertEqual(handle.kind, "glue_vertex_pair")
        self.assertIsNone(model.glue_pairs(glue)[0]["vertex_a"])
        self.assertTrue(any(d["id"] == pair for d in model.scene_projection()["disabled"]))
        plan = model.plan_scene_content({"operation": "duplicate_glue_pairs", "glue_id": glue, "ids": [pair], "before": None})
        copied = json.loads(plan.report_json)["generated_pair_ids"][pair]
        model.execute_scene_content_plan(plan)
        self.assertNotEqual(pair, copied)
        model.execute_scene_content_plan(model.plan_scene_content({"operation": "delete_glue_pairs", "glue_id": glue, "ids": [pair]}))
        model.undo()
        with self.assertRaises(kasane.SdkFailure):
            model.resolve_handle(handle)
        with TemporaryDirectory() as root:
            saved = model.save(Path(root) / "content")
            reopened = kasane.open_project(saved.manifest)
            self.assertEqual(reopened.glue_pairs(glue), model.glue_pairs(glue))
            self.assertEqual(reopened.validate_structure(), [])
