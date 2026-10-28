"""SDK test selection controls, shared across native Windows and POSIX."""
import sys
from pathlib import Path
import unittest
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import dev


class TestEntrypointTests(unittest.TestCase):
    def test_daily_gates_preserve_filters_and_harness_arguments(self):
        for mode in ('mesh', 'regression'):
            for arguments in (['coverage', '--no-run'], ['coverage', '--', '--nocapture']):
                command, = dev.test_commands('sdk', mode, arguments)
                self.assertEqual(command.count('--'), 1)
                self.assertEqual(command[-2:], ['--skip', 'alpha_mesh_stress::'])
                self.assertIn('coverage', command)
                if '--no-run' in arguments:
                    self.assertLess(command.index('--no-run'), command.index('--'))
                if mode == 'regression':
                    self.assert_packages(command)

    def assert_packages(self, command):
        names = [command[i+1] for i, arg in enumerate(command) if arg == '-p']
        self.assertCountEqual(names, dev.cargo_domain.domains()['sdk'])

    def test_full_and_stress_keep_matrix(self):
        for mode in ('full', 'stress'):
            commands = dev.test_commands('sdk', mode, [])
            self.assertFalse(any('--skip' in command for command in commands))
            if mode == 'full':
                for command in commands:
                    self.assert_packages(command)

    def test_empty_mode_is_help(self):
        with patch.object(dev.subprocess, 'call') as call:
            self.assertEqual(dev.main(['test', 'sdk']), 0)
            call.assert_not_called()

    def test_failures_stop_and_propagate(self):
        for status in (7, 19):
            with patch.object(dev.cargo_domain, 'validate'), patch.object(dev.subprocess, 'call', return_value=status) as call:
                self.assertEqual(dev.main(['test', 'sdk', 'full']), status)
                self.assertEqual(call.call_count, 1)
