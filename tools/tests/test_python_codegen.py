"""V4 generated values: compiled-source drift, reproducibility and file ownership."""
import ast
from copy import deepcopy
import importlib.util
import json
import subprocess
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = ROOT / 'crates/zipzi-python'
SPEC = importlib.util.spec_from_file_location('python_generate', PACKAGE / 'tools/generate.py')
gen = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gen)


def fixture():
    return {'protocol': 'zipzi-runtime-v4', 'record': {
        'kind': 'serde_record', 'name': 'TestValue', 'fields': [
            {'name': 'name', 'shape': {'kind': 'string'}},
            {'name': 'points', 'shape': {'kind': 'sequence', 'item': {'kind': 'tuple', 'items': [{'kind': 'f32'}, {'kind': 'f32'}]}}, 'default': []},
            {'name': 'from', 'shape': {'kind': 'optional', 'item': {'kind': 'string'}}},
        ]}}


class PythonCodegen(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.contract = gen.export_contract()

    def test_compiled_contract_matches_all_owned_outputs(self):
        self.assertEqual(gen.write_outputs(gen.render(self.contract), check=True), [])
        schema = json.loads((PACKAGE / 'schema/authoring.json').read_text())
        self.assertEqual(schema['roots']['protocol'], 'zipzi-runtime-v4')
        operations = schema['definitions']['Operation']['variants']
        names = {v['name'] for v in operations}
        self.assertTrue({'batch', 'preview', 'publish', 'query', 'undo', 'redo'} <= names)
        self.assertFalse({'capture_address', 'patch_address', 'begin_edit'} & names)

    def test_render_is_deterministic_and_valid_python(self):
        first = gen.render(self.contract)
        self.assertEqual(first, gen.render(deepcopy(self.contract)))
        ast.parse(first['client/python/zipzi/values.py'])
        self.assertNotIn('ObjectHandle', first['client/python/zipzi/values.py'])

    def test_public_types_accept_owned_values_and_reject_wrong_sources(self):
        command = ['uv', 'run', '--locked', 'pyright', '--project',
                   str(PACKAGE / 'pyrightconfig.json'), '--outputjson']
        valid = subprocess.run([*command, str(PACKAGE / 'tests/type_kernel.py')],
                               cwd=ROOT, capture_output=True, text=True)
        self.assertEqual(valid.returncode, 0, valid.stdout + valid.stderr)
        invalid = PACKAGE / 'tests/type_kernel_invalid.py'
        rejected = subprocess.run([*command, str(invalid)], cwd=ROOT,
                                  capture_output=True, text=True)
        self.assertEqual(rejected.returncode, 1, rejected.stderr)
        diagnostics = json.loads(rejected.stdout)['generalDiagnostics']
        expected = {line for line, text in enumerate(invalid.read_text().splitlines()) if '# error' in text}
        self.assertEqual({d['range']['start']['line'] for d in diagnostics}, expected)
        self.assertEqual(len(diagnostics), len(expected))

    def test_field_default_and_operation_drift_change_outputs(self):
        for change in ('field', 'default', 'type'):
            with self.subTest(change=change):
                contract = fixture()
                original = gen.render(contract)
                if change == 'field': contract['record']['fields'].append({'name':'extra', 'shape':{'kind':'bool'}})
                elif change == 'default': contract['record']['fields'][1]['default'] = [[1., 2.]]
                else: contract['record']['fields'][0]['shape'] = {'kind':'u64'}
                updated = gen.render(contract)
                self.assertNotEqual(original['schema/authoring.json'], updated['schema/authoring.json'])
                self.assertNotEqual(original['client/python/zipzi/values.py'], updated['client/python/zipzi/values.py'])

    def test_tuple_optional_and_keyword_fields_are_typed(self):
        output = gen.render(fixture())['client/python/zipzi/values.py']
        ast.parse(output)
        self.assertIn('tuple[tuple[float, float], ...]', output)
        self.assertIn('from_: str | None = None', output)

    def test_conflicting_public_names_and_unknown_shapes_are_rejected(self):
        contract = fixture()
        contract['second'] = deepcopy(contract['record'])
        contract['second']['name'] = 'other::TestValue'
        with self.assertRaisesRegex(ValueError, 'Conflicting public type'):
            gen.render(contract)
        contract = fixture()
        contract['record']['fields'][0]['shape'] = {'kind':'opaque', 'name':'PrivateState'}
        with self.assertRaisesRegex(ValueError, 'Unsupported public shape'):
            gen.render(contract)

    def test_check_never_changes_files_and_manifest_tracks_outputs(self):
        output = gen.render(fixture())
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.assertTrue(gen.write_outputs(output, check=True, package=root))
            self.assertEqual(list(root.iterdir()), [])
            gen.write_outputs(output, package=root)
            before = {p.relative_to(root):p.read_bytes() for p in root.rglob('*') if p.is_file()}
            self.assertEqual(gen.write_outputs(output, check=True, package=root), [])
            changed = fixture()
            changed['record']['fields'][0]['shape'] = {'kind':'bool'}
            self.assertTrue(gen.write_outputs(gen.render(changed), check=True, package=root))
            self.assertEqual(before, {p.relative_to(root):p.read_bytes() for p in root.rglob('*') if p.is_file()})

    def test_handwritten_files_and_symlink_targets_are_protected(self):
        output = gen.render(fixture())
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            target = root / 'client/python/zipzi/values.py'
            target.parent.mkdir(parents=True)
            target.write_text('# handwritten\n')
            with self.assertRaisesRegex(ValueError, 'handwritten'):
                gen.write_outputs(output, package=root)
            target.unlink()
            target.symlink_to(root / 'outside.py')
            with self.assertRaisesRegex(ValueError, 'symlink'):
                gen.write_outputs(output, package=root)

    def test_retired_modified_outputs_are_not_deleted(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            output = gen.render(fixture())
            gen.write_outputs(output, package=root)
            (root / 'client/python/zipzi/values.py').write_text(gen.HEADER + '# local change\n')
            with self.assertRaisesRegex(ValueError, 'modified generated file'):
                gen.write_outputs({'schema/authoring.json': output['schema/authoring.json']}, package=root)


if __name__ == '__main__':
    unittest.main()
