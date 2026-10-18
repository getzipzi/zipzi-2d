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

class CpuSessionLifecycleTests(unittest.TestCase):
    def test_public_api_survives_module_split(self):
        import pickle
        import typing

        for name in kasane.__all__:
            value = getattr(kasane, name)
            if isinstance(value, type):
                typing.get_type_hints(value)
        records = [kasane.Appearance(), kasane.Axis(PARAMETER, [0, 1]),
                   kasane.SaveResult(Path("/tmp/project"), True, [], [])]
        for record in records:
            self.assertEqual(type(record).__module__, "kasane")
            self.assertEqual(pickle.loads(pickle.dumps(record)), record)
        self.assertEqual(kasane.Session.__module__, "kasane")
        from kasane._session import _sessions
        model = session()
        self.assertIn(model, _sessions)

    def test_unbounded_model_space_and_source_metadata_are_independent(self):
        model = kasane.Session(DOCUMENT)
        self.assertIsNone(model.source_canvas)
        self.assertEqual(model.export_settings.pixels_per_unit,1)
        self.assertAlmostEqual(model.export_settings.padding_fraction,0.1)
        revision = model.evaluation_revision
        reference = kasane.CanvasSnapshot(1000, 800, 500, 400, 100, 1)
        with model.edit("source reference") as edit:
            edit.replace_source_canvas(reference)
        self.assertEqual(model.source_canvas, reference)
        self.assertEqual(model.evaluation_revision, revision)
        with model.edit("model units") as edit:
            edit.replace_export_settings(kasane.ExportSettings(pixels_per_unit=100))
        self.assertEqual(model.source_canvas, reference)
        self.assertEqual(model.export_settings.pixels_per_unit, 100)
        self.assertEqual(model.evaluation_revision, revision)
        with model.edit("remove source") as edit:
            edit.replace_source_canvas(None)
        self.assertIsNone(model.source_canvas)
        model.undo()
        self.assertEqual(model.source_canvas, reference)
        with TemporaryDirectory() as directory:
            saved = model.save(Path(directory).resolve() / "project")
            reopened = kasane.open_project(saved.manifest)
            self.assertEqual(reopened.export_settings, model.export_settings)
            self.assertEqual(reopened.source_canvas, reference)
        model.new_project(DOCUMENT)
        self.assertIsNone(model.source_canvas)
        with self.assertRaises(ValueError):
            kasane.Session(DOCUMENT, width=100)

    def test_model_coordinates_need_no_source_conversion(self):
        model=session()
        with model.edit("rectangle") as edit:
            edit.add_png_asset(ASSET,"texture",TEXTURE)
            edit.create_rectangle(MESH,"mesh",ASSET,(-10,-20),(30,40))
        self.assertEqual(model.geometry(MESH).space,"model")
        self.assertEqual(model.evaluate({}).drawables[0].positions,model.mesh(MESH).positions)

    def test_shared_authoring_contract_matches_expected_results(self):
        spec = json.loads((TEXTURE.parent / "authoring-contract.json").read_text())
        model = kasane.Session(spec["document_id"], 100, 100, (50, 50))
        before = model.version
        with self.assertRaises(kasane.SdkFailure) as failure:
            with model.edit("invalid") as edit:
                edit.create_rectangle(
                    spec["mesh_id"], spec["initial_name"], spec["missing_asset_id"],
                    tuple(spec["minimum"]), tuple(spec["maximum"]),
                )
        self.assertEqual(failure.exception.code, spec["invalid_create_code"])
        self.assertEqual(model.version, before)
        self.assertEqual(model.history_lengths(), (0, 0))
        with model.edit("create") as edit:
            edit.add_png_asset(spec["asset_id"], "texture", TEXTURE)
            edit.create_rectangle(
                spec["mesh_id"], spec["initial_name"], spec["asset_id"],
                tuple(spec["minimum"]), tuple(spec["maximum"]),
            )
        self.assertEqual(model.evaluate({}).drawables[0].positions[0],
                         tuple(spec["evaluated_first_position"]))
        with model.edit("rename") as edit:
            edit.rename_mesh(spec["mesh_id"], spec["updated_name"])
        self.assertEqual(model.mesh(spec["mesh_id"]).name, spec["updated_name"])
        model.undo()
        self.assertEqual(model.mesh(spec["mesh_id"]).name, spec["initial_name"])
        model.redo()
        self.assertEqual(model.mesh(spec["mesh_id"]).name, spec["updated_name"])

    def test_validation_capabilities_identify_the_engine(self):
        capabilities = kasane.capabilities()
        self.assertIn("purism_core_validation", capabilities)
        self.assertFalse(capabilities["official_core_validation"])

    def test_create_save_reopen_and_snapshot_copy(self):
        model = session()
        self.assertEqual(model.source_canvas.origin, (50, 50))
        self.assertEqual(model.uv_v_origin, "bottom")
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        self.assertEqual(model.mesh_ids(), [MESH])
        snapshot = model.mesh(MESH)
        self.assertEqual(snapshot.name, "face")
        snapshot.positions[0] = (999, 999)
        self.assertEqual(model.mesh(MESH).positions[0], (40, 40))
        self.assertEqual(model.evaluate({}).drawables[0].positions[0], (40, 40))
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            result = model.save(destination)
            self.assertTrue(result.manifest.is_file())
            self.assertEqual(model.project_path, result.manifest)
            self.assertFalse(model.modified)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.source_canvas.origin, (50, 50))
            self.assertEqual(reopened.mesh_ids(), [MESH])
            self.assertEqual(reopened.mesh(MESH).positions, model.mesh(MESH).positions)
            self.assertEqual(reopened.mesh(MESH).vertex_ids, model.mesh(MESH).vertex_ids)
            self.assertEqual(reopened.diagnose_resources(), [])
        model.undo()
        self.assertIsNone(model.mesh(MESH))
        model.redo()
        self.assertFalse(model.modified)

    def test_rollback_and_structured_error(self):
        model = session()
        with self.assertRaisesRegex(ValueError, "cancel"):
            with model.edit("cancel") as edit:
                edit.add_png_asset(ASSET, "texture", TEXTURE)
                raise ValueError("cancel")
        self.assertEqual(model.asset_ids(), [])
        with self.assertRaises(kasane.SdkFailure) as failure:
            with model.edit("missing asset") as edit:
                edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        self.assertEqual(failure.exception.code, "MISSING_ASSET")
        self.assertEqual(failure.exception.operation, "create_mesh")
        self.assertEqual(model.mesh_ids(), [])

        with self.assertRaises(kasane.SdkFailure) as aborted:
            with model.edit("caught invalid input") as edit:
                edit.add_png_asset(ASSET, "texture", TEXTURE)
                try:
                    edit.create_rectangle(MESH, "face", ASSET, ("bad", 40), (60, 60))
                except TypeError:
                    pass
        self.assertEqual(aborted.exception.code, "EDIT_ABORTED")
        self.assertEqual(model.asset_ids(), [])

    def test_active_edit_blocks_conflicting_operations_and_releases_on_cancel(self):
        model = session()
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        before = model.version
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            edit = model.edit("rename")
            edit.rename_mesh(MESH, "changed")
            for action in (lambda: model.edit("nested"),
                           lambda: model.save(destination), model.undo,
                           lambda: model.new_project(DOCUMENT, 100, 100, (50, 50))):
                with self.assertRaises(kasane.SdkFailure) as failure:
                    action()
                self.assertEqual(failure.exception.code, "EDIT_ACTIVE")
            self.assertFalse(destination.exists())
            self.assertEqual(model.version, before)
            edit.cancel()
            self.assertEqual(model.mesh(MESH).name, "face")
            with model.edit("rename") as next_edit:
                next_edit.rename_mesh(MESH, "changed")
            self.assertEqual(model.mesh(MESH).name, "changed")

    def test_aborted_edit_cannot_release_a_newer_edit(self):
        model = session()
        old = model.edit("bad")
        with self.assertRaises(kasane.SdkFailure):
            old.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        current = model.edit("current")
        old.cancel()
        del old
        with self.assertRaises(kasane.SdkFailure) as failure:
            model.edit("nested")
        self.assertEqual(failure.exception.code, "EDIT_ACTIVE")
        current.cancel()

    def test_repeated_parameter_replacement_uses_candidate_state(self):
        model = session()
        with model.edit("parameter") as edit:
            edit.create_parameter(PARAMETER, "p", 0, 1, 0.5)
        with model.edit("two replacements") as edit:
            edit.replace_parameter(PARAMETER, "p", 0, 1, 0.5, kind="blend_shape")
            self.assertEqual(edit.parameter(PARAMETER).kind, "blend_shape")
            self.assertEqual(model.parameter(PARAMETER).kind, "normal")
            edit.replace_parameter(PARAMETER, "renamed", 0, 1, 0.5)
            self.assertEqual(edit.parameter(PARAMETER).name, "renamed")
        self.assertEqual(model.parameter(PARAMETER).kind, "blend_shape")
        self.assertEqual(model.parameter(PARAMETER).name, "renamed")
        model.undo()
        self.assertEqual(model.parameter(PARAMETER).kind, "normal")

    def test_full_evaluation_snapshot_exposes_render_attributes(self):
        model = session()
        with model.edit("mesh") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        properties = model.mesh_properties(MESH)
        with model.edit("opacity") as edit:
            edit.update_mesh_properties(MESH, kasane.MeshProperties(
                properties.texture_asset_id, kasane.Appearance(0.5),
                properties.draw_order, properties.blend_mode, properties.enabled,
                properties.double_sided, properties.inverted_mask, properties.masks,
            ))
        snapshot = model.evaluate_snapshot({})
        self.assertEqual(snapshot.version, model.version)
        self.assertEqual(snapshot.source_revision, model.evaluation_revision)
        self.assertEqual(snapshot.drawables[0].positions, model.evaluate({}).drawables[0].positions)
        self.assertEqual(snapshot.drawables[0].opacity, 0.5)
        self.assertTrue(snapshot.drawables[0].visible)
        self.assertEqual(snapshot.drawables[0].texture_asset_id, ASSET)
        self.assertIn(("draw_mesh", MESH), snapshot.render_plan)
        self.assertEqual(model.preview_snapshot().drawables[0].id, MESH)

    def test_update_positions_publishes_one_edit(self):
        model = session()
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        before = model.version
        mesh = model.mesh(MESH)
        shifted = [(x + 10, y) for x, y in mesh.positions]
        with model.edit("move", expected_version=before) as edit:
            edit.update_positions(MESH, mesh.vertex_ids, shifted)
        self.assertEqual(model.version[2], before[2] + 1)
        self.assertEqual(model.evaluate({}).drawables[0].positions[0], (50, 40))

    def test_import_edit_save_and_export(self):
        model = session()
        imported = model.import_model3(EXTERNAL / "model.model3.json")
        self.assertEqual(imported.moc_version, 5)
        self.assertEqual([item.code for item in imported.diagnostics],
                         ["MISSING_PHYSICS_ATTACHMENT", "MISSING_MOTION_ATTACHMENT"])
        self.assertTrue(model.missing_attachments())
        self.assertEqual(model.uv_v_origin, "bottom")
        mesh_id = model.mesh_ids()[0]
        with model.edit("rename") as edit:
            edit.rename_mesh(mesh_id, "changed")
        with TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            model.save(root / "project")
            with self.assertRaises(kasane.SdkFailure) as failure:
                model.export_package(root / "package")
            self.assertEqual(failure.exception.code, "MISSING_PACKAGE_ATTACHMENT")
            reopened = kasane.open_project(root / "project")
            self.assertEqual(reopened.missing_attachments(), model.missing_attachments())
            with model.edit("discard missing references") as edit:
                for path in model.missing_attachments():
                    edit.discard_missing_attachment(path)
            exported = model.export_package(root / "package")
            self.assertTrue(exported.published)
            self.assertTrue((root / "package/model.moc3").is_file())
            self.assertEqual(reopened.mesh(mesh_id).name, "changed")
            self.assertEqual(reopened.diagnose_resources(), [])
        bare = session()
        bare_import = bare.import_bare_moc3(
            EXTERNAL / "model.moc3", {0: EXTERNAL / "texture_00.png"}
        )
        self.assertEqual(bare_import.moc_version, 5)
        self.assertEqual(bare_import.diagnostics, [])

    def test_import_psd_publishes_reopenable_project(self):
        model = session()
        with TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            destination = root / "art-project"
            before = model.version
            with self.assertRaises(kasane.SdkFailure) as failure:
                model.import_psd(root / "missing.psd", destination)
            self.assertEqual(failure.exception.code, "PROJECT_IO")
            self.assertEqual(model.version, before)
            self.assertFalse(destination.exists())

            result = model.import_psd(LAYERED_PSD, destination, before)
            self.assertEqual((result.width, result.height), (8, 8))
            self.assertEqual((result.raster_layers, result.groups), (1, 0))
            self.assertEqual(result.manifest, destination / "project.kasane.json")
            self.assertTrue(result.manifest.is_file())
            self.assertTrue(result.durable)
            self.assertEqual(model.project_path, result.manifest)
            self.assertEqual(model.mesh(model.mesh_ids()[0]).name, "face")
            self.assertEqual(model.uv_v_origin, "bottom")
            self.assertEqual(model.mesh_record(model.mesh_ids()[0]).runtime_id, "face")
            self.assertEqual(model.diagnose_resources(), [])
            reopened = kasane.open_project(result.manifest)
            self.assertEqual(reopened.mesh_ids(), model.mesh_ids())
            self.assertEqual(reopened.diagnose_resources(), [])
            model.export_package(root / "model-package")
            package = session()
            package.import_model3(root / "model-package/model.model3.json")
            self.assertEqual(package.mesh(package.mesh_ids()[0]).name, "face")

            with self.assertRaises(kasane.SdkFailure) as conflict:
                model.import_psd(LAYERED_PSD, destination)
            self.assertEqual(conflict.exception.code, "DESTINATION_EXISTS")
            self.assertEqual(model.version, result.version)

    def test_parameter_binding_samples_and_reopens(self):
        model = session()
        with model.edit("create") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
        base = model.mesh(MESH).positions
        shifted = [(x + 10, y) for x, y in base]
        with model.edit("bind") as edit:
            edit.create_parameter(PARAMETER, "open", 0, 1, 0)
            edit.create_mesh_binding(
                BINDING,
                MESH,
                [kasane.Axis(PARAMETER, [0, 1])],
                [kasane.MeshKeyform([0], base), kasane.MeshKeyform([1], shifted)],
            )
        self.assertEqual(model.parameter_ids(), [PARAMETER])
        self.assertEqual(model.binding_ids(), [BINDING])
        binding = model.binding(BINDING)
        self.assertEqual(binding.mesh_id, MESH)
        self.assertEqual(binding.axes, [kasane.Axis(PARAMETER, [0, 1])])
        self.assertEqual(model.binding_for_mesh(MESH), binding)
        binding.keyforms[0].positions[0] = (999, 999)
        self.assertEqual(model.binding(BINDING).keyforms[0].positions[0], base[0])
        self.assertEqual(model.parameter(PARAMETER).name, "open")
        self.assertEqual(model.parameter_id("open"), PARAMETER)
        self.assertEqual(model.parameter_id(PARAMETER), PARAMETER)
        middle = model.evaluate({PARAMETER: 0.5})
        self.assertEqual(model.evaluate({"open": 0.5}), middle)
        self.assertEqual(model.evaluate_snapshot({"open": 0.5}),
                         model.evaluate_snapshot({PARAMETER: 0.5}))
        with self.assertRaisesRegex(ValueError, "Unknown parameter"):
            model.evaluate({"missing": 0.5})
        with self.assertRaisesRegex(ValueError, "supplied twice"):
            model.evaluate({PARAMETER: 0.5, "open": 0.5})
        self.assertEqual(middle.parameters[0].value, 0.5)
        self.assertEqual(middle.drawables[0].positions[0], (45, 40))
        self.assertEqual(model.evaluate({PARAMETER: 2}).parameters[0].value, 1)
        preview_before = model.preview_revision
        self.assertTrue(model.set_preview_values({PARAMETER: 0.5}))
        self.assertFalse(model.set_preview_values({"open": 0.5}))
        self.assertEqual(model.preview_values, {PARAMETER: 0.5})
        self.assertGreater(model.preview_revision, preview_before)
        self.assertEqual(model.preview_frame().drawables[0].positions[0], (45, 40))
        self.assertGreaterEqual(model.preview_evaluation_count, 1)
        self.assertEqual(model.evaluate({PARAMETER: 0}).drawables[0].positions[0], (40, 40))
        revision = model.preview_revision
        with self.assertRaises(kasane.SdkFailure):
            model.set_preview_parameter("00000000-0000-4000-8000-000000000099", 1)
        self.assertEqual(model.preview_revision, revision)
        self.assertTrue(model.set_preview_parameter(PARAMETER, 1))
        self.assertFalse(model.set_preview_parameter("open", 1))
        self.assertEqual(model.preview_frame().drawables[0].positions[0], (50, 40))
        self.assertTrue(model.reset_preview_values())
        self.assertEqual(model.preview_values, {})
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            model.save(destination)
            reopened = kasane.open_project(destination)
            self.assertEqual(reopened.evaluate({PARAMETER: 0.5}), middle)

    def test_parameter_runtime_id_survives_package_export(self):
        model = session()
        with model.edit("create runtime parameter") as edit:
            edit.add_png_asset(ASSET, "texture", TEXTURE)
            edit.create_rectangle(MESH, "face", ASSET, (40, 40), (60, 60))
            edit.create_parameter(
                PARAMETER, "eye display name", 0, 1, 1,
                runtime_id="ParamEyeLOpen",
            )
        base = model.mesh(MESH).positions
        with model.edit("bind runtime parameter") as edit:
            edit.create_mesh_binding(
                BINDING, MESH, [kasane.Axis(PARAMETER, [0, 1])],
                [kasane.MeshKeyform([0], [(x, y + 5) for x, y in base]),
                 kasane.MeshKeyform([1], base)],
            )
        self.assertEqual(model.parameter(PARAMETER).runtime_id, "ParamEyeLOpen")
        with TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            manifest = model.save(root / "project").manifest
            self.assertEqual(kasane.open_project(manifest).parameter(PARAMETER).runtime_id,
                             "ParamEyeLOpen")
            model.export_package(root / "package")
            imported = session()
            imported.import_model3(root / "package/model.model3.json")
            self.assertEqual(len(imported.parameter_ids()), 1)
            self.assertEqual(imported.parameter(imported.parameter_ids()[0]).runtime_id,
                             "ParamEyeLOpen")
            self.assertEqual(imported.parameter(imported.parameter_ids()[0]).name,
                             "eye display name")
        with model.edit("rename runtime parameter") as edit:
            edit.replace_parameter(PARAMETER, "eye display name", 0, 1, 1,
                                   runtime_id="ParamEyeROpen")
        self.assertEqual(model.parameter(PARAMETER).runtime_id, "ParamEyeROpen")

    def test_save_new_allocates_sibling_without_overwriting(self):
        first = session()
        with TemporaryDirectory() as directory:
            destination = Path(directory).resolve() / "project"
            first_manifest = first.save(destination).manifest
            before = first_manifest.read_bytes()
            second = session()
            with self.assertRaises(kasane.SdkFailure) as failure:
                second.save(destination)
            self.assertEqual(failure.exception.code, "DESTINATION_EXISTS")
            receipt = second.save(destination, on_exists="new")
            self.assertEqual(receipt.manifest.parent.name, "project-1")
            self.assertEqual(first_manifest.read_bytes(), before)
            self.assertTrue(kasane.open_project(receipt.manifest).validate_structure() == [])
            with self.assertRaisesRegex(ValueError, "on_exists"):
                second.save(destination, on_exists="overwrite")
            first_manifest.write_bytes(before + b" ")
            with self.assertRaises(kasane.SdkFailure) as conflict:
                first.save(destination, on_exists="new")
            self.assertEqual(conflict.exception.code, "PROJECT_CONFLICT")

    def test_duplicate_parameter_name_requires_id(self):
        model = session()
        other = "00000000-0000-4000-8000-000000000099"
        with model.edit("duplicate names") as edit:
            edit.create_parameter(PARAMETER, "open", 0, 1, 0)
            edit.create_parameter(other, "open", 0, 1, 0)
        with self.assertRaisesRegex(ValueError, "Ambiguous parameter name"):
            model.evaluate({"open": 0.5})
        self.assertEqual(model.parameter_id(PARAMETER), PARAMETER)
        self.assertEqual(len(model.evaluate({PARAMETER: 0.5}).parameters), 2)
