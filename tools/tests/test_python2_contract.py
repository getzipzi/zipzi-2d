"""P3A evidence integrity and public-inventory negative controls."""
import importlib.util
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("python2_contract", ROOT / "tools/python/python2_contract.py")
contract = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(contract)


class Python2ContractTools(unittest.TestCase):
    def test_inventory_is_deterministic_and_covers_reachable_inherited_methods(self):
        first = contract.inventory()
        self.assertEqual(first, contract.inventory())
        paths = {row["path"]: row for row in first["symbols"]}
        for path in ("zipzi.Runtime.sessions.create", "zipzi.Session.project.export",
                     "zipzi.Edit.meshes.create", "zipzi.Edit.meshes.get",
                     "zipzi.Session.modeling.sample_async", "zipzi.Buffer.close",
                     "zipzi.scene.Summary.id", "zipzi.parameters.Kind.NORMAL",
                     "zipzi.SdkFailure.code", "zipzi.OutcomeUnknown.pending",
                     "zipzi.Busy.cleanup", "zipzi.events.SessionSubscription.initial",
                     "zipzi.Edit.__exit__", "zipzi.modeling.ModelingCommandCreateMesh"):
            self.assertIn(path, paths)
        self.assertEqual(paths["zipzi.Edit.meshes.create"]["kind"], "method")
        self.assertIn("zipzi.Path", paths)  # incidental root export outside __all__
        self.assertFalse(paths["zipzi.Path"]["explicit_all"])
        self.assertIn("spec", paths["zipzi.Edit.meshes.create"]["signature"])
        self.assertEqual(len(paths), len(first["symbols"]))
        self.assertNotIn("zipzi_native._native", sys.modules)
        self.assertEqual(first["status"], "observed_not_accepted")

    def test_inventory_records_missing_exports_and_new_facade_methods(self):
        client = contract.load_client()
        from zipzi import _lifecycle
        def future_method(self, value: int) -> str:
            return str(value)
        future_method.__module__ = "zipzi._lifecycle"
        with patch.object(client, "__all__", [*client.__all__, "MissingExport"]), \
             patch.object(_lifecycle.Meshes, "future_method", future_method, create=True):
            rows = {row["path"]: row for row in contract.inventory()["symbols"]}
        self.assertEqual(rows["zipzi.MissingExport"]["kind"], "missing_export")
        for owner in ("Session", "Edit"):
            self.assertIn(f"zipzi.{owner}.meshes.future_method", rows)
            self.assertEqual(rows[f"zipzi.{owner}.meshes.future_method"]["target_status"], "needs_review")

    def test_unresolved_hints_are_recorded_not_silently_skipped(self):
        def broken(value: "MissingContractType") -> "MissingResultType":
            pass
        result = contract.inspect_callable(broken, {})
        self.assertIn("MissingContractType", result["signature"])
        self.assertIn("NameError", result["type_hints"])

    def test_evidence_cannot_overwrite_previous_run_and_missing_binary_is_not_verified(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "evidence.json"
            contract.write_json(path, {"status": "failed"})
            with self.assertRaises(FileExistsError):
                contract.write_json(path, {"status": "passed"})
            self.assertEqual(json.loads(path.read_text()), {"status": "failed"})
            missing = contract.artifact_identity(Path(directory) / "missing.so")
            self.assertEqual(missing["status"], "missing")
            self.assertIsNone(missing["sha256"])
            present = contract.artifact_identity(path)
            self.assertEqual(present["status"], "present_unverified")
            self.assertEqual(len(present["sha256"]), 64)

    def test_result_recorder_keeps_failures_subtests_and_skips(self):
        class Sample(unittest.TestCase):
            def test_failure(self):
                self.fail("independent expected value")
            def test_subtests(self):
                for value in (0, 1):
                    with self.subTest(value=value):
                        self.assertEqual(value, 0)
            def test_skip(self):
                self.skipTest("unavailable host")
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(Sample)
        result = unittest.TextTestRunner(stream=io.StringIO(), resultclass=contract.RecordingResult).run(suite)
        self.assertFalse(result.wasSuccessful())
        self.assertEqual(result.testsRun, 3)
        self.assertEqual([row["status"] for row in result.records], ["failed", "unverified", "passed", "failed"])
        self.assertIn("independent expected value", result.records[0]["detail"])

    def test_empty_skipped_expected_failures_or_changed_inputs_cannot_pass(self):
        empty = unittest.TestResult()
        self.assertFalse(contract.accepted(empty, True))
        class Sample(unittest.TestCase):
            @unittest.expectedFailure
            def test_unimplemented(self):
                self.fail("must remain unverified")
        result = unittest.TextTestRunner(stream=io.StringIO(), resultclass=contract.RecordingResult).run(
            unittest.defaultTestLoader.loadTestsFromTestCase(Sample))
        self.assertTrue(result.wasSuccessful())  # unittest's weaker criterion
        self.assertFalse(contract.accepted(result, True))
        self.assertEqual(result.records[0]["status"], "unverified")
        passing = unittest.TestResult()
        passing.testsRun = 1
        self.assertTrue(contract.accepted(passing, True))
        self.assertFalse(contract.accepted(passing, False))
        passing.skipped.append((Sample("test_unimplemented"), "host unavailable"))
        self.assertFalse(contract.accepted(passing, True))


if __name__ == "__main__":
    unittest.main()
