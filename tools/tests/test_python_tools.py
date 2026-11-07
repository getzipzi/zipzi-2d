"""Check that the client and native adapter remain separate distributions."""

from pathlib import Path
import sys
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = ROOT / "crates/zipzi-python"


class PythonToolsTests(unittest.TestCase):
    def test_client_and_native_are_distinct_distributions_with_matching_versions(self):
        client = tomllib.loads((PACKAGE / "client/pyproject.toml").read_text())
        native = tomllib.loads((PACKAGE / "pyproject.toml").read_text())
        cargo = tomllib.loads((PACKAGE / "Cargo.toml").read_text())
        workspace = tomllib.loads((ROOT / "Cargo.toml").read_text())
        self.assertEqual(cargo["package"]["name"], "zipzi-python")
        self.assertEqual(client["project"]["name"], "zipzi")
        self.assertEqual(native["project"]["name"], "zipzi-native")
        version = workspace["workspace"]["package"]["version"]
        sys.path.insert(0, str(ROOT / "tools/packaging"))
        from release import versions
        python_version = versions(version)["python_version"]
        self.assertEqual(client["project"]["version"], python_version)
        self.assertIn(f"zipzi=={python_version}", native["project"]["dependencies"])
        self.assertNotIn("dependencies", client["project"])
        self.assertEqual(native["tool"]["maturin"]["module-name"], "zipzi_native._native")
        self.assertNotIn("zipzi-python", cargo["dependencies"])
        for directory in (PACKAGE, PACKAGE / "client"):
            for name in ("LICENSE-MIT", "THIRD_PARTY_NOTICES.md"):
                self.assertEqual((directory / name).read_bytes(), (ROOT / name).read_bytes())
