"""Update authenticity must cover the actual package, not just a transport checksum."""
import importlib.util
import json
import hashlib
from pathlib import Path
import sys
import tempfile
import unittest

TOOLS = Path(__file__).resolve().parents[1] / 'packaging'
sys.path.insert(0, str(TOOLS))
SPEC = importlib.util.spec_from_file_location('package_updates', TOOLS / 'package_updates.py')
updates = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(updates)


class UpdatePackages(unittest.TestCase):
    def test_signature_covers_exact_feed_and_verified_package(self):
        key = updates.private_key('07' * 32)
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package = root / 'Zipzi.Editor-1.0.1-full.nupkg'
            package.write_bytes(b'package payload')
            asset = dict(PackageId=updates.PACKAGE_ID, FileName=package.name,
                         Size=package.stat().st_size, SHA256=hashlib.sha256(package.read_bytes()).hexdigest())
            feed = root / 'releases.windows-x86_64-alpha.json'
            feed.write_text(json.dumps({'Assets': [asset]}))
            updates.sign_feed(feed, key)
            signature = bytes.fromhex(feed.with_suffix('.json.sig').read_text().strip())
            key.public_key().verify(signature, feed.read_bytes())
            package.write_bytes(b'corrupted payload')
            with self.assertRaisesRegex(ValueError, 'failed verification'):
                updates.sign_feed(feed, key)

    def test_traversal_and_foreign_app_are_rejected_even_before_signing(self):
        key = updates.private_key('07' * 32)
        with tempfile.TemporaryDirectory() as directory:
            feed = Path(directory) / 'releases.test.json'
            for identity, name in [('Other.App', 'payload.nupkg'), (updates.PACKAGE_ID, '../payload.nupkg')]:
                feed.write_text(json.dumps({'Assets': [dict(PackageId=identity, FileName=name)]}))
                with self.assertRaisesRegex(ValueError, 'identity or filename'):
                    updates.sign_feed(feed, key)


if __name__ == '__main__':
    unittest.main()
