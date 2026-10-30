"""The base Runtime is CPU-only and SDK must not acquire a reverse dependency."""
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]


class RuntimeDependencies(unittest.TestCase):
    def dependencies(self, package, features=None, *, default_features=True, target=None):
        command = [
            'cargo', 'tree', '--locked', '-p', package, '-e', 'normal',
            '--prefix', 'none', '--format', '{p}',
        ]
        if features:
            command += ['--features', features]
        if not default_features:
            command += ['--no-default-features']
        if target:
            command += ['--target', target]
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
        self.assertIn('zipzi-observe', runtime)
        self.assertIn('zipzi-render', runtime)
        self.assertFalse({name for name in runtime if name.startswith((
            'gpui', 'pyo3', 'zipzi-editor',
        ))}, runtime)

    def test_cpu_observation_planning_has_no_gpu_dependencies(self):
        for target in ['aarch64-apple-darwin', 'x86_64-pc-windows-msvc', 'x86_64-unknown-linux-gnu']:
            with self.subTest(target=target):
                observe = self.dependencies('zipzi-observe', default_features=False, target=target)
                self.assertIn('zipzi-contract', observe)
                self.assertNotIn('zipzi-observe-backend', observe)
                self.assertFalse({name for name in observe if name.startswith((
                    'gpui', 'pyo3', 'wgpu', 'metal', 'zipzi-render', 'zipzi-editor',
                ))}, observe)

    def test_default_metal_hosts_do_not_depend_on_wgpu(self):
        for package, features in [('zipzi-observe', None), ('zipzi-runtime', 'gpu-observation'),
                                  ('zipzi-editor', None), ('zipzi-python2', None)]:
            with self.subTest(package=package):
                deps = self.dependencies(package, features, target='aarch64-apple-darwin')
                self.assertIn('zipzi-render-metal', deps)
                self.assertNotIn('zipzi-render-wgpu', deps)
                self.assertFalse({name for name in deps if name.startswith('wgpu')}, deps)

    def test_explicit_macos_wgpu_and_other_platform_gpu_builds_enable_wgpu(self):
        for package in ['zipzi-observe', 'zipzi-runtime', 'zipzi-editor', 'zipzi-python2']:
            with self.subTest(package=package, target='aarch64-apple-darwin'):
                deps = self.dependencies(package, 'wgpu-backend', target='aarch64-apple-darwin')
                self.assertIn('zipzi-render-wgpu', deps)
                self.assertIn('wgpu', deps)
        for target in ['x86_64-pc-windows-msvc', 'x86_64-unknown-linux-gnu']:
            for features in [None, 'wgpu-backend']:
                with self.subTest(target=target, features=features):
                    deps = self.dependencies('zipzi-observe', features, target=target)
                    self.assertIn('zipzi-render-wgpu', deps)
                    self.assertIn('wgpu', deps)
                    self.assertNotIn('zipzi-render-metal', deps)


if __name__ == '__main__':
    unittest.main()
