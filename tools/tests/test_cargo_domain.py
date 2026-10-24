"""Domain boundaries must fail closed when the workspace grows or flags override scope."""
import json
import unittest
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tools/workspace"))
from unittest.mock import patch
import cargo_domain as gate


class PackageDomains(unittest.TestCase):
    def test_real_workspace_is_exhaustively_partitioned(self):
        gate.validate(gate.domains())

    def test_missing_and_stale_members_are_rejected(self):
        groups = {'sdk': ['core'], 'editor': ['editor']}
        for names in [['core', 'editor', 'new'], ['core']]:
            metadata = {'workspace_members': names, 'packages': [{'id': n, 'name': n} for n in names]}
            with self.subTest(names=names), patch.object(gate.subprocess, 'check_output', return_value=json.dumps(metadata)):
                with self.assertRaises(ValueError):
                    gate.validate(groups)

    def test_duplicate_or_empty_domains_are_rejected(self):
        for groups in [{'sdk': ['core'], 'editor': ['core']}, {'sdk': ['core'], 'editor': []}]:
            with patch.object(gate.Path, 'read_text', return_value=json.dumps(groups)):
                with self.assertRaises(ValueError):
                    gate.domains()

    def test_scope_overrides_rejected_but_harness_flags_preserved(self):
        groups = {'sdk': ['core'], 'editor': ['editor']}
        for flag in ['--workspace', '--all', '-p', '-pother', '--package=other', '--exclude=core', '--manifest-path=other']:
            with self.subTest(flag=flag), self.assertRaises(ValueError):
                gate.command(groups, 'sdk', 'test', [flag])
        self.assertEqual(gate.command(groups, 'sdk', 'test', ['--locked', '--', '--exact']),
                         ['cargo', 'test', '-p', 'core', '--locked', '--', '--exact'])
        self.assertEqual(gate.command(groups, 'editor', 'fmt', ['--', '--check']),
                         ['cargo', 'fmt', '-p', 'editor', '--', '--check'])

    def test_cargo_failure_is_propagated_from_repository_root(self):
        with patch.object(gate.subprocess, 'call', return_value=19) as call:
            self.assertEqual(gate.main(['sdk', 'test', '--locked']), 19)
        self.assertEqual(call.call_args.kwargs['cwd'], gate.ROOT)


if __name__ == '__main__':
    unittest.main()
