"""Independent package build boundaries and P0 evidence validation."""
from copy import deepcopy
import importlib.util
from pathlib import Path
import sys
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = ROOT / "crates/zipzi-python2"
sys.path.insert(0, str(PACKAGE / "tools"))
SPEC = importlib.util.spec_from_file_location("python2_baseline", PACKAGE / "tools/baseline.py")
baseline = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(baseline)

class Python2ToolsTests(unittest.TestCase):
    def record(self):
        return dict(validated=True, extension_sha256="same", cases={case:
            dict(wall_ns=list(range(1, baseline.SAMPLES + 1)), probe=dict(copied_bytes=None)) for case in baseline.CASES})

    def test_measurement_rejects_missing_cases_samples_validation_and_binary_drift(self):
        run = self.record()
        summary = baseline.summarize([run, deepcopy(run)])
        self.assertEqual(summary["small_query"]["p50_ns"], 16.5)
        self.assertIsNone(summary["large_array"]["probes"][0]["copied_bytes"])
        for corrupt in (
            lambda r: r.update(validated=False),
            lambda r: r.update(extension_sha256="other"),
            lambda r: r["cases"].pop("batch_100"),
            lambda r: r["cases"]["large_array"]["wall_ns"].pop(),
            lambda r: r["cases"]["single_edit"]["wall_ns"].__setitem__(0, 0),
        ):
            broken = deepcopy(run)
            corrupt(broken)
            with self.assertRaises(ValueError):
                baseline.summarize([run, broken])

    def test_client_and_native_are_distinct_distributions_with_matching_versions(self):
        client = tomllib.loads((PACKAGE / "client/pyproject.toml").read_text())
        native = tomllib.loads((PACKAGE / "pyproject.toml").read_text())
        cargo = tomllib.loads((PACKAGE / "Cargo.toml").read_text())
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
        self.assertEqual(cargo["package"]["name"], "zipzi-python2")
        self.assertEqual(client["project"]["name"], "zipzi-python2")
        self.assertEqual(native["project"]["name"], "zipzi-python2-native")
        version = workspace["workspace"]["package"]["version"]
        self.assertEqual(client["project"]["version"], version)
        self.assertIn(f"zipzi-python2=={version}", native["project"]["dependencies"])
        self.assertNotIn("dependencies", client["project"])
        self.assertEqual(native["tool"]["maturin"]["module-name"], "zipzi_native._native")
        self.assertNotIn("zipzi-python", cargo["dependencies"])
        for directory in (PACKAGE, PACKAGE / "client"):
            for name in ("LICENSE-MIT", "THIRD_PARTY_NOTICES.md"):
                self.assertEqual((directory / name).read_bytes(), (ROOT / name).read_bytes())

if __name__ == "__main__":
    unittest.main()
