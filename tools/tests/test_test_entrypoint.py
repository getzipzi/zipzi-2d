"""Keep daily exclusions local while the full Rust gate retains every stress case."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest


class TestEntrypointTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / 'tools').mkdir()
        shutil.copy(Path(__file__).resolve().parents[2] / 'tools/test-sdk.sh', self.root / 'tools/test-sdk.sh')
        for name in ['workspace/cargo_domain.py', 'workspace/package-domains.json']:
            (self.root / "tools" / name).parent.mkdir(parents=True, exist_ok=True)
            shutil.copy(Path(__file__).resolve().parents[2] / 'tools' / name, self.root / 'tools' / name)
        binary = self.root / 'bin'
        binary.mkdir()
        for name in ['cargo']:
            path = binary / name
            path.write_text(f'#!{sys.executable}\n' + '''import json,os,pathlib,sys
with open('calls.jsonl','a') as output:
    output.write(json.dumps([pathlib.Path(sys.argv[0]).name,*sys.argv[1:]])+'\\n')
if sys.argv[1:2]==['fmt']:sys.exit(int(os.environ.get('FORMAT_EXIT','0')))
if sys.argv[1:2]==['test']:sys.exit(int(os.environ.get('CARGO_TEST_EXIT','0')))
''')
            path.chmod(0o755)
        self.env = {**os.environ, 'PATH': f'{binary}:{os.environ["PATH"]}'}

    def assert_sdk_packages(self, command):
        expected = json.loads((self.root / 'tools/workspace/package-domains.json').read_text())['sdk']
        actual = [command[i + 1] for i, value in enumerate(command) if value == '-p']
        self.assertCountEqual(actual, expected)
        self.assertNotIn('--all', command)

    def run_gate(self, *arguments, **environment):
        (self.root / 'calls.jsonl').unlink(missing_ok=True)
        result = subprocess.run(['sh', 'tools/test-sdk.sh', *arguments], cwd=self.root,
                                env={**self.env, **environment}, capture_output=True, text=True)
        calls = [json.loads(line) for line in (self.root / 'calls.jsonl').read_text().splitlines()]
        return result, calls

    def test_daily_gates_preserve_filters_and_harness_arguments(self):
        for gate in ['mesh', 'regression']:
            for arguments in [('coverage', '--no-run'), ('coverage', '--', '--nocapture')]:
                with self.subTest(gate=gate, arguments=arguments):
                    result, calls = self.run_gate(gate, *arguments)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(len(calls), 1)
                    command = calls[0]
                    self.assertEqual(command.count('--'), 1)
                    self.assertEqual(command[-2:], ['--skip', 'alpha_mesh_stress::'])
                    self.assertIn('coverage', command)
                    if '--nocapture' in arguments:
                        self.assertIn('--nocapture', command)
                    else:
                        self.assertLess(command.index('--no-run'), command.index('--'))
                    self.assertNotIn('--workspace', command)
                    if gate == 'mesh':
                        self.assertIn('alpha_mesh', command)
                    else:
                        self.assert_sdk_packages(command)

    def test_stress_and_full_include_the_matrix_without_exclusions(self):
        for gate in ['stress', 'full', 'default']:
            with self.subTest(gate=gate):
                result, calls = self.run_gate(*([] if gate == 'default' else [gate]))
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertFalse(any('--skip' in call for call in calls))
                rust = [call for call in calls if call[:2] == ['cargo', 'test']]
                self.assertEqual(len(rust), 1)
                self.assertNotIn('--workspace', rust[0])
                if gate == 'stress':
                    self.assertIn('alpha_mesh', rust[0])
                else:
                    self.assert_sdk_packages(rust[0])
                    self.assert_sdk_packages(calls[0])
                if gate != 'stress':
                    self.assertEqual(len(calls), 2)

    def test_format_failure_propagates(self):
        result, calls = self.run_gate('full', FORMAT_EXIT='7')
        self.assertEqual(result.returncode, 7)
        self.assertEqual(len(calls), 1)

    def test_full_propagates_rust_failure(self):
        result, calls = self.run_gate('full', CARGO_TEST_EXIT='19')
        self.assertEqual(result.returncode, 19)
        self.assertEqual([call[0] for call in calls], ['cargo', 'cargo'])


if __name__ == '__main__':
    unittest.main()
