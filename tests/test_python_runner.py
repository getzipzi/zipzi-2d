"""Check that local SDK tests never silently exercise a stale or wrong install."""

from contextlib import redirect_stderr
import importlib.util
import io
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location("test_python_runner_tool", Path(__file__).resolve().parents[1] / "tools/test_python.py")
runner = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(runner)


class PythonRunnerTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.addCleanup(patch.stopall)
        patch.object(runner, "ROOT", self.root).start()

    def test_failed_rebuild_never_runs_tests_against_previous_extension(self):
        def fail_build(command, **kwargs):
            raise subprocess.CalledProcessError(17, command)
        with patch.object(runner, "execute", side_effect=fail_build) as execute:
            with self.assertRaises(subprocess.CalledProcessError):
                runner.main([])
        self.assertEqual(execute.call_count, 1)

    def test_parallel_run_cannot_replace_extension_while_tests_are_active(self):
        directory = self.root / "target/python-tests"
        directory.mkdir(parents=True)
        with runner.exclusive_run(directory / "runner.lock"), patch.object(runner, "execute") as execute:
            with self.assertRaisesRegex(RuntimeError, "Another Python SDK test run"):
                runner.main(["--observe"])
        execute.assert_not_called()

    def test_wrong_capabilities_stop_before_any_tests(self):
        def fail_check(command, **kwargs):
            if "-c" in command:
                raise subprocess.CalledProcessError(1, command)
        with patch.object(runner, "execute", side_effect=fail_check) as execute:
            with self.assertRaises(subprocess.CalledProcessError):
                runner.main(["--observe"])
        self.assertEqual(execute.call_count, 2)

    def test_wheel_gate_cannot_filter_away_acceptance(self):
        with patch.object(runner, "execute") as execute:
            with self.assertRaises(SystemExit) as error, redirect_stderr(io.StringIO()):
                runner.main(["--wheel", "--", "-k", "history"])
        self.assertEqual(error.exception.code, 2)
        execute.assert_not_called()

    def test_development_refreshes_variant_in_one_persistent_isolated_environment(self):
        with patch.dict(os.environ, {"VIRTUAL_ENV": "/unrelated-venv", "PYTHONPATH": "/stale-source"}), patch.object(runner, "execute") as execute:
            runner.main([])
            first = execute.call_args_list
            execute.reset_mock()
            runner.main(["--observe", "--", "-k", "history"])
            second = execute.call_args_list
        self.assertEqual(first[0].kwargs["env"]["UV_PROJECT_ENVIRONMENT"], second[0].kwargs["env"]["UV_PROJECT_ENVIRONMENT"])
        self.assertEqual(first[0].kwargs["env"]["CARGO_TARGET_DIR"], second[0].kwargs["env"]["CARGO_TARGET_DIR"])
        self.assertNotIn("PYTHONPATH", first[0].kwargs["env"])
        self.assertNotIn("VIRTUAL_ENV", first[0].kwargs["env"])
        self.assertIn("develop", first[0].args[0])
        self.assertNotIn("--release", first[0].args[0])
        self.assertIn("observe", second[0].args[0])
        self.assertTrue(any("test_observe.py" in call.args[0] for call in second))
        for call in second[2:]:
            self.assertEqual(call.args[0][-2:], ["-k", "history"])
            self.assertFalse(str(call.kwargs["cwd"]).startswith(str(self.root / "modules")))


if __name__ == "__main__":
    unittest.main()
