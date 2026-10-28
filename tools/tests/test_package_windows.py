"""Windows distribution must contain the intended binary and notices."""
import importlib.util
from pathlib import Path
import struct
import sys
import tempfile
import unittest
import zipfile

TOOLS = Path(__file__).resolve().parents[1] / 'packaging'
sys.path.insert(0, str(TOOLS))
spec = importlib.util.spec_from_file_location('package_windows', TOOLS / 'package_windows.py')
package_windows = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package_windows)


def executable(machine=0x8664):
    data = bytearray(128)
    data[:2] = b'MZ'
    struct.pack_into('<I', data, 60, 64)
    data[64:68] = b'PE\0\0'
    struct.pack_into('<H', data, 68, machine)
    return bytes(data)


class WindowsPackage(unittest.TestCase):
    def test_rejects_invalid_or_wrong_architecture_binary(self):
        for data in (b'', b'MZ', executable(0xaa64)):
            with self.subTest(data=data), self.assertRaises(ValueError):
                package_windows.check_executable(data)

    def test_archive_contains_binary_notices_and_commit(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'Cargo.toml').write_text('[workspace.package]\nversion="0.1.0-alpha.2"\n')
            for name in package_windows.NOTICES:
                (root / name).write_text(name)
            (root / 'vendor/library').mkdir(parents=True)
            (root / 'vendor/README.md').write_text('vendor')
            (root / 'vendor/library/LICENSE').write_text('license')
            binary = root / 'editor.exe'
            binary.write_bytes(executable())
            output = root / 'out.zip'
            package_windows.package(binary, output, 'a' * 40, root)
            with zipfile.ZipFile(output) as archive:
                self.assertEqual(archive.read('Zipzi Editor/zipzi-editor.exe'), executable())
                self.assertEqual(archive.read('Zipzi Editor/vendor-notices/library/LICENSE'), b'license')
                self.assertIn(b'0.1.0-alpha.2', archive.read('Zipzi Editor/BUILD-INFO.txt'))
                self.assertIn(b'a' * 40, archive.read('Zipzi Editor/BUILD-INFO.txt'))


if __name__ == '__main__':
    unittest.main()
