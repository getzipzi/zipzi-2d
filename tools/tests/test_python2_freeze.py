"""P3A-0 mappings, target signatures and experiment anti-drift controls."""
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch
import unittest

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/python"))
import python2_targets as targets
import python2_acceptance as acceptance


class Python2Freeze(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        # Small synthetic inputs exercise the rules without retaining historical reports.
        symbols = []
        for name in ("meshes.Snapshot", "parameters.Snapshot", "bindings.Snapshot", "resources.Snapshot"):
            symbols.append({"path": "zipzi." + name, "kind": "export",
                "signature": "(source: Source, geometry: meshes.Geometry) -> None",
                "implementation": "zipzi.fixture.Value"})
        for name in ("ops.Create", "parameters.Patch", "modeling.ModelingCommandCreateMesh",
                     "references.LabelAddress", "Buffer", "ops.ApplyPlan", "Source", "Job"):
            symbols.append({"path": "zipzi." + name, "kind": "export",
                "signature": "() -> None", "implementation": "zipzi.fixture.Value"})
        symbols.append({"path": "zipzi.modeling.ModelingCommandScene", "kind": "export",
            "signature": "(intent: modeling.SceneIntent) -> None", "implementation": "zipzi.fixture.Value"})
        symbols.append({"path": "zipzi.modeling.ModelingIntentScene", "kind": "export",
            "signature": "(value: modeling.SceneIntent) -> None", "implementation": "zipzi.fixture.Value"})
        for name in ("Session.modeling.records", "Session.project.export", "references.LabelAddress.plan",
                     "Buffer.read", "Edit.recover", "Source.read", "Job.start", "Edit.commit"):
            symbols.append({"path": "zipzi." + name, "kind": "method", "signature": "(self) -> None"})
        symbols.extend([
            {"path": "zipzi.meshes.Snapshot.geometry", "kind": "field", "annotation": "meshes.Geometry"},
            {"path": "zipzi.modeling.ModelingIntentScene.value", "kind": "field", "annotation": "modeling.SceneIntent"},
            {"path": "zipzi.modeling.read_Mesh", "kind": "export", "implementation": "zipzi.fixture.read"},
        ])
        temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(temporary.cleanup)
        inventory = Path(temporary.name) / "inventory.json"
        inventory.write_text(json.dumps({"symbols": symbols}))
        for attribute, value in (("INVENTORY", inventory),
                                 ("INVENTORY_SHA256", hashlib.sha256(inventory.read_bytes()).hexdigest())):
            replacement = patch.object(targets, attribute, value)
            replacement.start()
            cls.addClassCleanup(replacement.stop)
        cls.symbol_count = len(symbols)
        cls.catalogue = targets.build()
        with patch.object(acceptance.shutil, "which", return_value=__file__), \
             patch.object(acceptance.subprocess, "check_output", return_value="codex-test-runner"):
            cls.config = acceptance.configuration()

    def test_all_input_symbols_resolved_with_full_method_signatures(self):
        targets.validate(self.catalogue)
        self.assertEqual(targets.build(), self.catalogue)
        self.assertEqual(len(self.catalogue["mapping"]), self.symbol_count)
        self.assertEqual(sum(self.catalogue["decisions"].values()), self.symbol_count)

    def test_domain_ownership_container_projection_and_r4_capabilities(self):
        rows = {row["source"]: row for row in self.catalogue["mapping"]}
        for source, target in {
            "zipzi.ops.Create": "zipzi.meshes.Create",
            "zipzi.parameters.Patch": "zipzi.parameters.ParameterPatch",
            "zipzi.meshes.Snapshot.geometry": "zipzi.meshes.Mesh.geometry",
            "zipzi.modeling.ModelingIntentScene.value": "zipzi.modeling.ModelingCommandScene.intent",
            "zipzi.modeling.ModelingCommandCreateMesh": "zipzi.modeling.ModelingCommandCreateMesh",
            "zipzi.Session.modeling.records": "zipzi.Session.modeling.records",
            "zipzi.Session.project.export": "zipzi.Session.project.export",
            "zipzi.references.LabelAddress.plan": "zipzi.references.LabelAddress.plan",
            "zipzi.Buffer.read": "zipzi.Buffer.read",
        }.items():
            self.assertEqual(rows[source]["target"], target)
        for source in ("zipzi.Edit.recover", "zipzi.modeling.read_Mesh", "zipzi.ops.ApplyPlan", "zipzi.Source.read", "zipzi.Job.start"):
            self.assertEqual(rows[source]["decision"], "remove")
        self.assertIn("intent:", rows["zipzi.modeling.ModelingIntentScene"]["signatures"][0])
        self.assertNotIn("<factory>", json.dumps(self.catalogue))
        self.assertNotIn("_variant:", json.dumps(self.catalogue))

    def test_missing_duplicate_unresolved_or_removed_r4_symbol_is_rejected(self):
        for mutate in (
            lambda c: c["mapping"].pop(),
            lambda c: c["mapping"].append(deepcopy(c["mapping"][0])),
            lambda c: c["mapping"][0].update(decision="needs_review"),
            lambda c: next(r for r in c["mapping"] if r["source"] == "zipzi.modeling.ModelingCommandCreateMesh").update(
                decision="remove", target=None),
        ):
            changed = deepcopy(self.catalogue)
            mutate(changed)
            with self.assertRaises(ValueError):
                targets.validate(changed)

    def test_new_methods_cannot_lose_exact_signatures_or_leak_backend(self):
        for mutate in (
            lambda c: c["new_and_changed_targets"].pop(0),
            lambda c: next(r for r in c["mapping"] if r["source"] == "zipzi.Edit.commit").update(signatures=[]),
            lambda c: next(r for r in c["new_and_changed_targets"] if r["path"] == "zipzi.recover_call").update(signatures=["(method: Any) -> Any"]),
            lambda c: next(r for r in c["new_and_changed_targets"] if r["path"] == "zipzi.Session.edit").update(signatures=["(self, backend: Backend) -> Edit"]),
        ):
            changed = deepcopy(self.catalogue)
            mutate(changed)
            with self.assertRaises(ValueError):
                targets.validate(changed)

    def test_exact_42_run_matrix_and_runner_configuration(self):
        acceptance.validate(self.config)
        self.assertEqual(sum(len(task["environments"]) * len(task["seeds"]) for task in self.config["tasks"]), 42)
        self.assertEqual(self.config["status"], "configuration_frozen_tasks_not_run")
        self.assertEqual(len(self.config["runner"]["executable_sha256"]), 64)
        self.assertFalse(self.config["model"]["fallback"])

    def test_prompts_seeds_thresholds_and_budget_cannot_be_weakened_even_if_rehashed(self):
        for mutate in (
            lambda c: c["tasks"][0]["environments"].pop(),
            lambda c: c["tasks"][0]["seeds"].__setitem__(0, 1),
            lambda c: c["tasks"][0].update(prompt="private answers"),
            lambda c: c["thresholds"].update(minimum_successes=1),
            lambda c: c["thresholds"]["hard_stop"].pop(),
            lambda c: c["budget_per_run"].update(tool_calls=100000),
            lambda c: c["model"].update(fallback=True),
        ):
            changed = deepcopy(self.config)
            mutate(changed)
            changed.pop("configuration_sha256")
            changed["configuration_sha256"] = acceptance.digest(changed)
            with self.assertRaises(ValueError):
                acceptance.validate(changed)


if __name__ == "__main__":
    unittest.main()
