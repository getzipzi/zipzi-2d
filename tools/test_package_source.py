"""Source releases must retain assets and reject leaked private/generated content."""
from pathlib import Path
import subprocess
import tempfile
import unittest
from package_source import source_files


class SourcePackage(unittest.TestCase):
    def test_assets_included_and_ignored_models_excluded(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            subprocess.run(['git', 'init', '-q', str(root)], check=True)
            (root/'.gitignore').write_text('models/local/\ntarget/\nthird_party/CubismSdkForNative*/\n')
            for name in ['assets/editor/locales/en.toml', 'crates/kasane-editor/src/lib.rs',
                         'models/local/private.cmo3', 'target/build',
                         'third_party/CubismSdkForNative-5-r.5/Core/private.a']:
                path = root/name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text('fixture')
            self.assertEqual(set(source_files(root)), {'.gitignore', 'assets/editor/locales/en.toml', 'crates/kasane-editor/src/lib.rs'})
            subprocess.run(['git', 'add', '-f', 'models/local/private.cmo3'], cwd=root, check=True)
            with self.assertRaisesRegex(ValueError, 'Private/generated'):
                source_files(root)
            subprocess.run(['git', 'rm', '--cached', '-f', 'models/local/private.cmo3'], cwd=root,
                           check=True, capture_output=True)
            subprocess.run(['git', 'add', '-f', 'third_party/CubismSdkForNative-5-r.5/Core/private.a'],
                           cwd=root, check=True)
            with self.assertRaisesRegex(ValueError, 'Private/generated'):
                source_files(root)

    def test_external_symlink_is_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)/'repo'
            root.mkdir()
            subprocess.run(['git', 'init', '-q', str(root)], check=True)
            outside = Path(tmp)/'secret'
            outside.write_text('private')
            (root/'escape').symlink_to(outside)
            with self.assertRaisesRegex(ValueError, 'escapes'):
                source_files(root)


if __name__ == '__main__':
    unittest.main()
