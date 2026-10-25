"""Independent codegen acceptance: determinism, protected sources and Rust typing."""
from copy import deepcopy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = ROOT / 'crates/zipzi-python2'
SPEC = importlib.util.spec_from_file_location('python2_generate', PACKAGE / 'tools/generate.py')
gen = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gen)

class Python2Codegen(unittest.TestCase):
    def test_selected_dtos_follow_compiled_fields_and_reject_unknown_shapes(self):
        contract = self.contract()["dtos"]
        output = gen._dto_codegen.render_dtos(contract, gen.HEADER)
        self.assertIn("class ParameterPatch:", output["client/python/zipzi/_generated/parameter_values.py"])
        self.assertIn("keys: Sequence[float]", output["client/python/zipzi/_generated/binding_values.py"])
        changed = deepcopy(contract)
        spec = changed["parameters"][0]["variants"][0]["fields"][0]["shape"]
        spec["fields"].append({"name":"future_flag", "shape":{"kind":"bool"}, "default":None})
        generated = gen._dto_codegen.render_dtos(changed, gen.HEADER)["client/python/zipzi/_generated/parameter_values.py"]
        self.assertIn("future_flag: bool", generated)
        self.assertIn("_flag(_get(value, 'future_flag'))", generated)
        self.assertIn("'future_flag': value.future_flag", generated)
        spec["fields"][-1]["shape"] = {"kind":"any"}
        with self.assertRaises(ValueError):
            gen._dto_codegen.render_dtos(changed, gen.HEADER)

    def test_scene_contract_protects_shapes_and_generates_nested_ownership_and_typed_native_inputs(self):
        contract = self.contract()
        output = gen.render(contract)
        python = output['client/python/zipzi/_generated/scene.py']
        native = output['src/generated/scene.rs']
        self.assertIn('triangles: tuple[tuple[int, int, int], ...]', python)
        self.assertIn('tuple(tuple(item1 for item1 in item0) for item0 in self.triangles)', python)
        self.assertIn('Expected i32', python)
        self.assertIn('collect::<PyResult<_>>()?', native)
        self.assertIn('zipzi_sdk::SceneOperation::CreateBlendTable', native)
        for mutate in (
            lambda c: c['scene'].update(version=999),
            lambda c: c['scene']['bindings'].append(deepcopy(c['scene']['bindings'][0])),
            lambda c: c['scene']['roots'][0].update(tag='unchecked'),
            lambda c: c['scene']['roots'][0]['variants'][0]['fields'][0].update(shape={'kind':'any'}),
            lambda c: c['scene']['roots'][0]['variants'][0]['fields'][0].update(name='wire'),
        ):
            invalid = deepcopy(contract)
            mutate(invalid)
            with self.assertRaises(ValueError):
                gen.render(invalid)
    def test_binary_contract_guards_compiled_source_layout_and_typed_array_outputs(self):
        contract=self.contract()
        output=gen.render(contract)
        binary=output['client/python/zipzi/_generated/binary.py']
        self.assertIn('def decode_meshes(',binary)
        self.assertIn('def decode_samples(',binary)
        self.assertIn('Snapshot(_read_sourceref(r), meshes.Mesh(',binary)
        self.assertIn("r.numeric('III', r.count())",binary)
        self.assertIn('w.raw(b"ZZA1")',binary)
        self.assertIn('mesh_read_bytes',output['src/generated/queries.rs'])
        self.assertIn('if isinstance(value,cls): return value',output['client/python/zipzi/_generated/meshes.py'])
        for mutate in (
            lambda c:c['binary'].update(source_variants=['Edit','Formal']),
            lambda c:c['binary'].update(source_fields=[['version','session'],['session','edit','base','sequence']]),
            lambda c:c['binary'].update(version=999),
            lambda c:c['binary']['edit_variants'].append('Unreviewed'),
        ):
            invalid=deepcopy(contract);mutate(invalid)
            with self.assertRaises(ValueError):gen.render(invalid)

    def contract(self):
        return json.loads((PACKAGE / 'schema/authoring.json').read_text())

    def files(self, folder):
        return {p.relative_to(folder).as_posix(): p.read_bytes() for p in folder.rglob('*') if p.is_file()}

    def test_compiled_contract_matches_committed_outputs_without_writing(self):
        contract = gen.export()
        output = gen.render(contract)
        self.assertEqual(gen.write_outputs(output, check=True), [])
        self.assertEqual(output, gen.render(self.contract()))

    def test_generation_is_deterministic_and_check_only_never_repairs_drift(self):
        output = gen.render(self.contract())
        self.assertEqual(output, gen.render(self.contract()))
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            gen.write_outputs(output, package=folder)
            changed = folder / 'client/python/zipzi/_generated/mesh_ops.py'
            changed.write_text(changed.read_text() + '# drift\n')
            before = self.files(folder)
            self.assertEqual(gen.write_outputs(output, check=True, package=folder), ['client/python/zipzi/_generated/mesh_ops.py'])
            self.assertEqual(self.files(folder), before)
            gen.write_outputs(output, package=folder)
            self.assertEqual(gen.write_outputs(output, check=True, package=folder), [])

    def test_handwritten_files_unowned_paths_and_symlinks_fail_before_any_write(self):
        output = gen.render(self.contract())
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            protected = folder / 'src/generated/queries.rs'
            protected.parent.mkdir(parents=True)
            protected.write_text('// handwritten\n')
            before = self.files(folder)
            for check in (True, False):
                with self.assertRaisesRegex(ValueError, 'handwritten'):
                    gen.write_outputs(output, check=check, package=folder)
                self.assertEqual(self.files(folder), before)
            malicious = dict(output, **{'src/lib.rs': '// bad'})
            with self.assertRaises(ValueError):
                gen.write_outputs(malicious, package=folder)
            self.assertEqual(self.files(folder), before)
            protected.unlink()
            outside = folder / 'outside.rs'
            outside.write_text('// user\n')
            protected.symlink_to(outside)
            with self.assertRaisesRegex(ValueError, 'symlink'):
                gen.write_outputs(output, package=folder)
            self.assertEqual(outside.read_text(), '// user\n')
            protected.unlink()
            protected.parent.rmdir()
            redirected = folder / 'redirected'
            redirected.mkdir()
            protected.parent.symlink_to(redirected, target_is_directory=True)
            before = self.files(folder)
            with self.assertRaisesRegex(ValueError, 'symlink'):
                gen.write_outputs(output, package=folder)
            self.assertEqual(self.files(folder), before)

    def test_unsupported_malformed_and_unregistered_shapes_fail(self):
        mutations = (
            lambda c: c['bindings'][0].update(name='Point; execute'),
            lambda c: c['bindings'].append(deepcopy(c['bindings'][0])),
            lambda c: c['operations'][0].update(handler=None),
            lambda c: c['roots'][0]['fields'][0].update(shape={'kind': 'any'}),
            lambda c: c['roots'][0]['fields'][0].update(name='read'),
            lambda c: c['roots'][0]['fields'][0].update(default={'kind': 'empty_string'}),
            lambda c: c['roots'][0]['fields'][0].update(shape={'kind': 'array', 'length': 2, 'item': {'kind': 'u32'}}),
        )
        for mutate in mutations:
            contract = self.contract()
            mutate(contract)
            with self.assertRaises(ValueError):
                gen.render(contract)

    def test_rust_field_change_updates_python_wire_and_native_shapes_together(self):
        contract = self.contract()
        def change(node):
            if isinstance(node, dict):
                if node.get('kind') == 'record' and node.get('name', '').endswith('::MeshGeometry'):
                    node['fields'].append(dict(name='segments', shape=dict(kind='u32'), default=None))
                for value in node.values():
                    change(value)
            elif isinstance(node, list):
                for value in node:
                    change(value)
        change(contract)
        output = gen.render(contract)
        self.assertIn('segments: int', output['client/python/zipzi/_generated/meshes.py'])
        self.assertIn('segments=op.spec.geometry.segments', output['client/python/zipzi/_generated/mesh_ops.py'])
        self.assertIn('value.getattr("segments")', output['src/generated/mesh.rs'])
        self.assertIn('value.segments', output['src/generated/mesh.rs'])
        # This test supplies a changed shape; it never supplies a second list
        # of field definitions to any emitter.

    def test_missing_wrong_handler_and_unsupported_rust_type_really_fail_compilation(self):
        original_lock = (ROOT / 'Cargo.lock').read_bytes()
        with tempfile.TemporaryDirectory(prefix='zz-schema-') as folder:
            folder = Path(folder)
            (folder / 'src').mkdir()
            (folder / 'Cargo.toml').write_text('[package]\nname="zipzi_schema_compile_controls"\nversion="0.0.0"\nedition="2021"\n[workspace]\n[dependencies]\n'
                + 'zipzi-core={path=' + json.dumps(str(ROOT / 'crates/zipzi-core')) + '}\n'
                + 'zipzi-runtime={path=' + json.dumps(str(ROOT / 'crates/zipzi-runtime')) + '}\n'
                + 'zipzi-api-derive={path=' + json.dumps(str(ROOT / 'crates/zipzi-api-derive')) + '}\n'
                + 'serde={version="1",features=["derive"]}\n')
            cases = (
                ('use zipzi_runtime::authoring::*; fn main() { let _ = MESH_QUERY.shapes(); }', None),
                ('use zipzi_runtime::authoring::*; fn main() { let _ = (IMAGE_QUERY.shapes(), PLAN_QUERY.shapes(), PREPARED_PIXELS_QUERY.shapes()); }', None),
                ('use zipzi_runtime::{authoring::*,*}; fn wrong(_: &mut Runtime, _: ClientId, _: PlanQuery) -> Result<ImagePixels> { todo!() } fn main() { let _ = Registered::<PlanQuery,PlanSnapshot>::new("bad", wrong); }', 'E0308'),
                ('use zipzi_runtime::authoring::*; fn main() { let _ = Registered::<MeshQuery,MeshSnapshot>::new("missing"); }', 'E0061'),
                ('use zipzi_runtime::{authoring::*,*}; fn wrong(_: &mut Runtime, _: ClientId, _: MeshQuery) -> Result<()> { Ok(()) } fn main() { let _ = Registered::<MeshQuery,MeshSnapshot>::new("bad", wrong); }', 'E0308'),
                ('zipzi_core::api_record! { pub struct Supported { pub values: std::collections::HashMap<String,String> } } fn main() {}', None),
                ('zipzi_core::api_record! { pub struct Unsupported { pub values: std::collections::HashMap<bool,String> } } fn main() {}', 'E0277'),
                ('zipzi_core::api_record! { pub struct Supported { pub values: std::collections::BTreeMap<String,u32> } } fn main() {}', None),
                ('zipzi_core::api_record! { pub struct Unsupported { pub values: std::collections::BTreeMap<String,std::num::NonZeroU64> } } fn main() {}', 'E0277'),
                ('#[derive(serde::Serialize,serde::Deserialize,zipzi_api_derive::ApiShape)] pub struct Unsupported { #[serde(flatten)] pub fields: std::collections::BTreeMap<String,String> } fn main() {}', 'selected'),
                ('zipzi_core::api_record! { #[derive(serde::Serialize,serde::Deserialize)] #[serde(rename_all="camelCase")] pub struct Unsupported { pub some_field: String } } fn main() {}', 'macro'),
                ('zipzi_core::api_record! { #[derive(serde::Serialize,serde::Deserialize)] pub struct Unsupported { #[serde(default)] #[serde(rename="wire")] pub field: String } } fn main() {}', 'macro'),
                ('zipzi_core::api_union! { tag="op"; #[derive(serde::Serialize,serde::Deserialize)] #[serde(rename_all_fields="camelCase")] pub enum Unsupported { Change { some_field: String } } } fn main() {}', 'macro'),
            )
            for source, code in cases:
                (folder / 'src/main.rs').write_text(source)
                result = subprocess.run(['cargo', 'check', '--offline', '--manifest-path', str(folder / 'Cargo.toml'), '--message-format=json'],
                    cwd=ROOT, env=dict(os.environ, CARGO_TARGET_DIR=str(ROOT / 'target')), capture_output=True, text=True)
                errors = [r['message'] for line in result.stdout.splitlines() if (r := json.loads(line)).get('reason') == 'compiler-message' and r['message']['level'] == 'error']
                if code is None:
                    self.assertEqual(result.returncode, 0, result.stderr + result.stdout[-4000:])
                else:
                    self.assertNotEqual(result.returncode, 0)
                    if code == 'selected':
                        self.assertTrue(any('Unsupported selected serde attribute' in e['message'] for e in errors), result.stderr + result.stdout[-4000:])
                    elif code == 'macro':
                        self.assertTrue(any('no rules expected' in e['message'] for e in errors), result.stderr + result.stdout[-4000:])
                    else:
                        self.assertIn(code, [(e.get('code') or {}).get('code') for e in errors], result.stderr + result.stdout[-4000:])
        self.assertEqual((ROOT / 'Cargo.lock').read_bytes(), original_lock)

    def test_resource_fields_propagate_to_values_wire_native_and_convenience(self):
        contract = self.contract()
        def change(node):
            if isinstance(node, dict):
                if node.get('kind') == 'record' and node.get('name', '').endswith('::ImageCreateSpec'):
                    node['fields'].append(dict(name='label', shape=dict(kind='string'), default=None))
                    node['fields'].append(dict(name='description', shape=dict(kind='string'), default=dict(kind='empty_string')))
                if node.get('kind') == 'record' and node.get('name', '').endswith('::ConversionCost'):
                    node['fields'].append(dict(name='operations', shape=dict(kind='u64'), default=None))
                for value in node.values():
                    change(value)
            elif isinstance(node, list):
                for value in node:
                    change(value)
        change(contract)
        output = gen.render(contract)
        self.assertIn('label: str', output['client/python/zipzi/_generated/resource_ops.py'])
        self.assertIn('label=op.label', output['client/python/zipzi/_generated/resource_ops.py'])
        self.assertIn('label: str', output['client/python/zipzi/_generated/resource_conveniences.py'])
        self.assertIn('op.getattr("label")', output['src/generated/resources.rs'])
        self.assertIn('input_budget::text(&op.getattr("label")?)?', output['src/generated/resources.rs'])
        self.assertIn('measure_planintent(intent)?', output['src/generated/resource_queries.rs'])
        self.assertIn('operations: int', output['client/python/zipzi/_generated/plan_values.py'])
        self.assertIn('value.operations', output['src/generated/resources.rs'])
        self.assertIn('description: str = ""', output['client/python/zipzi/_generated/resource_ops.py'])
        convenience = output['client/python/zipzi/_generated/resource_conveniences.py']
        self.assertIn('description: str = ""', convenience)
        compile(convenience, '<generated resource convenience>', 'exec')

    def test_resource_contract_rejects_unknown_maps_tokens_and_handlers(self):
        def replace_maps(node):
            if isinstance(node, dict):
                if node.get('kind') == 'string_map':
                    node['value'] = {'kind': 'u32'}
                for value in node.values():
                    replace_maps(value)
            elif isinstance(node, list):
                for value in node:
                    replace_maps(value)
        for mutate in (replace_maps,
                       lambda c: c['resources']['operations'][0].update(handler=None),
                       lambda c: c['resources']['bindings'].__setitem__(0, dict(c['resources']['bindings'][0], name='read;evil')),
                       lambda c: c['resources']['bindings'].pop()):
            contract = self.contract()
            mutate(contract)
            with self.assertRaises(ValueError):
                gen.render(contract)

if __name__ == '__main__':
    unittest.main()
