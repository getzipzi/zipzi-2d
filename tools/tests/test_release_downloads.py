"""Public release downloads must be unambiguous and keep correct checksums."""
import hashlib
import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest

TOOLS = Path(__file__).resolve().parents[1] / 'packaging'
sys.path.insert(0, str(TOOLS))
SPEC = importlib.util.spec_from_file_location('release_downloads', TOOLS / 'release_downloads.py')
downloads = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(downloads)
TAG = 'v0.1.0-alpha.2'


class ReleaseDownloads(unittest.TestCase):
    def test_only_desktop_downloads_and_complete_source_are_published(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            source = root / 'input'
            source.mkdir()
            expected = downloads.download_names(TAG)
            for name in (*expected, f'Zipzi-Editor-{TAG}-macos-arm64.zip',
                         'zipzi_native-0.1.0a2-cp314-cp314-win_amd64.whl', 'SHA256SUMS'):
                (source / name).write_bytes(name.encode())
            output = root / 'public'
            downloads.prepare_downloads(source, output, TAG)
            self.assertEqual({p.name for p in output.iterdir()}, {*expected, 'SHA256SUMS'})
            for line in (output / 'SHA256SUMS').read_text().splitlines():
                digest, name = line.split('  ', 1)
                self.assertEqual((output / name).read_bytes(), (source / name).read_bytes())
                self.assertEqual(digest, hashlib.sha256((output / name).read_bytes()).hexdigest())

    def test_missing_downloads_fail_before_creating_partial_output(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            with self.assertRaises(ValueError):
                downloads.prepare_downloads(root, root / 'public', TAG)
            self.assertFalse((root / 'public').exists())

    def test_installation_links_follow_version_and_automatic_changelog_is_preserved(self):
        changelog = "## What's Changed\nExisting generated notes\n"
        body = downloads.prepend_notes(changelog, 'getzipzi/zipzi-2d', TAG)
        for name in downloads.download_names(TAG):
            self.assertIn('/releases/download/' + TAG + '/' + name, body)
        self.assertIn('python -m pip install zipzi==0.1.0a2', body)
        self.assertIn(changelog.strip(), body)
        self.assertEqual(downloads.prepend_notes(body, 'getzipzi/zipzi-2d', TAG), body)
        updated = downloads.prepend_notes(body, 'getzipzi/zipzi-2d', 'v0.1.0-alpha.3')
        self.assertNotIn(TAG, updated)
        self.assertIn('zipzi==0.1.0a3', updated)
        self.assertIn(changelog.strip(), updated)


if __name__ == '__main__':
    unittest.main()
