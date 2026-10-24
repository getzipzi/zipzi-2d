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

import zipzi
from .common import *

class CpuMotionAnimationTests(unittest.TestCase):
    def test_motion_library_plans_keep_ids_migrate_last_registration_and_reject_stale(self):
        model = session()
        clip, registration, group, attachment = (str(uuid4()) for _ in range(4))
        with model.edit("motion library") as edit:
            edit.motion_authoring({"op": "create", "id": clip, "name": "Clip", "registration_id": registration, "destination": {"kind": "root"}})
            edit.motion_authoring({"op": "create_group", "id": group, "name": "Group"})
            edit.attachment_authoring({"op": "create", "id": attachment, "path": "voice.wav"})
            edit.attachment_authoring({"op": "set_sound", "registration": registration, "reference": {"kind": "resolved", "attachment_id": attachment}})
        handle = model.handle("motion_registration", registration)
        plan = model.plan_motion_authoring({"op": "move_registrations", "ids": [registration], "destination": {"kind": "group", "id": group}, "before": None})
        model.execute_motion_plan(plan)
        model.resolve_handle(handle)
        plan = model.plan_motion_authoring({"op": "delete_group", "id": group})
        self.assertEqual(plan.migrated_to_root, [registration])
        model.execute_motion_plan(plan)
        self.assertEqual(model.motion_roots()[0]["id"], registration)
        self.assertEqual(model.motion_roots()[0]["sound"]["attachment_id"], attachment)
        with self.assertRaises(zipzi.SdkFailure):
            model.execute_motion_plan(plan)
        model.undo()
        model.resolve_handle(handle)

    def test_expression_entry_authoring_uses_sdk_identity_and_atomic_history(self):
        model = session()
        a, b, x, y = (str(uuid4()) for _ in range(4))
        with model.edit("expression fixture") as edit:
            edit.create_parameter(PARAMETER, "X", 0, 1, 0, runtime_id="ParamX")
            edit.create_expression(a, "A", [])
            edit.create_expression(b, "B", [])
            edit.create_expression_entry(a, x)
        self.assertEqual(model.expression(a)["entries"][0]["id"], x)
        self.assertEqual(model.expression(a)["entries"][0]["target"], {"kind": "unspecified"})
        handle = model.handle("expression_entry", x)
        affected, entries = model.plan_expression_move([x], b)
        self.assertEqual(set(affected), {a, b})
        self.assertEqual(entries, [x])
        with model.edit("configure and move") as edit:
            edit.patch_expression_entry(x, PARAMETER, 0.4)
            edit.move_expression_entries([x], b)
        model.resolve_handle(handle)
        self.assertAlmostEqual(model.expression(b)["entries"][0]["value"], 0.4)
        self.assertEqual(json.loads(model.export_expression3(b))["Parameters"][0]["Id"], "ParamX")
        with model.edit("copy") as edit:
            edit.duplicate_expression_entries([x], [y], b)
        self.assertEqual([e["id"] for e in model.expression(b)["entries"]], [x, y])
        with model.edit("delete") as edit:
            edit.delete_expression_entries([x])
        model.drain_events()
        model.undo()
        restored = model.drain_events()[0].changes
        self.assertTrue(any(e["id"] == x for e in restored["created"]))
        self.assertFalse(restored["full_invalidation"])
        with self.assertRaises(zipzi.SdkFailure):
            model.resolve_handle(handle)
        self.assertEqual(model.expression(b)["entries"][0]["id"], x)

    def test_motion_operation_identity_is_read_only_and_tracks_success(self):
        model = zipzi.Session(DOCUMENT, 100, 100, (50, 50))
        preview = model.motion_preview()
        preview_id = preview.operation["preview_id"]
        self.assertEqual(preview.operation["sequence"], 0)
        preview.reset()
        self.assertEqual(preview.operation["kind"]["operation"], "reset")
        preview.advance(0)
        self.assertEqual(preview.operation["kind"]["operation"], "advance")
        preview.seek(0)
        self.assertEqual(preview.operation["kind"]["operation"], "seek")
        self.assertEqual(preview.operation["sequence"], 3)
        self.assertEqual(preview.operation["preview_id"], preview_id)
        with self.assertRaises(zipzi.SdkFailure):
            preview.seek(float("nan"))
        self.assertEqual(preview.operation["sequence"], 3)

    def test_motion_registration_and_transactional_seek_cache(self):
        model = session()
        with model.edit("registration cache fixture") as edit:
            edit.create_parameter(PARAMETER, "X", 0, 1, 0, runtime_id="ParamX")
            edit.create_motion(MOTION, "Idle", 4, 30, fade_in=2, fade_out=0)
            edit.create_motion_track(MOTION, {
                "id": MOTION_TRACK,
                "target": {"kind": "parameter", "parameter_id": PARAMETER},
                "initial": {"id": "00000000-0000-4000-8000-0000000000ae", "time": 0, "value": 1},
                "segments": [{"kind": "linear", "end": {"id": "00000000-0000-4000-8000-0000000000af", "time": 4, "value": 1}}],
                "fade_in": None, "fade_out": None, "extensions": {},
            })
            edit.set_motion_groups([{"name": "Idle", "entries": [{"clip_id": MOTION,
                "fade_in": 0, "fade_out": 0, "sound": None, "extensions": {}}]}])
        preview = model.motion_preview()
        preview.schedule_motion_entry("Idle", 0, 0)
        self.assertEqual(preview.advance(0).parameters[PARAMETER], 1)
        preview.seek(2)
        expected = preview.snapshot()
        stats = preview.seek_cache_stats()
        self.assertIsInstance(stats, zipzi.SeekCacheStats)
        self.assertEqual(stats.checkpoints, 2)
        self.assertLessEqual(stats.estimated_bytes, stats.budget_bytes)
        with self.assertRaises(zipzi.SdkFailure) as missing:
            preview.schedule_motion_entry("Idle", 1, 2)
        self.assertEqual(missing.exception.code, "MISSING_MOTION_ENTRY")
        self.assertEqual(preview.seek_cache_stats(), stats)
        def fail(done, total):
            raise ValueError("cancel callback")
        with self.assertRaisesRegex(ValueError, "cancel callback"):
            preview.seek_with_progress(2, fail)
        self.assertEqual(preview.snapshot(), expected)
        self.assertEqual(preview.seek_cache_stats(), stats)
        updates = []
        self.assertEqual(preview.seek_with_progress(2, lambda done, total:
                         updates.append((done, total)) is None), expected)
        self.assertEqual(updates, [(0, 0)])
        self.assertEqual(preview.seek_cache_stats().last_replayed_steps, 0)
        preview.set_seek_cache_budget(0)
        self.assertEqual(preview.seek(2), expected)
        self.assertEqual(preview.seek_cache_stats().last_replayed_steps, 120)
        self.assertEqual(preview.seek_cache_stats().checkpoints, 0)
        preview.set_seek_cache_budget(16 * 1024 * 1024)
        preview.seek(2)
        preview.clear_seek_cache()
        self.assertEqual(preview.seek_cache_stats().checkpoints, 0)

    def test_model3_metadata_and_managed_attachments_roundtrip(self):
        model = session()
        with model.edit("parameter") as edit:
            edit.create_parameter(PARAMETER, "X", -1, 1, 0, runtime_id="ParamX")
        user_data = b'{"Version":3,"UserData":[]}'
        with model.edit("model3") as edit:
            edit.set_model3_settings(
                groups=[{"id":str(uuid4()),"name":"EyeBlink","parameters":[{"id":str(uuid4()),"target":{"kind":"resolved","object_id":PARAMETER}}]}],
                layout={"CenterX":0}, user_data="data.userdata3.json")
            edit.set_package_attachments({"data.userdata3.json": user_data})
        self.assertEqual(model.model3_settings()["user_data"]["kind"], "resolved")
        model.resolve_handle(model.handle("attachment", model.model3_settings()["user_data"]["attachment_id"]))
        self.assertEqual(model.package_attachments()["data.userdata3.json"], user_data)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = zipzi.open_project(destination)
            self.assertEqual(reopened.model3_settings(), model.model3_settings())
            self.assertEqual(reopened.package_attachments(), model.package_attachments())

    def test_model3_uuid_validation_and_history(self):
        model = session()
        with model.edit("parameter and metadata") as edit:
            edit.create_parameter(PARAMETER, "X", -1, 1, 0, runtime_id="ParamX")
            edit.set_model3_settings(groups=[{
                "id": str(uuid4()), "name": "EyeBlink",
                "parameters": [{"id":str(uuid4()), "target":{"kind": "resolved", "object_id": PARAMETER}}],
            }])
        before = model.model3_settings()
        with self.assertRaises(zipzi.SdkFailure) as failure:
            with model.edit("invalid reference") as edit:
                edit.set_model3_settings(groups=[{
                    "id": str(uuid4()), "name": "EyeBlink",
                    "parameters": [{"id":str(uuid4()), "target":{"kind": "resolved", "object_id": CDI_OTHER}}],
                }])
        self.assertEqual(failure.exception.code, "INVALID_MODEL3_GROUPS")
        self.assertEqual(model.model3_settings(), before)
        with self.assertRaises(zipzi.SdkFailure):
            with model.edit("erase referenced parameter") as edit:
                edit.erase_object(PARAMETER)
        with model.edit("rename runtime parameter") as edit:
            edit.replace_parameter(PARAMETER, "X", -1, 1, 0, runtime_id="ParamRenamed")
        self.assertEqual(model.model3_settings(), before)
        model.undo()
        self.assertEqual(model.model3_settings(), before)

    def test_physics_native_channels_plans_and_handle_lifetimes(self):
        model = session()
        asset, channel = str(uuid4()), str(uuid4())
        with model.edit("default physics") as edit:
            group = edit.create_physics(asset, "Rig")
            edit.physics_authoring({"operation": "create_input", "group_id": group,
                                    "id": channel, "before": None})
        native = model.physics()
        self.assertEqual(native["data"]["settings"][0]["inputs"][0]["source"],
                         {"kind": "unspecified"})
        handle = model.handle("physics_input", channel)
        with self.assertRaises(zipzi.SdkFailure) as failure:
            model.export_physics3()
        self.assertEqual(failure.exception.code, "NEEDS_CONFIGURATION")
        plan = model.plan_physics_authoring({"operation": "delete", "ids": [channel],
                                            "node_replacements": {}})
        self.assertIn(channel, plan.affected_ids)
        model.execute_physics_plan(plan)
        model.undo()
        with self.assertRaises(zipzi.SdkFailure):
            model.resolve_handle(handle)
        self.assertEqual(model.physics(), native)
        with self.assertRaises(zipzi.SdkFailure):
            model.execute_physics_plan(plan)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            self.assertEqual(zipzi.open_project(destination).physics(), native)

    def test_physics_authoring_preview_and_project_roundtrip(self):
        model = session()
        other = "00000000-0000-4000-8000-0000000000ae"
        with model.edit("parameters") as edit:
            edit.create_parameter(PARAMETER, "X", -1, 1, 0, runtime_id="ParamX")
            edit.create_parameter(other, "Y", -1, 1, 0, runtime_id="ParamY")
        source = (EXTERNAL.parent / "animation_cpu/thirty.physics3.json").read_text()
        with model.edit("physics") as edit:
            self.assertEqual(edit.import_physics3(PHYSICS, source), [])
        self.assertEqual(model.physics()["id"], PHYSICS)
        self.assertEqual(json.loads(model.export_physics3())["Meta"]["Fps"], 30)
        rig = model.physics_preview()
        for input_value in (0, 1, 1):
            rig.set_parameter(PARAMETER, input_value)
            values = rig.advance(1 / 60)
        self.assertAlmostEqual(values[other], -0.212036729, places=4)
        rig.reset()
        self.assertEqual(rig.parameters()[other], 0)
        preview = model.motion_preview()
        preview.schedule_parameter_input(PARAMETER, 1 / 60, 0)
        preview.schedule_parameter_input(PARAMETER, 2 / 60, 1)
        preview.schedule_parameter_input(PARAMETER, 3 / 60, 1)
        self.assertAlmostEqual(preview.seek(3 / 60).parameters[other], -0.212036729, places=4)
        self.assertEqual(preview.seek(3 / 60), preview.seek(3 / 60))
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = zipzi.open_project(destination)
            self.assertEqual(reopened.physics()["id"], PHYSICS)
            self.assertEqual(reopened.export_physics3(), model.export_physics3())
        with model.edit("erase physics") as edit:
            edit.erase_object(PHYSICS)
        self.assertIsNone(model.physics())
        model.undo()
        self.assertEqual(model.physics()["id"], PHYSICS)

    def test_pose_plans_own_ids_pending_references_moves_and_stale_handles(self):
        model = session()
        pose, group, destination, entry, link = (str(uuid4()) for _ in range(5))
        with model.edit("Pose draft") as edit:
            edit.create_part(PART, "Part")
            edit.pose_authoring({"operation": "create_group", "pose_id": pose, "id": group, "name": "Draft", "before": None})
            edit.pose_authoring({"operation": "create_group", "pose_id": pose, "id": destination, "name": "Destination", "before": None})
            edit.pose_authoring({"operation": "create_entry", "group_id": group, "id": entry, "before": None})
            edit.pose_authoring({"operation": "create_link", "entry_id": entry, "id": link, "before": None})
        source = model.pose()
        self.assertEqual(source["groups"][0]["entries"][0]["id"], entry)
        self.assertEqual(source["groups"][0]["entries"][0]["links"][0]["id"], link)
        with self.assertRaises(zipzi.SdkFailure):
            model.export_pose3()
        handle = model.handle("pose_entry", entry)
        plan = model.plan_pose_authoring({"operation": "move", "ids": [entry], "destination": {"kind": "entries", "group_id": destination}, "before": None})
        self.assertIn(entry, plan.affected_ids)
        model.execute_pose_plan(plan)
        model.resolve_handle(handle)
        with self.assertRaises(zipzi.SdkFailure):
            model.execute_pose_plan(plan)
        plan = model.plan_pose_authoring({"operation": "patch", "id": entry, "expected": {"kind": "entry", "part": {"kind": "unspecified"}}, "fields": {"kind": "entry", "part": {"kind": "resolved", "part_id": PART}}})
        model.execute_pose_plan(plan)
        with model.edit("Remove pending link") as edit:
            edit.pose_authoring({"operation": "delete", "ids": [link]})
        self.assertEqual(json.loads(model.export_pose3())["Groups"][1][0]["Id"], PART)
        with TemporaryDirectory() as directory:
            path = Path(directory).resolve() / "pose-project"
            model.save(path)
            self.assertEqual(zipzi.open_project(path).pose(), model.pose())
        with model.edit("Delete entry") as edit:
            edit.pose_authoring({"operation": "delete", "ids": [entry]})
        model.undo()
        with self.assertRaises(zipzi.SdkFailure):
            model.resolve_handle(handle)

    def test_pose_authoring_preview_and_project_roundtrip(self):
        model = session()
        with model.edit("parts") as edit:
            edit.create_part(PART, "first")
            edit.create_part(CHILD_PART, "second")
        with model.edit("pose") as edit:
            edit.create_pose(POSE, [[(PART, []), (CHILD_PART, [])]], fade_in=0.5)
        self.assertEqual(model.pose()["id"], POSE)
        wire = json.loads(model.export_pose3())
        self.assertEqual(wire["Groups"][0][0]["Id"], PART)
        self.assertEqual(model.motion_preview().snapshot().part_opacities[PART], 1)
        self.assertEqual(model.motion_preview().snapshot().part_opacities[CHILD_PART], 0)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = zipzi.open_project(destination)
            self.assertEqual(reopened.pose()["id"], POSE)
            self.assertEqual(reopened.export_pose3(), model.export_pose3())
        with model.edit("erase pose") as edit:
            edit.erase_object(POSE)
        self.assertIsNone(model.pose())
        model.undo()
        self.assertEqual(model.pose()["id"], POSE)

    def test_motion_authoring_history_and_project_roundtrip(self):
        model = session()
        with model.edit("parameter") as edit:
            edit.create_parameter(PARAMETER, "X", 0, 1, 0, runtime_id="ParamX")
        with model.edit("motion") as edit:
            edit.create_motion(MOTION, "Idle", 1, 30, fade_in=0)
            edit.create_motion_track(MOTION, {
                "id": MOTION_TRACK,
                "target": {"kind": "parameter", "parameter_id": PARAMETER},
                "initial": {"id": "00000000-0000-4000-8000-0000000000ae", "time": 0, "value": 0},
                "segments": [{"kind": "linear", "end": {"id": "00000000-0000-4000-8000-0000000000af", "time": 1, "value": 1}}],
                "fade_in": None, "fade_out": None, "extensions": {},
            })
            edit.set_motion_event(MOTION, {"id": MOTION_EVENT, "time": 0.5,
                                           "value": "你好", "extensions": {}})
            edit.set_motion_groups([{"name": "Idle", "entries": [{"clip_id": MOTION,
                "fade_in": 0.25, "fade_out": None, "sound": None, "extensions": {}}]}])
        self.assertEqual(model.motion_ids(), [MOTION])
        self.assertEqual(model.motion_groups()[0]["entries"][0]["clip_id"], MOTION)
        wire = json.loads(model.export_motion3(MOTION))
        self.assertEqual(wire["Meta"]["TotalUserDataSize"], 6)
        self.assertEqual(wire["Curves"][0]["Segments"][-1], 1)
        version = model.version
        preview = model.motion_preview()
        preview.schedule_motion(MOTION, 0)
        self.assertEqual(preview.advance(0).parameters[PARAMETER], 0)
        # Missing fade-out inherits one second, so this non-looping clip
        # already has half weight halfway to its natural end.
        self.assertAlmostEqual(preview.advance(0.5).parameters[PARAMETER], 0.25, places=5)
        self.assertEqual(preview.snapshot().fired_events[0][2], "你好")
        self.assertEqual(preview.seek(0.75), preview.seek(0.75))
        before_cancel = preview.snapshot()
        updates = []
        with self.assertRaises(zipzi.SdkFailure) as cancelled:
            preview.seek_with_progress(1.0, lambda done, total:
                                       updates.append((done, total)) is None and done < 3)
        self.assertEqual(cancelled.exception.code, "SEEK_CANCELLED")
        self.assertEqual(preview.snapshot(), before_cancel)
        self.assertEqual(updates[0], (0, 60))
        self.assertEqual(preview.seek_with_progress(0.75, lambda done, total: True),
                         preview.seek(0.75))
        self.assertEqual(model.version, version)
        with model.edit("move") as edit:
            edit.move_motion_key(MOTION, MOTION_TRACK, 1, 1, 0.5)
        self.assertEqual(json.loads(model.export_motion3(MOTION))["Curves"][0]["Segments"][-1], 0.5)
        model.undo()
        self.assertEqual(json.loads(model.export_motion3(MOTION))["Curves"][0]["Segments"][-1], 1)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = zipzi.open_project(destination)
            self.assertEqual(reopened.motion_ids(), [MOTION])
            self.assertEqual(reopened.motion_groups()[0]["name"], "Idle")
            self.assertEqual(reopened.motion(MOTION)["events"][0]["value"], "你好")
        with self.assertRaises(zipzi.SdkFailure) as failure:
            with model.edit("invalid motion") as edit:
                edit.move_motion_key(MOTION, MOTION_TRACK, 1, -1, 1)
        self.assertEqual(failure.exception.code, "INVALID_MOTION_POINT")

    def test_motion_loop_event_batches_and_limit(self):
        model = session()
        with model.edit("loop events") as edit:
            edit.create_motion(MOTION, "Loop", 1, 2, looping=True)
            edit.set_motion_event(MOTION, {"id": MOTION_EVENT, "time": 0.5,
                                           "value": "tick", "extensions": {}})
        preview = model.motion_preview()
        preview.schedule_motion(MOTION, 0)
        preview.advance(0)
        self.assertEqual(len(preview.advance(0.75).fired_events), 1)
        self.assertEqual(len(preview.advance(3).fired_events), 2)
        before = preview.snapshot()
        with self.assertRaises(zipzi.SdkFailure) as failure:
            preview.advance(1e10)
        self.assertEqual(failure.exception.code, "EVENT_LIMIT")
        self.assertEqual(preview.snapshot(), before)

    def test_expression_create_replace_and_erase(self):
        model = session()
        with model.edit("parameter") as edit:
            edit.create_parameter(PARAMETER, "X", -1, 1, 0, runtime_id="ParamX")
        with model.edit("create expression") as edit:
            edit.create_expression(EXPRESSION, "Smile", [(PARAMETER, 0.25, "add")])
        with model.edit("replace expression") as edit:
            edit.replace_expression(EXPRESSION, "Smile", [(PARAMETER, 0.75, "overwrite")],
                                    fade_in=0)
        encoded = json.loads(model.export_expression3(EXPRESSION))
        self.assertEqual(encoded["Parameters"][0]["Value"], 0.75)
        self.assertEqual(encoded["FadeInTime"], 0)
        with model.edit("erase expression") as edit:
            edit.erase_object(EXPRESSION)
        self.assertEqual(model.expression_ids(), [])
        model.undo()
        self.assertEqual(model.expression_ids(), [EXPRESSION])

        with model.edit("opaque expression") as edit:
            edit.import_expression3(
                EXPRESSION, "Smile",
                '{"Parameters":[{"Id":"ParamX","Value":0.2}],"Future":"keep"}',
            )
        version = model.version
        with self.assertRaises(zipzi.SdkFailure) as failure:
            with model.edit("unsafe expression edit") as edit:
                edit.replace_expression(EXPRESSION, "Smile", [(PARAMETER, 0.7, "add")])
        self.assertEqual(failure.exception.code, "OPAQUE_EDIT_REQUIRES_IMPORT")
        self.assertEqual(model.version, version)

    def test_expression_import_history_and_save_reopen(self):
        model = session()
        with model.edit("parameter") as edit:
            edit.create_parameter(PARAMETER, "X", -1, 1, 0, runtime_id="ParamX")
        before = model.evaluation_revision
        source = ('{"Type":"Live2D Expression","FadeInTime":0.2,'
                  '"Parameters":[{"Id":"ParamX","Value":0.5,"Blend":"Add"}]}')
        with model.edit("expression") as edit:
            self.assertEqual(edit.import_expression3(EXPRESSION, "Smile", source), [])
        self.assertGreater(model.evaluation_revision, before)
        self.assertEqual(model.expression_ids(), [EXPRESSION])
        encoded = json.loads(model.export_expression3(EXPRESSION))
        self.assertEqual(encoded["Parameters"][0]["Id"], "ParamX")
        self.assertEqual(encoded["FadeInTime"], 0.2)
        authoring_version = model.version
        preview = model.expression_preview()
        preview.schedule_expression(EXPRESSION, 0)
        self.assertEqual(preview.advance(0).parameters[PARAMETER], 0)
        self.assertAlmostEqual(preview.advance(0.1).parameters[PARAMETER], 0.25, places=5)
        self.assertAlmostEqual(preview.advance(0.1).parameters[PARAMETER], 0.5, places=5)
        self.assertAlmostEqual(preview.frame().parameters[0].value, 0.5, places=5)
        self.assertEqual(preview.seek(0.5), preview.seek(0.5))
        self.assertEqual(model.version, authoring_version)
        handle = model.handle("expression", EXPRESSION)
        model.undo()
        with self.assertRaises(zipzi.SdkFailure):
            model.resolve_handle(handle)
        model.redo()
        with self.assertRaises(zipzi.SdkFailure):
            model.resolve_handle(handle)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = zipzi.open_project(destination)
            self.assertEqual(reopened.expression(EXPRESSION)["name"], "Smile")
            self.assertEqual(json.loads(reopened.export_expression3(EXPRESSION)), encoded)

        invalid = '{"Parameters":[{"Id":"ParamX","Value":0.5,"Blend":"Invalid"}]}'
        version = model.version
        with self.assertRaises(zipzi.SdkFailure):
            with model.edit("invalid expression") as edit:
                edit.import_expression3(EXPRESSION, "Smile", invalid)
        self.assertEqual(model.version, version)
        with model.edit("unresolved expression") as edit:
            findings = edit.import_expression3(
                EXPRESSION, "Smile", '{"Parameters":[{"Id":"Missing","Value":0.5}]}'
            )
        self.assertEqual(findings[0].path, "$.Parameters[0].Id")
        with self.assertRaises(zipzi.SdkFailure) as failure:
            model.export_expression3(EXPRESSION)
        self.assertEqual(failure.exception.code, "UNRESOLVED_PARAMETER")

    def test_cdi_edit_import_history_and_save_reopen(self):
        model = session()
        with model.edit("parameters") as edit:
            edit.create_parameter(PARAMETER, "X", -1, 1, 0, runtime_id="ParamX")
            edit.create_parameter(CDI_OTHER, "Y", -1, 1, 0, runtime_id="ParamY")
        evaluation_revision = model.evaluation_revision
        with model.edit("display") as edit:
            edit.create_parameter_group(CDI_GROUP, "Face", "顔")
            edit.set_parameter_group(PARAMETER, CDI_GROUP)
            edit.set_parameter_display_name(PARAMETER, "角度")
            edit.set_combined_parameters(CDI_SET, [PARAMETER, CDI_OTHER])
        self.assertEqual(model.evaluation_revision, evaluation_revision)
        self.assertEqual(model.display_info()["parameter_groups"][0]["runtime_id"], "Face")
        cdi = json.loads(model.export_cdi3())
        self.assertEqual([entry["Id"] for entry in cdi["Parameters"]], ["ParamX", "ParamY"])
        self.assertEqual(cdi["Parameters"][0]["Name"], "角度")
        self.assertEqual(cdi["CombinedParameters"], [["ParamX", "ParamY"]])
        group_handle = model.handle("cdi_parameter_group", CDI_GROUP)
        model.undo()
        with self.assertRaises(zipzi.SdkFailure):
            model.resolve_handle(group_handle)
        model.redo()
        with self.assertRaises(zipzi.SdkFailure):
            model.resolve_handle(group_handle)
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = zipzi.open_project(destination)
            self.assertEqual(json.loads(reopened.export_cdi3()), cdi)

        invalid = '{"Version":3,"Parameters":[{"Id":"ParamX","GroupId":"Missing","Name":"X"}]}'
        before = model.version
        with self.assertRaises(zipzi.SdkFailure):
            with model.edit("invalid CDI") as edit:
                edit.import_cdi3(invalid)
        self.assertEqual(model.version, before)
        with model.edit("unresolved CDI") as edit:
            findings = edit.import_cdi3(
                '{"Version":3,"Parameters":[{"Id":"Absent","GroupId":"","Name":"?"}]}'
            )
        self.assertEqual(findings[0].path, "$.Parameters[0].Id")
        with self.assertRaises(zipzi.SdkFailure) as failure:
            model.export_cdi3()
        self.assertEqual(failure.exception.field_path, "$.Parameters[0].Id")
