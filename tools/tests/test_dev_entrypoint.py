"""Native controls for the unified test dispatcher; no shell stubs required."""
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import dev
from dev_platform import file_lock


class Entrypoint(unittest.TestCase):
    def test_full_format_failure_does_not_prepare_client(self):
        with patch.object(dev.cargo_domain, 'validate'), patch.object(dev.subprocess, 'call', return_value=7), patch.object(dev.subprocess, 'check_output') as prepare:
            self.assertEqual(dev.main(['test', 'editor', 'full']), 7)
            prepare.assert_not_called()

    def test_caller_relative_target_and_harness_arguments(self):
        with patch.object(dev.cargo_domain, 'validate'), patch.dict(os.environ, CARGO_TARGET_DIR='中文 target'), patch.object(dev, 'run_commands', return_value=0) as run:
            dev.main(['test', 'sdk', 'fast', '--target-dir', 'custom target', '--', '--exact'])
            command = run.call_args.args[0][0]
            self.assertEqual(command[command.index('--target-dir') + 1], str(Path('custom target').resolve()))
            self.assertEqual(command[-2:], ['--', '--exact'])
            self.assertEqual(run.call_args.kwargs['env']['CARGO_TARGET_DIR'], str(Path('中文 target').resolve()))

    def test_help_from_outside_checkout(self):
        with tempfile.TemporaryDirectory(prefix='中文 caller ') as folder:
            result = subprocess.run([sys.executable, str(dev.ROOT / 'tools/dev.py'), '--help'], cwd=folder, capture_output=True, text=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn('test sdk MODE', result.stdout)

    def test_python_gate_paths_remain_caller_relative(self):
        for mode, option in (('python2', '--output'), ('python2-connected', '--editor-host')):
            with self.subTest(mode=mode), patch.object(dev.cargo_domain, 'validate'), patch.object(dev, 'run_commands', return_value=0) as run:
                self.assertEqual(dev.main(['test', 'sdk', mode, option + '=中文 output']), 0)
                invocation = run.call_args.args[0][0]
                self.assertEqual(invocation[-1], option + '=' + str(Path('中文 output').resolve()))

    def test_empty_arguments_do_not_build(self):
        with patch.object(dev.subprocess, 'call') as run:
            self.assertEqual(dev.main([]), 0)
            run.assert_not_called()

    def test_daily_filter_and_separator(self):
        for mode in ('mesh', 'regression'):
            command, = dev.test_commands('sdk', mode, ['case', '--', '--exact'])
            self.assertEqual(command.count('--'), 1)
            self.assertEqual(command[-5:], ['case', '--', '--exact', '--skip', 'alpha_mesh_stress::'])

    def test_full_stops_on_failure(self):
        calls = []
        def run(command, **kwargs):
            calls.append(command)
            return 19
        self.assertEqual(dev.run_commands(dev.test_commands('sdk', 'full', []), runner=run), 19)
        self.assertEqual(len(calls), 1)

    def test_editor_clears_live_session(self):
        with patch.dict(os.environ, ZIPZI_DEV_SESSION_DIR='live'), patch.object(dev.cargo_domain, 'validate'), patch.object(dev, 'run_commands', return_value=0) as run:
            self.assertEqual(dev.main(['test', 'editor', 'fast', 'timeline::']), 0)
            self.assertNotIn('ZIPZI_DEV_SESSION_DIR', run.call_args.kwargs['env'])
            self.assertEqual(os.environ['ZIPZI_DEV_SESSION_DIR'], 'live')

    def test_real_child_exit_status(self):
        self.assertEqual(dev.run_commands([[sys.executable, '-c', 'raise SystemExit(23)']]), 23)

    def test_lock_excludes_other_process_and_releases(self):
        with tempfile.TemporaryDirectory(prefix='开发 lock ') as folder:
            lock = Path(folder) / 'prepare.lock'
            code = ('import sys; from pathlib import Path; '
                    f'sys.path.insert(0, {str(dev.ROOT / "tools/workspace")!r}); '
                    'from dev_platform import file_lock; '
                    'exec("with file_lock(Path(sys.argv[1]), blocking=False): pass")')
            def attempt():
                return subprocess.run([sys.executable, '-c', code, str(lock)], capture_output=True)
            with file_lock(lock):
                self.assertNotEqual(attempt().returncode, 0)
            self.assertEqual(attempt().returncode, 0)
