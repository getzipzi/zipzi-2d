"""Native sdist keeps SDK dependencies pinned and excludes desktop-only patches."""
import importlib.util
from pathlib import Path
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]

class Python2Distribution(unittest.TestCase):
    def backend(self):
        spec = importlib.util.spec_from_file_location('zipzi_build_backend', ROOT/'crates/zipzi-python2/build_backend.py')
        module = importlib.util.module_from_spec(spec)
        with patch.dict(sys.modules, maturin=types.ModuleType('maturin')):
            spec.loader.exec_module(module)
        return module

    def test_source_manifest_only_removes_reviewed_desktop_patches(self):
        backend = self.backend()
        original = (ROOT/'Cargo.toml').read_text()
        changed = backend.normalize_manifest(original)
        self.assertNotIn('[patch.', changed)
        self.assertIn('zipzi-runtime', changed)
        self.assertIn('[workspace.dependencies]', changed)
        self.assertEqual(backend.normalize_manifest(changed), changed)
        with self.assertRaises(ValueError):
            backend.normalize_manifest('[patch.crates-io]\nserde = { path = "vendor/serde" }\n')

    def test_pinned_versions_include_source_identity_and_ignore_checksums(self):
        with tempfile.TemporaryDirectory() as folder:
            lock = Path(folder)/'Cargo.lock'
            lock.write_text('version = 4\n[[package]]\nname="test"\nversion="1.2.3"\nsource="registry+https://example.test"\nchecksum="fixture"\n')
            self.assertEqual(self.backend().locked_versions(lock), {('test','1.2.3','registry+https://example.test')})

if __name__=='__main__':unittest.main()
