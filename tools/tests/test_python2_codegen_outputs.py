"""Ownership, recovery and static-report controls for generated Python outputs."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


def load(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


outputs = load('codegen_outputs', 'crates/zipzi-python2/tools/codegen/outputs.py')
stats = load('codegen_stats', 'tools/python/python2_codegen_stats.py')
HEADER = outputs.HEADERS[0]
A = 'client/python/zipzi/_generated/a.py'
B = 'client/python/zipzi/_generated/domain/b.py'


class OutputManifest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.fixed = frozenset({A})
        outputs.write_outputs({A: HEADER + 'a\n'}, False, self.root, self.fixed)

    def write(self, data, check=False):
        return outputs.write_outputs(data, check, self.root, self.fixed)

    def test_nested_outputs_and_safe_removal(self):
        self.write({A: HEADER + 'a\n', B: HEADER + 'b\n'})
        self.assertTrue((self.root / B).is_file())
        before = (self.root / outputs.MANIFEST).read_bytes()
        self.assertEqual(set(self.write({A: HEADER + 'a\n'}, True)), {B, outputs.MANIFEST})
        self.assertEqual((self.root / outputs.MANIFEST).read_bytes(), before)
        self.assertTrue((self.root / B).exists())
        self.write({A: HEADER + 'a\n'})
        self.assertFalse((self.root / B).exists())
        self.assertEqual(self.write({A: HEADER + 'a\n'}, True), [])

    def test_modified_stale_file_and_unknown_files_are_preserved(self):
        self.write({A: HEADER + 'a\n', B: HEADER + 'b\n'})
        (self.root / B).write_text(HEADER + 'changed\n')
        before = (self.root / outputs.MANIFEST).read_bytes()
        with self.assertRaisesRegex(ValueError, 'modified'):
            self.write({A: HEADER + 'new\n'})
        self.assertEqual((self.root / A).read_text(), HEADER + 'a\n')
        self.assertEqual((self.root / outputs.MANIFEST).read_bytes(), before)
        unknown = self.root / 'client/python/zipzi/_generated/unknown.py'
        unknown.write_text(HEADER + 'unknown\n')
        with self.assertRaisesRegex(ValueError, 'Unmanaged'):
            self.write({A: HEADER + 'a\n', B: HEADER + 'b\n'})
        self.assertTrue(unknown.exists())

    def test_symlinks_and_unsafe_paths_fail_before_writes(self):
        outside = self.root / 'outside'
        outside.mkdir()
        (self.root / 'client/python/zipzi/_generated/domain').symlink_to(outside, target_is_directory=True)
        with self.assertRaisesRegex(ValueError, 'symlink'):
            self.write({A: HEADER + 'new\n', B: HEADER + 'b\n'})
        self.assertEqual((self.root / A).read_text(), HEADER + 'a\n')
        with self.assertRaises(ValueError):
            self.write({A: HEADER + 'a\n', 'src/lib.rs': HEADER + 'x\n'})

    def test_interrupted_publication_is_reported_and_recoverable(self):
        real_replace = outputs.os.replace
        calls = 0
        def interrupted(source, target):
            nonlocal calls
            calls += 1
            if calls == 2:
                raise OSError('interrupted')
            return real_replace(source, target)
        data = {A: HEADER + 'new\n', B: HEADER + 'b\n'}
        with patch.object(outputs.os, 'replace', side_effect=interrupted):
            with self.assertRaisesRegex(OSError, 'interrupted'):
                self.write(data)
        self.assertTrue(self.write(data, True))
        self.write(data)
        self.assertEqual(self.write(data, True), [])


class CodegenStats(unittest.TestCase):
    def test_static_counts_and_domain_coverage(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            generated = root / 'client/python/zipzi/_generated'
            (generated / 'values').mkdir(parents=True)
            (generated / 'modeling.py').write_text('class A:\n def __init__(self):\n  object.__setattr__(self, "x", self.x)\n')
            (generated / 'values/mesh.py').write_text('class B: pass\n')
            (generated / 'catalog.py').write_text("CATALOG: dict = {'a.summary': {'documentation': 'same'}, 'b.summary': {'documentation': 'same'}, 'A': {'documentation': 'Fields:\\nx', 'fields': [('x', 'x')]}}\n")
            result = stats.inspect(root)
            self.assertEqual(result['modeling_classes'], 2)
            self.assertEqual(result['domain_coverage'], 0.5)
            self.assertEqual(result['invalid_self_setattrs'], 1)
            self.assertEqual(result['entries_with_repeated_field_blocks'], 1)
            self.assertEqual(result['inherited_method_docs']['summary']['duplicate_descriptions'], 1)


if __name__ == '__main__':
    unittest.main()
