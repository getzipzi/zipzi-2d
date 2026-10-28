"""Editor test routing, environment isolation and ordered prerequisite failures."""
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import dev


class TestRunnerTests(unittest.TestCase):
    def run_gate(self, *args, status=0):
        with patch.object(dev.cargo_domain, 'validate'), patch.object(dev.subprocess, 'call', return_value=status) as call:
            result = dev.main(['test', 'editor', *args])
        return result, call

    def test_fast_isolates_session_and_preserves_filter(self):
        with patch.dict(os.environ, ZIPZI_DEV_SESSION_DIR='live'):
            result, call = self.run_gate('fast', 'timeline::', '--', '--exact')
        self.assertEqual(result, 0)
        self.assertNotIn('ZIPZI_DEV_SESSION_DIR', call.call_args.kwargs['env'])
        self.assertEqual(call.call_args.args[0][-3:], ['timeline::', '--', '--exact'])

    def test_no_mode_is_help(self):
        result, call = self.run_gate()
        self.assertEqual(result, 0)
        call.assert_not_called()

    def test_full_runs_format_tools_and_complete_domain(self):
        with patch.dict(os.environ, ZIPZI_CLIENT_PYTHON=sys.executable):
            result, call = self.run_gate('full')
        self.assertEqual(result, 0)
        commands = [entry.args[0] for entry in call.call_args_list]
        self.assertEqual(len(commands), 3)
        self.assertEqual(commands[0][:2], ['cargo', 'fmt'])
        self.assertEqual(Path(commands[1][1]).name, 'test_tools.py')
        names = [commands[2][i+1] for i, arg in enumerate(commands[2]) if arg == '-p']
        self.assertCountEqual(names, dev.cargo_domain.domains()['editor'])
        self.assertNotIn('--lib', commands[2])

    def test_tooling_failure_prevents_install_and_rust(self):
        with patch.object(dev.cargo_domain, 'validate'), patch.object(dev.subprocess, 'call', side_effect=[0, 9]) as call, patch.object(dev.subprocess, 'check_output') as install:
            self.assertEqual(dev.main(['test', 'editor', 'full']), 9)
            self.assertEqual(call.call_count, 2)
            install.assert_not_called()

    def test_acceptance_prepares_client_and_retains_targets(self):
        with patch.dict(os.environ, ZIPZI_CLIENT_PYTHON=''), patch.object(dev.subprocess, 'check_output', return_value=sys.executable) as install:
            result, call = self.run_gate('acceptance', status=17)
        self.assertEqual(result, 17)
        self.assertIn('--install-only', install.call_args.args[0])
        self.assertEqual(call.call_args.kwargs['env']['ZIPZI_CLIENT_PYTHON'], sys.executable)
        for target in ('acceptance', 'native_render', 'headless_screenshot', 'animation_acceptance', 'timeline_acceptance', 'runtime_interaction_acceptance', 'runtime_ipc_acceptance', 'runtime_handoff_acceptance'):
            self.assertIn(target, call.call_args.args[0])

    def test_unknown_mode_starts_no_work(self):
        with patch.object(dev.subprocess, 'call') as call:
            with self.assertRaises(ValueError):
                dev.main(['test', 'editor', 'fasst'])
            call.assert_not_called()
