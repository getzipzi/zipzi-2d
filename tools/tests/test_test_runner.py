"""Exercise test entrypoint isolation and failure propagation without native builds."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools"))


class TestRunnerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.app = self.root / "crates/zipzi-editor"
        (self.root / "tools").mkdir(parents=True)
        self.app.mkdir(parents=True)
        for name in ["test-editor.sh", "workspace/workspace.sh", "workspace/cargo_domain.py", "workspace/package-domains.json"]:
            (self.root / "tools" / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy((Path(__file__).resolve().parents[2] / "tools").joinpath(name), self.root / "tools" / name)
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name in ["cargo", "python3"]:
            tool = self.bin / name
            tool.write_text(f"#!{sys.executable}\n" + '''import json,os,pathlib,sys
if pathlib.Path(sys.argv[0]).name == 'python3' and pathlib.Path(sys.argv[1]).name == 'cargo_domain.py':
    os.execv(sys.executable, [sys.executable, *sys.argv[1:]])
with open('calls.jsonl','a') as out:
    out.write(json.dumps({'tool':pathlib.Path(sys.argv[0]).name,'args':sys.argv[1:],'session':os.environ.get('ZIPZI_DEV_SESSION_DIR')})+'\\n')
if sys.argv[1:2]==['fmt']:sys.exit(int(os.environ.get('FORMAT_EXIT','0')))
sys.exit(int(os.environ.get('TEST_EXIT','0')))
''')
            tool.chmod(0o755)
        self.env = {**os.environ, "PATH": f"{self.bin}:{os.environ['PATH']}", "ZIPZI_DEV_SESSION_DIR": "live-development-session"}

    def assert_editor_packages(self, args):
        expected = json.loads((self.root / 'tools/workspace/package-domains.json').read_text())['editor']
        actual = [args[i + 1] for i, value in enumerate(args) if value == '-p']
        self.assertCountEqual(actual, expected)
        self.assertNotIn('--workspace', args)
        self.assertNotIn('--all', args)

    def run_entrypoint(self, *args, **environment):
        result = subprocess.run(["sh", "tools/test-editor.sh", *args], cwd=self.root, env={**self.env, **environment}, capture_output=True, text=True)
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

    def test_full_still_runs_tooling_and_complete_editor_domain(self):
        result, calls = self.run_entrypoint()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual([call["tool"] for call in calls], ["cargo", "python3", "cargo"])
        self.assert_editor_packages(calls[-1]["args"])
        self.assert_editor_packages(calls[0]["args"])
        self.assertNotIn("--lib", calls[-1]["args"])
        self.assertIsNone(calls[-1]["session"])

    def test_full_stops_after_tooling_failure(self):
        result, calls = self.run_entrypoint('full', TEST_EXIT='9')
        self.assertEqual(result.returncode, 9)
        self.assertEqual([call['tool'] for call in calls], ['cargo', 'python3'])

    def test_acceptance_keeps_every_native_target_and_propagates_failure(self):
        result, calls = self.run_entrypoint("acceptance", TEST_EXIT="9")
        self.assertEqual(result.returncode, 9)
        self.assertEqual(len(calls), 1)
        for target in ["acceptance", "native_render", "headless_screenshot", "animation_acceptance", "timeline_acceptance"]:
            self.assertIn(target, calls[0]["args"])

    def test_ui_uses_the_full_editor_feature_graph_and_keeps_the_filter(self):
        result, calls = self.run_entrypoint("ui", "headless_tests::ui_performance")
        self.assertEqual(result.returncode, 0, result.stderr)
        args = calls[0]["args"]
        self.assert_editor_packages(args)
        self.assertIn("zipzi-editor/test-support", args)
        self.assertEqual(args[-1], "headless_tests::ui_performance")
        self.assertIsNone(calls[0]["session"])

    def test_absolute_entrypoint_resolves_tool_discovery_from_another_directory(self):
        for cwd in [self.app, self.root.parent]:
            result = subprocess.run(['sh',str(self.root/'tools/test-editor.sh'),'tools','-v'],cwd=cwd,
                env=self.env,capture_output=True,text=True)
            self.assertEqual(result.returncode,0,result.stderr)
            call = json.loads((self.root/'calls.jsonl').read_text().splitlines()[-1])
            self.assertEqual(Path(call['args'][0]).name, 'test_tools.py')
            self.assertNotIn('discover',call['args'])
            self.assertEqual(call['args'][-1],'-v')

    def test_unknown_mode_fails_without_starting_work(self):
        result, calls = self.run_entrypoint("fasst")
        self.assertEqual(result.returncode, 2)
        self.assertEqual(calls, [])


if __name__ == "__main__":
    unittest.main()
