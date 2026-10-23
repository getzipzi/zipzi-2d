"""Exercise test entrypoint isolation and failure propagation without native builds."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


class TestRunnerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / "scripts").mkdir()
        shutil.copy(Path(__file__).with_name("test.sh"), self.root / "scripts/test.sh")
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name in ["cargo", "python3"]:
            tool = self.bin / name
            tool.write_text(f"#!{sys.executable}\n" + '''import json,os,pathlib,sys
with open('calls.jsonl','a') as out:
    out.write(json.dumps({'tool':pathlib.Path(sys.argv[0]).name,'args':sys.argv[1:],'session':os.environ.get('KASANE_DEV_SESSION_DIR')})+'\\n')
if sys.argv[1:2]==['fmt']:sys.exit(int(os.environ.get('FORMAT_EXIT','0')))
sys.exit(int(os.environ.get('TEST_EXIT','0')))
''')
            tool.chmod(0o755)
        self.env = {**os.environ, "PATH": f"{self.bin}:{os.environ['PATH']}", "KASANE_DEV_SESSION_DIR": "live-development-session"}

    def run_entrypoint(self, *args, **environment):
        result = subprocess.run(["sh", "scripts/test.sh", *args], cwd=self.root, env={**self.env, **environment}, capture_output=True, text=True)
        log = self.root / "calls.jsonl"
        calls = [json.loads(line) for line in log.read_text().splitlines()] if log.exists() else []
        return result, calls

    def test_fast_isolates_development_session_and_preserves_filter_arguments(self):
        result, calls = self.run_entrypoint("fast", "timeline::", "--", "--exact")
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(len(calls), 1)
        self.assertIsNone(calls[0]["session"])
        self.assertEqual(calls[0]["args"][-3:], ["timeline::", "--", "--exact"])
        self.assertIn("integration", calls[0]["args"])
        self.assertNotIn("--workspace", calls[0]["args"])

    def test_no_argument_preserves_full_gate_and_stops_on_format_failure(self):
        result, calls = self.run_entrypoint(FORMAT_EXIT="7")
        self.assertEqual(result.returncode, 7)
        self.assertEqual([call["tool"] for call in calls], ["cargo"])
        self.assertEqual(calls[0]["args"][0], "fmt")

    def test_full_still_runs_tooling_and_complete_workspace(self):
        result, calls = self.run_entrypoint()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([call["tool"] for call in calls], ["cargo", "python3", "cargo"])
        self.assertIn("--workspace", calls[-1]["args"])
        self.assertNotIn("--lib", calls[-1]["args"])
        self.assertIsNone(calls[-1]["session"])

    def test_acceptance_keeps_every_native_target_and_propagates_failure(self):
        result, calls = self.run_entrypoint("acceptance", TEST_EXIT="9")
        self.assertEqual(result.returncode, 9)
        self.assertEqual(len(calls), 1)
        for target in ["acceptance", "native_render", "headless_screenshot", "animation_acceptance", "timeline_acceptance"]:
            self.assertIn(target, calls[0]["args"])

    def test_ui_uses_the_full_workspace_feature_graph_and_keeps_the_filter(self):
        result, calls = self.run_entrypoint("ui", "headless_tests::ui_performance")
        self.assertEqual(result.returncode, 0, result.stderr)
        args = calls[0]["args"]
        self.assertIn("--workspace", args)
        self.assertIn("kasane-editor/test-support", args)
        self.assertEqual(args[-1], "headless_tests::ui_performance")
        self.assertIsNone(calls[0]["session"])

    def test_unknown_mode_fails_without_starting_work(self):
        result, calls = self.run_entrypoint("fasst")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main()
