"""Alpha feed publication must authenticate bytes, preserve packages and never roll back."""
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

TOOLS = Path(__file__).resolve().parents[1] / 'packaging'
sys.path.insert(0, str(TOOLS))
SPEC = importlib.util.spec_from_file_location('publish_updates', TOOLS / 'publish_updates.py')
publisher = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(publisher)
from package_updates import private_key, sign_feed


class FakeGitHub:
    def __init__(self, root):
        self.root = root
        self.events = []

    def release(self):
        self.events.append(('read',))
        if not self.root.exists():
            return None
        return dict(prerelease=True, assets=[dict(name=path.name,
            digest='sha256:' + hashlib.sha256(path.read_bytes()).hexdigest())
            for path in self.root.iterdir()])

    def create(self, revision):
        self.events.append(('create', revision))
        self.root.mkdir()

    def download(self, names, destination):
        for name in names:
            shutil.copyfile(self.root / name, destination / name)

    def upload(self, files, replace=False):
        self.events.append(('upload', replace, [path.name for path in files]))
        for path in files:
            destination = self.root / path.name
            if destination.exists() and not replace:
                raise AssertionError('Attempted to overwrite a package')
            shutil.copyfile(path, destination)


class AlphaPublication(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)
        self.key = private_key('07' * 32)
        self.public_key = self.key.public_key().public_bytes_raw().hex()
        self.github = FakeGitHub(self.root / 'github')

    def build(self, version='0.1.0-alpha.2'):
        directory = self.root / version
        directory.mkdir()
        for channel in publisher.CHANNELS:
            package = directory / f'Zipzi.Editor-{version}-{channel}-full.nupkg'
            package.write_bytes((channel + version).encode())
            asset = dict(PackageId='Zipzi.Editor', Type='Full', Version=version,
                         FileName=package.name, Size=package.stat().st_size,
                         SHA256=hashlib.sha256(package.read_bytes()).hexdigest())
            feed = directory / f'releases.{channel}.json'
            feed.write_text(json.dumps({'Assets': [asset]}))
            sign_feed(feed, self.key)
        return directory

    def publish(self, directory):
        return publisher.publish(directory, 'getzipzi/zipzi-2d', 'a' * 40,
                                 'v' + directory.name, self.public_key, self.github)

    def test_packages_are_published_before_exact_signed_feeds(self):
        directory = self.build()
        self.assertTrue(self.publish(directory))
        uploads = [event for event in self.github.events if event[0] == 'upload']
        self.assertFalse(uploads[0][1])
        self.assertTrue(all(name.endswith('.nupkg') for name in uploads[0][2]))
        self.assertTrue(uploads[1][1])
        self.assertTrue(all('.json' in name for name in uploads[1][2]))
        for path in directory.iterdir():
            self.assertEqual(path.read_bytes(), (self.github.root / path.name).read_bytes())

    def test_older_tags_and_out_of_order_runs_never_roll_back_the_channel(self):
        self.publish(self.build('0.1.0-alpha.10'))
        before = {path.name: path.read_bytes() for path in self.github.root.iterdir()}
        self.github.events.clear()
        self.assertFalse(self.publish(self.build('0.1.0-alpha.9')))
        self.assertEqual(self.github.events, [('read',)])
        self.assertEqual(before, {path.name: path.read_bytes() for path in self.github.root.iterdir()})

    def test_retry_reuses_identical_packages_but_rejects_same_name_with_changed_bytes(self):
        directory = self.build()
        self.publish(directory)
        self.github.events.clear()
        self.assertTrue(self.publish(directory))
        self.assertFalse(any(event[0] == 'upload' and not event[1] and event[2]
                             for event in self.github.events))
        package = next(self.github.root.glob('*.nupkg'))
        package.write_bytes(b'tampered package')
        with self.assertRaisesRegex(ValueError, 'immutable update package'):
            self.publish(directory)

    def test_bad_signature_corrupt_payload_missing_platform_and_wrong_tag_prevent_all_writes(self):
        for case in ('signature', 'payload', 'platform', 'version'):
            with self.subTest(case=case):
                directory = self.build('0.1.0-alpha.20')
                if case == 'signature':
                    next(directory.glob('*.sig')).write_text('00' * 64)
                elif case == 'payload':
                    next(directory.glob('*.nupkg')).write_bytes(b'corrupted')
                elif case == 'platform':
                    next(directory.glob('*.json')).unlink()
                else:
                    directory = directory.rename(self.root / '0.1.0-alpha.99')
                with self.assertRaises(ValueError):
                    self.publish(directory)
                self.assertEqual(self.github.events, [])
                shutil.rmtree(directory)

    def test_interrupted_feed_replacement_can_be_completed_without_package_overwrites(self):
        directory = self.build()
        self.publish(directory)
        next(self.github.root.glob('*.sig')).unlink()
        self.assertTrue(self.publish(directory))
        self.assertEqual(len(list(self.github.root.glob('*.sig'))), 2)

    def test_retry_repairs_either_half_of_an_interrupted_feed_signature_pair(self):
        old = self.build('0.1.0-alpha.1')
        new = self.build('0.1.0-alpha.2')
        for replaced in ('json', 'sig'):
            with self.subTest(replaced=replaced):
                if self.github.root.exists():
                    shutil.rmtree(self.github.root)
                self.publish(old)
                current = next(new.glob('*.json' if replaced == 'json' else '*.sig'))
                shutil.copyfile(current, self.github.root / current.name)
                self.assertTrue(self.publish(new))
                for path in new.iterdir():
                    self.assertEqual(path.read_bytes(), (self.github.root / path.name).read_bytes())

    def test_package_upload_failure_leaves_both_previous_feeds_unchanged(self):
        old, new = self.build('0.1.0-alpha.1'), self.build('0.1.0-alpha.2')
        self.publish(old)
        upload = self.github.upload

        def fail(files, replace=False):
            if not replace:
                raise RuntimeError('Interrupted package upload')
            upload(files, replace)

        self.github.upload = fail
        with self.assertRaisesRegex(RuntimeError, 'Interrupted'):
            self.publish(new)
        for path in old.glob('*.json*'):
            self.assertEqual(path.read_bytes(), (self.github.root / path.name).read_bytes())

    def test_newer_uploaded_package_blocks_rollback_even_with_a_missing_feed_signature(self):
        self.publish(self.build('0.1.0-alpha.10'))
        for signature in self.github.root.glob('*.sig'):
            signature.unlink()
        self.assertFalse(self.publish(self.build('0.1.0-alpha.9')))


if __name__ == '__main__':
    unittest.main()
