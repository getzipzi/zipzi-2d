"""Tool discovery must find new modules from any cwd and propagate failures."""

from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


class ToolDiscoveryTests(unittest.TestCase):
    def test_discovers_new_tests_and_propagates_status_outside_checkout(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "checkout with spaces"
            (root / "tools").mkdir(parents=True)
            (root / "tools/tests").mkdir()
            runner = root / "tools/test_tools.py"
            shutil.copy(Path(__file__).resolve().parents[2] / "tools/test_tools.py", runner)
            fixture = root / "tools/tests/test_new_tool.py"
            for valid in [True, False]:
                with self.subTest(valid=valid):
                    fixture.write_text(
                        "import unittest\nclass NewTool(unittest.TestCase):\n"
                        f"    def test_new(self): self.assertTrue({valid})\n"
                    )
                    result = subprocess.run(
                        [sys.executable, str(runner), "-p", "test_new_tool.py", "-v"],
                        cwd=temporary, capture_output=True, text=True,
                    )
                    self.assertEqual(result.returncode, 0 if valid else 1, result.stderr)
                    self.assertIn("Ran 1 test", result.stderr)
