"""The connection installation must be a self-contained, deterministic wheel."""
import base64
import csv
import hashlib
import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("client_builder", ROOT / "tools/python/build_zipzi_client.py")
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


class ClientWheel(unittest.TestCase):
    def test_pure_wheel_records_and_reproducible_contents(self):
        with tempfile.TemporaryDirectory() as temp:
            wheel = builder.build(Path(temp))
            first = wheel.read_bytes()
            self.assertEqual(first, builder.build(Path(temp)).read_bytes())
            with zipfile.ZipFile(wheel) as archive:
                names = archive.namelist()
                self.assertIn("zipzi/_connection.py", names)
                self.assertIn("zipzi/_endpoint.py", names)
                self.assertFalse(any(n.endswith((".so", ".dylib", ".pyd")) for n in names))
                record = next(n for n in names if n.endswith("/RECORD"))
                rows = list(csv.reader(io.StringIO(archive.read(record).decode())))
                self.assertEqual({r[0] for r in rows}, set(names))
                for name, digest, length in rows:
                    if name == record:
                        self.assertEqual((digest, length), ("", ""))
                        continue
                    data = archive.read(name)
                    expected = base64.urlsafe_b64encode(hashlib.sha256(data).digest()).rstrip(b"=").decode()
                    self.assertEqual(digest, "sha256=" + expected)
                    self.assertEqual(int(length), len(data))


if __name__ == "__main__":
    unittest.main()
