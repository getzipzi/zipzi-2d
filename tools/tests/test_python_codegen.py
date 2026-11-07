"""V4 generated values: compiled-source drift, reproducibility and file ownership."""
import ast
from copy import deepcopy
import importlib.util
import json
import re
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
        for path, source in first.items():
            if path.endswith('.py'):
                ast.parse(source)

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
        # Overload resolution can emit both a call and an argument diagnostic
        # for one invalid line. Every marked line must fail, and no other may.

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
                self.assertNotEqual(original['client/python/zipzi/_schema.json'], updated['client/python/zipzi/_schema.json'])
                if change != 'default':
                    self.assertNotEqual(original['client/python/zipzi/values/common.py'], updated['client/python/zipzi/values/common.py'])

    def test_field_help_is_preserved_and_codec_metadata_is_not_inline(self):
        contract = fixture()
        contract['record']['fields'][0]['doc'] = 'Display label, not a stable object identity.'
        output = gen.render(contract)
        source = output['client/python/zipzi/values/common.py']
        value = next(node for node in ast.parse(source).body if isinstance(node, ast.ClassDef))
        self.assertIn('name: Display label, not a stable object identity.', ast.get_docstring(value))
        self.assertNotIn("'kind':", source)
        for path, source in gen.render(self.contract).items():
            if path.endswith('.py'):
                self.assertLess(max(map(len, source.splitlines())), 1024)

    def test_reference_covers_exports_and_links_resolve(self):
        output = gen.render(self.contract)
        directory = gen.reference.DIRECTORY
        for path, source in output.items():
            if path.startswith('client/python/zipzi/values/') and not path.endswith('__init__.py'):
                page = output[directory + 'values-' + Path(path).stem + '.md']
                for exported in gen.reference.exports(ast.parse(source)):
                    self.assertIn('## zipzi.values.' + exported + '\n', page)
        entrypoints = ast.parse((PACKAGE / 'client/python/zipzi/__init__.py').read_text())
        for exported in gen.reference.exports(entrypoints):
            self.assertIn('zipzi.' + exported, output[directory + 'index.md'])
        for path, page in output.items():
            if not path.startswith(directory):
                continue
            anchors = re.findall(r'<a id="([^"]+)">', page)
            self.assertEqual(len(anchors), len(set(anchors)), path)
            prose = re.sub(r'```.*?```', '', page, flags=re.DOTALL)
            for link in re.findall(r'\]\(([^)]+)\)', prose):
                destination, _, anchor = link.partition('#')
                target = (PACKAGE / path).parent / destination
                relative = target.resolve().relative_to(PACKAGE.resolve()).as_posix()
                self.assertTrue(relative in output or target.is_file(), (path, link))
                if anchor:
                    self.assertIn(f'<a id="{anchor}">', output[relative], (path, link))

    def test_reference_tracks_contract_defaults_and_public_source(self):
        contract = fixture()
        contract['record']['fields'][0]['doc'] = 'Stable | label.\nSecond line.'
        output = gen.render(contract)
        page = output[gen.reference.DIRECTORY + 'values-common.md']
        self.assertIn('Stable \\| label.<br>Second line.', page)
        self.assertIn('| `name` | `str` | Required |', page)
        self.assertIn('| `from_` | `str \\| None` | `null` (`None`) |', page)
        contract['record']['fields'][1]['default'] = [[1., 2.]]
        changed = gen.render(contract)[gen.reference.DIRECTORY + 'values-common.md']
        self.assertNotEqual(page, changed)
        self.assertIn('[[1.0, 2.0]]', changed)

        sources = {p: (PACKAGE / gen.reference.PREFIX / p).read_text() for p in gen.reference.SOURCES}
        sources['__init__.py'] = sources['__init__.py'].replace('timeout: float = 10.0', 'timeout: float = 12.5').replace('Connect to an Editor', 'Connect to the selected Editor')
        pages = gen.reference.render(output, {}, PACKAGE, sources=sources)
        self.assertIn('timeout: float=12.5', pages[gen.reference.DIRECTORY + 'runtime.md'])
        self.assertIn('Connect to the selected Editor', pages[gen.reference.DIRECTORY + 'runtime.md'])

    def test_reference_preserves_overloads_variants_and_inheritance(self):
        output = gen.render(self.contract)
        runtime = output[gen.reference.DIRECTORY + 'runtime.md']
        query = runtime.split('### zipzi.Session.query\n')[1].split('<a id=')[0]
        self.assertIn('@overload', query)
        self.assertIn('Snapshot[r.Objects]', query)
        self.assertIn('Snapshot[r.Collection]', query)
        self.assertNotIn('def _call', runtime)
        parameters = output[gen.reference.DIRECTORY + 'values-parameters.md']
        self.assertIn('| `NORMAL` |', parameters)
        self.assertIn('Drives ordinary bindings', parameters)
        commands = output[gen.reference.DIRECTORY + 'values-commands.md']
        variants = commands.split('Variants: ', 1)[1].split('\n', 1)[0]
        self.assertIn('[CommandCreateParameter]', variants)
        results = output[gen.reference.DIRECTORY + 'results.md']
        collection = results.split('## zipzi.results.Collection\n')[1].split('<a id=')[0]
        self.assertIn('[zipzi.results.Objects]', collection)
        self.assertIn('type Capability =', results)

    def test_public_instance_attributes_are_declared_and_documented(self):
        output = gen.render(self.contract)
        runtime = output[gen.reference.DIRECTORY + 'runtime.md']
        tree = ast.parse((PACKAGE / gen.reference.PREFIX / '_kernel.py').read_text())
        for cls in tree.body:
            if not isinstance(cls, ast.ClassDef):
                continue
            initializers = [n for n in cls.body if isinstance(n, ast.FunctionDef) and n.name == '__init__']
            declared = {n.target.id for n in cls.body if isinstance(n, ast.AnnAssign) and isinstance(n.target, ast.Name)}
            public = {n.attr for init in initializers for n in ast.walk(init)
                      if isinstance(n, ast.Attribute) and isinstance(n.ctx, ast.Store)
                      and isinstance(n.value, ast.Name) and n.value.id == 'self'
                      and not n.attr.startswith('_')}
            self.assertLessEqual(public, declared, cls.name)
            if public:
                section = runtime.split('## zipzi.' + cls.name + '\n')[1].split('\n## ')[0]
                for field in public:
                    self.assertIn('| `' + field + '` |', section)

    def test_tuple_optional_and_keyword_fields_are_typed(self):
        output = gen.render(fixture())['client/python/zipzi/values/common.py']
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

    def test_export_replay_rejects_duplicate_keys_and_unresolved_references(self):
        with self.assertRaisesRegex(ValueError, 'Duplicate schema key'):
            gen.read_export('{"protocol":"zipzi-runtime-v4","protocol":"zipzi-runtime-v3"}')
        contract = fixture()
        contract['record']['fields'][0]['shape'] = {'kind':'ref', 'name':'Missing'}
        with self.assertRaisesRegex(ValueError, 'Unresolved public type'):
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
            target = root / 'client/python/zipzi/values/__init__.py'
            target.parent.mkdir(parents=True)
            target.write_text('# handwritten\n')
            with self.assertRaisesRegex(ValueError, 'handwritten'):
                gen.write_outputs(output, package=root)
            target.unlink()
            target.symlink_to(root / 'outside.py')
            with self.assertRaisesRegex(ValueError, 'symlink'):
                gen.write_outputs(output, package=root)


if __name__ == '__main__':
    unittest.main()
