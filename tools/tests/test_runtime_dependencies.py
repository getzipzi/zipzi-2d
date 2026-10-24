"""The base Runtime is CPU-only and SDK must not acquire a reverse dependency."""
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]


class RuntimeDependencies(unittest.TestCase):
    def dependencies(self, package):
        output = subprocess.check_output([
            'cargo', 'tree', '--locked', '-p', package, '-e', 'normal',
            '--prefix', 'none', '--format', '{p}',
        ], cwd=ROOT, text=True)
        return {line.split()[0] for line in output.splitlines() if line.strip()}

    def test_base_runtime_and_sdk_dependency_direction(self):
        runtime = self.dependencies('zipzi-runtime')
        self.assertIn('zipzi-sdk', runtime)
        self.assertFalse({name for name in runtime if name.startswith((
            'gpui', 'pyo3', 'wgpu', 'metal', 'zipzi-render', 'zipzi-editor',
        ))}, runtime)
        self.assertNotIn('zipzi-runtime', self.dependencies('zipzi-sdk'))


if __name__ == '__main__':
    unittest.main()
