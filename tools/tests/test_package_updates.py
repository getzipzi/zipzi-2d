"""Update authenticity must cover the actual package, not just a transport checksum."""
import importlib.util
import json
import hashlib
from pathlib import Path
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch
import zipfile

TOOLS = Path(__file__).resolve().parents[1] / 'packaging'
sys.path.insert(0, str(TOOLS))
SPEC = importlib.util.spec_from_file_location('package_updates', TOOLS / 'package_updates.py')
updates = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(updates)


class UpdatePackages(unittest.TestCase):
    def test_macos_packaging_uses_editor_entrypoint_and_exports_signed_feed(self):
        import package_macos

        key = updates.private_key('07' * 32)
        public_key = key.public_key().public_bytes_raw().hex()
        update_url = 'https://example.com/updates'
        info = updates.metadata()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            app = root / 'Zipzi Editor.app'
            binary = app / 'Contents/MacOS/zipzi-editor'
            binary.parent.mkdir(parents=True)
            binary.write_bytes((public_key + update_url).encode())
            output = root / 'release'
            args = SimpleNamespace(app=app, binary=None, output=output, vpk='vpk',
                                   revision='a' * 40, allow_unsigned=False)

            def run(command, **kwargs):
                if command[:2] == ['vpk', 'pack']:
                    self.assertNotIn('ZIPZI_UPDATE_SIGNING_KEY', kwargs['env'])
                    # Velopack defaults to the package ID when no entrypoint is given.
                    name = (command[command.index('--mainExe') + 1] if '--mainExe' in command
                            else command[command.index('--packId') + 1])
                    content = Path(command[command.index('--packDir') + 1])
                    if not (content / 'Contents/MacOS' / name).is_file():
                        raise FileNotFoundError(f'Main executable not found: {name}')
                    self.assertIn('--noInst', command)  # The installer is our DMG.
                    self.assertIn('--signDisableDeep', command)
                    self.assertIn('--yes', command)
                    self.assertIn('--skip-updates', command)
                    target = Path(command[command.index('--outputDir') + 1])
                    channel = command[command.index('--channel') + 1]
                    target.mkdir()
                    package = target / 'Zipzi.Editor-test-full.nupkg'
                    package.write_bytes(b'packaged app')
                    asset = dict(PackageId=updates.PACKAGE_ID, FileName=package.name,
                                 Size=package.stat().st_size,
                                 SHA256=hashlib.sha256(package.read_bytes()).hexdigest())
                    (target / f'releases.{channel}.json').write_text(json.dumps({'Assets': [asset]}))
                    with zipfile.ZipFile(target / 'Zipzi.Editor-Portable.zip', 'w') as archive:
                        archive.write(binary, 'Zipzi Editor.app/Contents/MacOS/zipzi-editor')
                elif command[:2] == ['ditto', '-xk']:
                    with zipfile.ZipFile(command[2]) as archive:
                        archive.extractall(command[3])
                else:
                    self.fail(f'Unexpected command: {command}')

            def dmg(packaged_app, destination, trusted):
                self.assertTrue(trusted)
                self.assertEqual((packaged_app / 'Contents/MacOS/zipzi-editor').read_bytes(),
                                 binary.read_bytes())
                destination.write_bytes(b'disk image')

            env = dict(ZIPZI_UPDATE_SIGNING_KEY='07' * 32, ZIPZI_UPDATE_PUBLIC_KEY=public_key,
                       ZIPZI_UPDATE_URL=update_url, VPK_SIGN_APP_IDENTITY='test identity',
                       VPK_NOTARY_PROFILE='test profile')
            with (patch.dict(updates.os.environ, env), patch.object(updates.sys, 'platform', 'darwin'),
                  patch.object(updates.subprocess, 'check_output', return_value=f'Velopack CLI {updates.VELOPACK_VERSION},'),
                  patch.object(updates.subprocess, 'run', side_effect=run),
                  patch.object(package_macos, 'package', side_effect=dmg)):
                updates.package(args)
            feeds = list(output.glob('releases.*.json'))
            self.assertEqual(len(feeds), 1)
            signature = bytes.fromhex(feeds[0].with_suffix('.json.sig').read_text().strip())
            key.public_key().verify(signature, feeds[0].read_bytes())
            self.assertEqual((output / 'Zipzi.Editor-test-full.nupkg').read_bytes(), b'packaged app')
            self.assertTrue((output / f'Zipzi-Editor-{info["tag"]}-macos-arm64.dmg').is_file())

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
