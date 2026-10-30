"""Independent codegen acceptance: determinism, protected sources and Rust typing."""
from copy import deepcopy
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
PACKAGE = ROOT / 'crates/zipzi-python2'
SPEC = importlib.util.spec_from_file_location('python2_generate', PACKAGE / 'tools/generate.py')
gen = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gen)


class Python2ContractIsolation(unittest.TestCase):
    def test_real_implementation_and_contract_changes_in_an_isolated_checkout(self):
        spec = importlib.util.spec_from_file_location('contract_acceptance',
            ROOT / 'tools/python/python2_codegen_acceptance.py')
        acceptance = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(acceptance)
        with tempfile.TemporaryDirectory(prefix='zipzi contract 中文 ') as folder:
            report = acceptance.Acceptance(Path(folder), samples=1).run()
        self.assertEqual(report['status'], 'passed')
        for scenario in ('core', 'sdk', 'runtime', 'field-drift', 'default-drift',
                         'enum-drift', 'operation-input-drift', 'operation-output-drift',
                         'cache-deleted', 'invalid-contract', 'failure-recovered'):
            self.assertIn(scenario, report['summary'])


class Python2Codegen(unittest.TestCase):
    def test_exporter_dependency_closure_contains_no_implementation_packages(self):
        metadata = json.loads(subprocess.check_output(
            ['cargo', 'metadata', '--locked', '--no-deps', '--format-version', '1'], cwd=ROOT))
        owned = {p['name'] for p in metadata['packages'] if p['id'] in metadata['workspace_members']}
        tree = subprocess.check_output(
            ['cargo', 'tree', '--locked', '-p', 'zipzi-contract', '--all-features', '--target', 'all', '-e', 'normal,build',
             '--prefix', 'none', '--format', '{p}'], cwd=ROOT, text=True)
        names = {line.split(' v', 1)[0] for line in tree.splitlines()}
        self.assertEqual(names & owned, {'zipzi-contract', 'zipzi-api-derive'})
        self.assertFalse({name for name in names if name.startswith(
            ('pyo3', 'wgpu', 'naga', 'metal', 'objc2-metal', 'ash', 'glow', 'gpu-'))})

    def test_export_uses_checkout_target_even_with_external_cargo_target(self):
        result = unittest.mock.Mock(returncode=0, stdout='{}', stderr='')
        with patch.dict(os.environ, {'CARGO_TARGET_DIR': '/external/target'}), \
             patch.object(gen.subprocess, 'run', return_value=result) as run:
            self.assertEqual(gen.export(), {})
        command = run.call_args.args[0]
        self.assertEqual(command[command.index('--target-dir') + 1], str(ROOT / 'target/python2/schema-cargo'))
        self.assertEqual(command[command.index('-p') + 1], 'zipzi-contract')

    def test_generation_and_check_hold_the_same_checkout_lock_through_writes(self):
        from contextlib import contextmanager
        held = False
        paths = []

        @contextmanager
        def lock(path):
            nonlocal held
            self.assertFalse(held)
            held = True
            paths.append(path)
            try:
                yield
            finally:
                held = False

        def require_lock(*args, **kwargs):
            self.assertTrue(held)
            return {}

        for check in (False, True):
            args = ['generate.py'] + (['--check'] if check else [])
            with patch.object(gen.sys, 'argv', args), patch.object(gen, 'file_lock', lock), \
                 patch.object(gen, 'export', side_effect=require_lock), \
                 patch.object(gen, 'cached_render', side_effect=require_lock), \
                 patch.object(gen, 'write_outputs', side_effect=require_lock) as write:
                gen.main()
            self.assertFalse(held)
            self.assertEqual(write.call_args.args[1], check)
        self.assertEqual(paths, [ROOT / 'target/python2/generate.lock'] * 2)

    def test_render_cache_reuses_and_invalidates_inputs_and_recovers_from_corruption(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / 'tools').mkdir()
            (root / 'tools/generate.py').write_text('generator')
            (root / 'tools/codegen').mkdir()
            (root / 'tools/codegen/helper.py').write_text('helper')
            client = root / 'client/python/zipzi'
            (client / 'docs').mkdir(parents=True)
            (client / 'docs/guide.md').write_text('guide')
            (client / 'api.py').write_text('api')
            (root / 'client/pyproject.toml').write_text('version')
            cache = root / 'cache.json'
            output = {path: 'generated' for path in gen.TARGETS}
            with patch.object(gen, 'PACKAGE', root), patch.object(gen.subprocess, 'check_output', return_value='rustfmt 1'), patch.object(gen, 'render', return_value=output) as render:
                contract = {'version': 1}
                self.assertEqual(gen.cached_render(contract, cache), output)
                self.assertEqual(gen.cached_render(contract, cache), output)
                self.assertEqual(render.call_count, 1)
                (client / 'api.py').write_text('changed')
                gen.cached_render(contract, cache)
                gen.cached_render({'version': 2}, cache)
                (root / 'tools/codegen/helper.py').write_text('changed helper')
                gen.cached_render(contract, cache)
                cache.write_text('{invalid')
                gen.cached_render(contract, cache)
                self.assertEqual(render.call_count, 5)
                cache.write_text('{invalid')
                gen.cached_render(contract, cache, read_only=True)
                self.assertEqual(cache.read_text(), '{invalid')

    def test_catalog_navigation_and_public_query_defaults(self):
        output=gen.render(self.contract());namespace={}
        exec(output['client/python/zipzi/_generated/catalog.py'],namespace)
        catalog=namespace['CATALOG']
        for path in catalog:
            parent=path.rpartition('.')[0]
            if parent in catalog:
                self.assertIn(path,catalog[parent]['children'])
        for path in ('Runtime.sessions','Runtime.resources','Session.project'):
            self.assertTrue(catalog['zipzi.'+path]['children'])
        signature=catalog['zipzi.modeling.ModelingQuery']['signature']
        self.assertNotIn('field(',signature)
        self.assertIn('assets: tuple[str, ...] = ()',signature)
        self.assertIn('physics: bool = False',signature)
        self.assertIn('`'+signature+'`',output['client/python/zipzi/docs/reference.md'])

    def test_full_help_preserves_content_and_query_runtime_defaults(self):
        import sys
        with patch.object(sys,'path',[str(PACKAGE/'client/python'),*sys.path]):
            from zipzi import ApiDescription, modeling
            help=ApiDescription('zipzi.test','type','signature'*100,'documentation'*100,
                (('field','meaning'*100),),('zipzi.test.child',),'docs/recipes.py')
            full=help.format()
            for value in (help.path,help.kind,help.signature,help.documentation,
                          help.fields[0][1],help.children[0],help.example):
                self.assertIn(value,full)
            self.assertLess(len(repr(help)),len(full))
            query=modeling.ModelingQuery()
            self.assertEqual(query.assets,())
            self.assertIs(query.physics,False)
            self.assertIs(query.pose,False)

    def test_flattened_physics_extensions_roundtrip_and_cannot_shadow_fields(self):
        import sys
        with patch.object(sys, 'path', [str(PACKAGE / 'client/python'), *sys.path]):
            from zipzi._generated import modeling as codec
            domain = codec.PhysicsRange(Minimum=-1., Default=0., Maximum=1.)
            nested = {'enabled': True, 'levels': [1, 2]}
            value = codec.PhysicsNormalization(Position=domain, Angle=domain,
                extensions={'VendorNormalization': nested})
            nested['levels'].append(3)
            wire = codec.encode_PhysicsNormalization(value)
            self.assertEqual(wire, {'Position': {'Minimum': -1., 'Default': 0., 'Maximum': 1.},
                'Angle': {'Minimum': -1., 'Default': 0., 'Maximum': 1.},
                'VendorNormalization': {'enabled': True, 'levels': [1, 2]}})
            self.assertEqual(codec.read_PhysicsNormalization(wire), value)
            codec.validate_PhysicsNormalization(value)
            shadow = codec.PhysicsNormalization(Position=domain, Angle=domain,
                extensions={'Position': 42})
            with self.assertRaisesRegex(ValueError, 'overlap declared fields'):
                codec.validate_PhysicsNormalization(shadow)

    def test_single_variant_type_alias_keeps_fields_and_public_methods_in_catalog(self):
        output=gen.render(self.contract());namespace={}
        exec(output['client/python/zipzi/_generated/catalog.py'],namespace)
        catalog=namespace['CATALOG']
        alias='zipzi.modeling.PhysicsAssetProperty'
        concrete='zipzi.modeling.PhysicsAssetPropertyFps'
        self.assertEqual(catalog[alias]['kind'],'type')
        self.assertEqual(catalog[alias]['fields'],catalog[concrete]['fields'])
        self.assertTrue(dict(catalog[alias]['fields'])['value'])
        for method in ('summary','__repr__'):
            self.assertEqual(catalog[alias+'.'+method]['kind'],'callable')
            self.assertEqual(catalog[alias+'.'+method]['documentation'],
                             catalog[concrete+'.'+method]['documentation'])

    def test_catalog_version_follows_distribution_metadata(self):
        output=gen.render(self.contract())
        namespace={}
        exec(output['client/python/zipzi/_generated/catalog.py'],namespace)
        import tomllib
        version=tomllib.loads((PACKAGE/'client/pyproject.toml').read_text())['project']['version']
        self.assertEqual(namespace['PACKAGE_VERSION'],version)
        self.assertIn('Package '+version+';',namespace['CATALOG']['zipzi']['documentation'])
        self.assertEqual(namespace['CONTRACT'],self.contract()['protocol'])
        changed_protocol=deepcopy(self.contract());changed_protocol['protocol']='test-runtime-contract'
        protocol_output=gen.render(changed_protocol)
        exec(protocol_output['client/python/zipzi/_generated/catalog.py'],namespace)
        self.assertEqual(namespace['CONTRACT'],'test-runtime-contract')
        self.assertIn('protocol test-runtime-contract;',namespace['CATALOG']['zipzi']['documentation'])
        with patch.object(gen._catalog_codegen.tomllib,'loads',return_value={'project':{'version':'9.8.7a6'}}):
            changed=gen.render(self.contract())
        exec(changed['client/python/zipzi/_generated/catalog.py'],namespace)
        self.assertEqual(namespace['PACKAGE_VERSION'],'9.8.7a6')
        self.assertIn('Package 9.8.7a6;',changed['client/python/zipzi/docs/reference.md'])

    def test_handwritten_context_and_representation_methods_have_public_entries(self):
        output=gen.render(self.contract());namespace={}
        exec(output['client/python/zipzi/_generated/catalog.py'],namespace)
        catalog=namespace['CATALOG']
        for path in ('zipzi.Runtime.__enter__','zipzi.Runtime.__exit__',
                     'zipzi.Edit.__enter__','zipzi.Edit.__exit__','zipzi.Job.__exit__',
                     'zipzi.Plan.__exit__','zipzi.Buffer.__enter__','zipzi.Source.__repr__'):
            with self.subTest(path=path):
                self.assertEqual(catalog[path]['kind'],'callable')
                self.assertTrue(catalog[path]['documentation'])

    def test_missing_field_docs_fail_instead_of_generating_declaration_placeholders(self):
        contract = self.contract()
        def remove(node):
            if isinstance(node, dict):
                if node.get('name', '').endswith('::ParameterPatch') and node.get('kind') == 'serde_record':
                    next(field for field in node['fields'] if field['name'] == 'repeat')['doc'] = ''
                for value in node.values():
                    remove(value)
            elif isinstance(node, list):
                for value in node:
                    remove(value)
        remove(contract)
        with self.assertRaisesRegex(ValueError, r'Missing semantic field documentation: .*ParameterPatch.repeat'):
            gen.render(contract)

    def test_field_semantics_are_shared_by_docstrings_catalog_and_reference(self):
        output=gen.render(self.contract())
        namespace={}
        exec(output['client/python/zipzi/_generated/catalog.py'],namespace)
        catalog=namespace['CATALOG']
        examples={
            'zipzi.parameters.ParameterPatch':('repeat','None preserves the current value'),
            'zipzi.bindings.Keyform':('keys','axis order'),
            'zipzi.resources.ImagePixels':('rgba','straight alpha'),
            'zipzi.Source':('version','formal base version'),
            'zipzi.Snapshot':('source','captured atomically'),
            'zipzi.scene.VertexPatch':('position','BlendShape XY displacement'),
            'zipzi.modeling.CdiNamespaceIds':('parameters','document-ID to runtime-ID map'),
            'zipzi.modeling.BindingGridEditEdit':('operation','Typed axis/sample grid edit'),
            'zipzi.modeling.DeltaOffscreenKeyform':('multiply','neutral (1, 1, 1)'),
        }
        reference=output['client/python/zipzi/docs/reference.md']
        for path,(field,meaning) in examples.items():
            with self.subTest(path=path,field=field):
                entry=catalog[path]
                self.assertIn(meaning,' '.join(dict(entry['fields'])[field].split()))
                self.assertIn(meaning,' '.join(entry['documentation'].split()))
                self.assertIn(meaning,' '.join(reference.split()))
        mesh=output['client/python/zipzi/_generated/mesh_ops.py']
        self.assertIn('name precondition',mesh)

    def test_operation_docs_are_bound_to_the_declaring_domain_and_propagate_changes(self):
        contract=self.contract()
        def describe(node):
            if isinstance(node,dict):
                if isinstance(node.get('kind'),str) and node['kind']=='union' and node.get('name','').endswith('::MeshOperation'):
                    node['variants'][0]['fields'][0]['doc']='Mesh-only specification semantics.'
                if isinstance(node.get('kind'),str) and node['kind']=='serde_enum' and node.get('name','').endswith('::ParameterOperation'):
                    node['variants'][0]['fields'][0]['doc']='Parameter-only specification semantics.'
                for value in node.values():describe(value)
            elif isinstance(node,list):
                for value in node:describe(value)
        describe(contract)
        output=gen.render(contract)
        mesh=output['client/python/zipzi/_generated/mesh_ops.py']
        parameter=output['client/python/zipzi/_generated/parameter_values.py']
        self.assertIn('Mesh-only specification semantics.',mesh)
        self.assertNotIn('Parameter-only specification semantics.',mesh)
        self.assertIn('Parameter-only specification semantics.',parameter)
        self.assertNotIn('Mesh-only specification semantics.',parameter)

    def test_selected_dtos_follow_compiled_fields_and_reject_unknown_shapes(self):
        contract = self.contract()["dtos"]
        output = gen._dto_codegen.render_dtos(contract, gen.HEADER)
        self.assertIn("class ParameterPatch:", output["client/python/zipzi/_generated/parameter_values.py"])
        self.assertIn("keys: Sequence[float]", output["client/python/zipzi/_generated/binding_values.py"])
        changed = deepcopy(contract)
        spec = changed["parameters"][0]["variants"][0]["fields"][0]["shape"]
        spec["fields"].append({"name":"future_flag", "shape":{"kind":"bool"}, "default":None,
                               "doc":"Explicit future parameter behavior flag."})
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
            lambda c: c['roots'][0]['fields'][0].update(shape={'kind': 'array', 'length': 4, 'item': {'kind': 'u32'}}),
        )
        for mutate in mutations:
            contract = self.contract()
            mutate(contract)
            with self.assertRaises(ValueError):
                gen.render(contract)

    def test_union_fields_cannot_shadow_operation_kind_but_records_can_use_it(self):
        contract = self.contract()
        # MeshConstraint.kind is ordinary data and must remain supported.
        model = gen.Model(contract)
        self.assertTrue(any(f['name'] == 'kind'
                            for s in model.records for f in s['fields']))
        operation = next(s for s in contract['roots'] if s['kind'] == 'union')
        operation['variants'][0]['fields'].append(dict(
            name='kind', shape={'kind': 'string'}, default=None,
            doc='A domain field that collides with the generated discriminator.',
        ))
        with self.assertRaisesRegex(ValueError, 'Field shadows generated API'):
            gen.Model(contract)

    def test_struct_default_change_updates_generated_python(self):
        contract = self.contract()
        checks = contract['dtos']['sampling'][0]
        field = next(f for f in checks['fields'] if f['name'] == 'min_triangle_area')
        self.assertEqual(field['default'], 0.0)
        field['default'] = 0.5
        output = gen.render(contract)
        self.assertIn('min_triangle_area: float = 0.5',
                      output['client/python/zipzi/_generated/sampling_values.py'])
        self.assertIn('min_triangle_area: float=0.5',
                      output['client/python/zipzi/_generated/catalog.py'])

    def test_rust_field_change_updates_python_wire_and_native_shapes_together(self):
        contract = self.contract()
        def change(node):
            if isinstance(node, dict):
                if node.get('kind') == 'record' and node.get('name', '').endswith('::MeshGeometry'):
                    index = next((i for i, f in enumerate(node['fields']) if f.get('default')), len(node['fields']))
                    node['fields'].insert(index, dict(name='segments', shape=dict(kind='u32'), default=None,
                                                     doc='Number of generated geometry segments.'))
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
                + 'zipzi-contract={path=' + json.dumps(str(ROOT / 'crates/zipzi-contract')) + '}\n'
                + 'zipzi-runtime={path=' + json.dumps(str(ROOT / 'crates/zipzi-runtime')) + '}\n'
                + 'zipzi-api-derive={path=' + json.dumps(str(ROOT / 'crates/zipzi-api-derive')) + '}\n'
                + 'serde={version="1",features=["derive"]}\n')
            cases = (
                ('use zipzi_runtime::authoring::*; fn main() { let _ = MESH_QUERY.shapes(); }', None),
                ('use zipzi_runtime::authoring::*; fn main() { let _ = (IMAGE_QUERY.shapes(), PLAN_QUERY.shapes(), PREPARED_PIXELS_QUERY.shapes()); }', None),
                ('use zipzi_runtime::{authoring::*,*}; fn wrong(_: &mut Runtime, _: ClientId, _: PlanQuery) -> Result<ImagePixels> { todo!() } fn main() { let _ = Registered::<PlanQuery,PlanSnapshot>::bind(zipzi_contract::operations::PLAN_QUERY, wrong); }', 'E0308'),
                ('use zipzi_runtime::authoring::*; fn main() { let _ = Registered::<MeshQuery,MeshSnapshot>::bind(zipzi_contract::operations::MESH_QUERY); }', 'E0061'),
                ('use zipzi_runtime::{authoring::*,*}; fn wrong(_: &mut Runtime, _: ClientId, _: MeshQuery) -> Result<()> { Ok(()) } fn main() { let _ = Registered::<MeshQuery,MeshSnapshot>::bind(zipzi_contract::operations::MESH_QUERY, wrong); }', 'E0308'),
                ('const _: () = zipzi_contract::operations::assert_unique(&["same", "same"]); fn main() {}', 'E0080'),
                ('macro_rules! handler { (MESH_QUERY) => {} } macro_rules! bindings { ($($id:ident: $input:ty => $output:ty = $name:literal;)*) => { $(handler!($id);)* }; } zipzi_contract::authoring_operations!(bindings); fn main() {}', 'macro'),
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
                    cwd=ROOT, env=dict(os.environ, CARGO_TARGET_DIR=os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'target'))), capture_output=True, text=True)
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
                    node['fields'].append(dict(name='label', shape=dict(kind='string'), default=None,
                                              doc='User-facing label for this image creation.'))
                    node['fields'].append(dict(name='description', shape=dict(kind='string'), default=dict(kind='empty_string'),
                                              doc='Optional descriptive text; the empty string supplies no description.'))
                if node.get('kind') == 'record' and node.get('name', '').endswith('::ConversionCost'):
                    node['fields'].append(dict(name='operations', shape=dict(kind='u64'), default=None,
                                              doc='Estimated number of native operations in the conversion.'))
                for value in node.values():
                    change(value)
            elif isinstance(node, list):
                for value in node:
                    change(value)
        change(contract)
        output = gen.render(contract)
        self.assertIn('label: str', output['client/python/zipzi/_generated/resource_ops.py'])
        self.assertIn('label=op.label', output['client/python/zipzi/_generated/resource_ops.py'])
        self.assertIn('label: str', output['client/python/zipzi/_generated/facades.py'])
        self.assertIn('op.getattr("label")', output['src/generated/resources.rs'])
        self.assertIn('input_budget::text(&op.getattr("label")?)?', output['src/generated/resources.rs'])
        self.assertIn('measure_planintent(intent)?', output['src/generated/resource_queries.rs'])
        self.assertIn('operations: int', output['client/python/zipzi/_generated/plan_values.py'])
        self.assertIn('value.operations', output['src/generated/resources.rs'])
        self.assertIn('description: str = ""', output['client/python/zipzi/_generated/resource_ops.py'])
        convenience = output['client/python/zipzi/_generated/facades.py']
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
