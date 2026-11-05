"""Alpha release configuration must fail before builds and never export secrets."""
import importlib.util
from pathlib import Path
import plistlib
import sys
import tempfile
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1] / 'packaging'
sys.path.insert(0, str(TOOLS))
SPEC = importlib.util.spec_from_file_location('ci_updates', TOOLS / 'ci_updates.py')
updates = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(updates)


class AlphaConfiguration(unittest.TestCase):
    def enabled(self):
        public_key = updates.private_key('07' * 32).public_key().public_bytes_raw().hex()
        return dict({name: 'fixture credential' for name in updates.SIGNING_INPUTS},
                    ZIPZI_DESKTOP_UPDATES='true', GITHUB_REPOSITORY='getzipzi/zipzi-2d',
                    ZIPZI_UPDATE_PUBLIC_KEY=public_key, ZIPZI_UPDATE_SIGNING_KEY='07' * 32)

    def test_unconfigured_build_retains_legacy_downloads(self):
        off = updates.configure({})
        self.assertEqual(off['updates_enabled'], 'false')
        self.assertEqual(off['update_url'], '')
        self.assertEqual(off['update_public_key'], '')
        self.assertEqual(off, updates.configure(dict(self.enabled(), ZIPZI_DESKTOP_UPDATES='false')))

    def test_enabled_build_has_fixed_github_source_and_exports_only_public_settings(self):
        env = self.enabled()
        result = updates.configure(env)
        self.assertEqual(result['update_url'],
                         'https://github.com/getzipzi/zipzi-2d/releases/download/desktop-alpha-updates')
        self.assertEqual(result['update_public_key'], env['ZIPZI_UPDATE_PUBLIC_KEY'])
        for value in (env['ZIPZI_UPDATE_SIGNING_KEY'], 'fixture credential'):
            self.assertNotIn(value, str(result))
        self.assertNotEqual(result['build_configuration'], updates.configure({})['build_configuration'])
        changed = dict(env, GITHUB_REPOSITORY='another/repository')
        self.assertNotEqual(result['build_configuration'], updates.configure(changed)['build_configuration'])

    def test_every_signing_input_and_matching_key_is_required(self):
        for name in (*updates.SIGNING_INPUTS, 'ZIPZI_UPDATE_SIGNING_KEY', 'ZIPZI_UPDATE_PUBLIC_KEY'):
            env = self.enabled()
            del env[name]
            with self.subTest(name=name), self.assertRaises(ValueError):
                updates.configure(env)
        with self.assertRaisesRegex(ValueError, 'does not match'):
            updates.configure(dict(self.enabled(), ZIPZI_UPDATE_SIGNING_KEY='08' * 32))
        for value in ('TRUE', 'yes', '1', 'false\n'):
            with self.subTest(value=value), self.assertRaises(ValueError):
                updates.configure({'ZIPZI_DESKTOP_UPDATES': value})

    def test_developer_id_previews_keep_sandbox_entitlements_and_hardened_runtime(self):
        import build_project_preview

        for identity in ('-', 'Developer ID Application: Fixture'):
            with self.subTest(identity=identity), tempfile.TemporaryDirectory() as directory:
                app = Path(directory) / 'Zipzi Editor.app'
                (app / 'Contents').mkdir(parents=True)
                (app / 'Contents/Info.plist').write_bytes(plistlib.dumps(dict(
                    CFBundleIdentifier='dev.zipzi.editor', CFBundleVersion='1',
                    CFBundleShortVersionString='0.1.0')))
                with patch.object(build_project_preview.subprocess, 'run') as run:
                    build_project_preview.build_macos(app, Path(directory) / 'fixture.a', identity)
                signatures = [call.args[0] for call in run.call_args_list
                              if call.args[0][0] == 'codesign']
                self.assertEqual(len(signatures), 2)
                for command in signatures:
                    entitlements = Path(command[command.index('--entitlements') + 1])
                    self.assertEqual(plistlib.loads(entitlements.read_bytes()),
                                     {'com.apple.security.app-sandbox': True})
                    self.assertEqual('--timestamp' in command, identity != '-')
                    self.assertEqual('--options' in command, identity != '-')


if __name__ == '__main__':
    unittest.main()
