"""Release identities, dependency order and bundle metadata must agree."""
import importlib.util
import os
from pathlib import Path
import plistlib
import tempfile
import unittest
from unittest.mock import patch

SPEC = importlib.util.spec_from_file_location('zipzi_release', Path(__file__).resolve().parents[1] / 'packaging/release.py')
release = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(release)


class ReleaseVersions(unittest.TestCase):
    def test_python_prerelease_translation_and_invalid_versions(self):
        for rust, python in [('0.1.0-alpha.1', '0.1.0a1'), ('0.1.0-beta.2', '0.1.0b2'),
                             ('0.1.0-rc.3', '0.1.0rc3'), ('0.1.0', '0.1.0')]:
            self.assertEqual(release.versions(rust)['python_version'], python)
        for invalid in ['v0.1.0', '0.1', '0.1.0-alpha.0', '01.1.0', '0.1.0a1', '0.1.0\n']:
            with self.subTest(version=invalid), self.assertRaises(ValueError):
                release.versions(invalid)

    def fixture(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        root = Path(folder.name)
        (root / 'Cargo.toml').write_text('''[workspace]
members = ["crates/zipzi-core", "crates/zipzi-sdk"]
[workspace.package]
version = "0.1.0-alpha.1"
[workspace.dependencies]
zipzi-core = { version = "=0.1.0-alpha.1", path = "crates/zipzi-core" }
''')
        for name in ['zipzi-core', 'zipzi-sdk']:
            path = root / 'crates' / name
            path.mkdir(parents=True)
            (path / 'Cargo.toml').write_text(f'[package]\nname = "{name}"\nversion.workspace = true\n')
        with (root / 'crates/zipzi-sdk/Cargo.toml').open('a') as manifest:
            manifest.write('[dependencies]\nzipzi-core = { version = "=0.1.0-alpha.1", path = "../zipzi-core" }\n')
        client = root / 'crates/zipzi-python2/client'
        client.mkdir(parents=True)
        (client / 'pyproject.toml').write_text('[project]\nname = "zipzi"\nversion = "0.1.0a1"\n')
        (client.parent / 'pyproject.toml').write_text('[project]\nname = "zipzi-native"\ndependencies = ["zipzi==0.1.0a1"]\n')
        (root / 'Cargo.lock').write_text('version = 4\n' + ''.join(
            f'[[package]]\nname = "{name}"\nversion = "0.1.0-alpha.1"\n' for name in ['zipzi-core', 'zipzi-sdk'])
            + '[[package]]\nname = "external"\nversion = "0.1.0"\nsource = "registry+https://example.test"\n')
        return root

    def test_version_change_updates_all_owned_metadata_and_leaves_external_lock_entries(self):
        root = self.fixture()
        release.set_version('0.2.0-rc.2', root)
        self.assertEqual(release.check(root)['python_version'], '0.2.0rc2')
        self.assertIn('version = "0.1.0"\nsource =', (root / 'Cargo.lock').read_text())
        manifests = [root/'Cargo.toml', root/'crates/zipzi-sdk/Cargo.toml', root/'Cargo.lock']
        before = [path.read_bytes() for path in manifests]
        release.set_version('0.2.0-rc.2', root)
        self.assertEqual([path.read_bytes() for path in manifests], before)

    def test_check_rejects_tag_version_drift_and_unversioned_path_dependency(self):
        root = self.fixture()
        with self.assertRaisesRegex(ValueError, 'Tag'):
            release.check(root, tag='v0.1.0-alpha.2')
        path = root / 'crates/zipzi-sdk/Cargo.toml'
        path.write_text(path.read_text().replace('version = "=0.1.0-alpha.1", ', ''))
        with self.assertRaisesRegex(ValueError, 'exact release version'):
            release.check(root)

    def test_bundle_uses_numeric_apple_versions_and_preserves_full_alpha_identity(self):
        root = self.fixture()
        with patch.dict(os.environ, ZIPZI_BUILD_NUMBER='12.2', GITHUB_SHA='a' * 40):
            release.app_plist(root / 'Info.plist', root)
        info = plistlib.loads((root / 'Info.plist').read_bytes())
        self.assertEqual(info['CFBundleShortVersionString'], '0.1.0')
        self.assertEqual(info['CFBundleVersion'], '12.2')
        self.assertEqual(info['ZipziReleaseVersion'], '0.1.0-alpha.1')
        self.assertEqual(info['ZipziSourceRevision'], 'a' * 40)
        with patch.dict(os.environ, ZIPZI_BUILD_NUMBER='0.1.0-alpha.1'):
            with self.assertRaisesRegex(ValueError, 'numeric build'):
                release.app_plist(root / 'Info.plist', root)

    def test_public_crates_are_ordered_by_dependencies(self):
        self.assertEqual(release.cargo_order(self.fixture()), ['zipzi-core', 'zipzi-sdk'])


if __name__ == '__main__':
    unittest.main()
