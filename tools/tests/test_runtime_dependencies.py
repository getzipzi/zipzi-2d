"""The base Runtime is CPU-only and SDK must not acquire a reverse dependency."""
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]


class RuntimeDependencies(unittest.TestCase):
    def dependencies(self, package, features=None):
        command = [
            'cargo', 'tree', '--locked', '-p', package, '-e', 'normal',
            '--prefix', 'none', '--format', '{p}',
        ]
        if features:
            command += ['--features', features]
        output = subprocess.check_output(command, cwd=ROOT, text=True)
        return {line.split()[0] for line in output.splitlines() if line.strip()}

    def test_base_runtime_and_sdk_dependency_direction(self):
        runtime = self.dependencies('zipzi-runtime')
        self.assertIn('zipzi-sdk', runtime)
        self.assertFalse({name for name in runtime if name.startswith((
            'gpui', 'pyo3', 'wgpu', 'metal', 'zipzi-render', 'zipzi-editor',
        ))}, runtime)
        self.assertNotIn('zipzi-runtime', self.dependencies('zipzi-sdk'))

    def test_gpu_runtime_keeps_ui_and_python_adapters_out(self):
        runtime = self.dependencies('zipzi-runtime', 'gpu-observation')
        self.assertIn('zipzi-sdk-observe', runtime)
        self.assertIn('zipzi-render', runtime)
        self.assertFalse({name for name in runtime if name.startswith((
            'gpui', 'pyo3', 'zipzi-editor',
        ))}, runtime)


if __name__ == '__main__':
    unittest.main()
