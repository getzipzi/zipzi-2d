"""Only acceptance-tested distributions from the release commit can be published."""
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

TOOLS = Path(__file__).resolve().parents[1] / 'packaging'
sys.path.insert(0, str(TOOLS))
spec = importlib.util.spec_from_file_location('collect_python', TOOLS / 'collect_python.py')
collect_python = importlib.util.module_from_spec(spec)
spec.loader.exec_module(collect_python)


class PythonReleaseCollection(unittest.TestCase):
    def fixture(self, root):
        (root / 'Cargo.toml').write_text('[workspace.package]\nversion="0.1.0-alpha.2"\n')
        builds = root / 'builds'
        for platform in ('macos', 'windows'):
            build = builds / platform
            (build / 'wheels').mkdir(parents=True)
            (build / 'sdists').mkdir()
            suffix = 'macosx_11_0_arm64' if platform == 'macos' else 'win_amd64'
            names = [f'wheels/zipzi_native-0.1.0a2-cp314-cp314-{suffix}.whl']
            if platform == 'macos':
                names += ['wheels/zipzi-0.1.0a2-py3-none-any.whl',
                          'sdists/zipzi-0.1.0a2.tar.gz', 'sdists/zipzi_native-0.1.0a2.tar.gz']
            artifacts = []
            for name in names:
                path = build / name
                path.write_bytes(name.encode())
                artifacts.append({'path': str(path), 'sha256': hashlib.sha256(path.read_bytes()).hexdigest()})
            (build / 'results.json').write_text(json.dumps({'status': 'passed', 'base_commit': 'abc', 'artifacts': artifacts}))
        return builds

    def test_collects_only_tested_distributions(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            builds = self.fixture(root)
            collect_python.collect(builds, root / 'dist', 'abc', root)
            self.assertEqual(len(list((root / 'dist').iterdir())), 5)

    def test_rejects_wrong_commit_and_modified_artifact(self):
        for wrong_commit in (True, False):
            with self.subTest(wrong_commit=wrong_commit), tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                builds = self.fixture(root)
                if not wrong_commit:
                    next((builds / 'windows/wheels').iterdir()).write_bytes(b'tampered')
                with self.assertRaises(ValueError):
                    collect_python.collect(builds, root / 'dist', 'wrong' if wrong_commit else 'abc', root)


if __name__ == '__main__':
    unittest.main()
