"""Independent codegen acceptance: determinism, protected sources and Rust typing."""
from copy import deepcopy
import ast
import importlib.util
import inspect
import json
import os
import shutil
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
from codegen import projections

FIXTURE_SPEC = importlib.util.spec_from_file_location('python2_shape_fixture', ROOT / 'tools/tests/_python2_shape_fixture.py')
fixture = importlib.util.module_from_spec(FIXTURE_SPEC)
FIXTURE_SPEC.loader.exec_module(fixture)

def render_fixture(contract):
    if contract.get('format_version') == 2:
        return gen.render(contract)
    root, definitions = fixture.normalize(contract, projections.EXTERNAL_TYPES)
    return gen.render({'format_version': 2, 'root': root, 'definitions': definitions})


def catalog_from_output(output):
    import importlib
    import sys
    with tempfile.TemporaryDirectory() as folder:
        root=Path(folder)
        package=root/'test_generated_catalog'
        package.mkdir()
        for path, source in output.items():
            prefix='client/python/zipzi/_generated/catalog/'
            if path.startswith(prefix):
                destination=package/path[len(prefix):]
                destination.parent.mkdir(parents=True,exist_ok=True)
                destination.write_text(source,encoding='utf-8')
        try:
            with patch.object(sys,'path',[folder,*sys.path]):
                directory=importlib.import_module('test_generated_catalog')
                return {path:directory.lookup(path) for path in directory.iter_paths()}
        finally:
            for name in list(sys.modules):
                if name=='test_generated_catalog' or name.startswith('test_generated_catalog.'):
                    del sys.modules[name]


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
    def test_mesh_reference_has_public_signatures_context_and_definitions(self):
        output = render_fixture(self.contract())
        catalog = catalog_from_output(output)
        self.assertEqual(catalog['zipzi.Session.id']['kind'], 'property')
        self.assertEqual(catalog['zipzi.Session.id']['signature'], ': SessionId')
        self.assertEqual(catalog['zipzi.Session.project']['signature'], 'session.project')
        self.assertEqual(catalog['zipzi.Session.project']['kind'], 'property')
        for owner in ('Session', 'Edit'):
            entry = catalog['zipzi.' + owner + '.meshes']
            self.assertEqual(entry['kind'], 'property')
            self.assertNotIn('session:', entry['signature'])
        self.assertIn('committed Session', catalog['zipzi.Session.meshes.get']['documentation'])
        self.assertIn('pending writes', catalog['zipzi.Edit.meshes.get']['documentation'])
        self.assertEqual(catalog['zipzi.meshes.Point']['signature'], 'tuple[float, float]')
        self.assertEqual(catalog['zipzi.meshes.ConstraintKind']['kind'], 'enum')
        self.assertTrue(catalog['zipzi.meshes.ConstraintKind']['fields'])
        self.assertNotIn('kind:', catalog['zipzi.meshes.Create']['signature'])
        self.assertEqual(catalog['zipzi.meshes.Rename']['signature'],
                         '(id: str, name: str, *, expected_name: str | None = None)')
        for name in ('Geometry', 'Mesh', 'MeshSpec', 'Create', 'Rename', 'SetPositions'):
            entry = catalog['zipzi.meshes.' + name]
            self.assertNotIn('Immutable owned', entry['documentation'])
        page = output['client/python/zipzi/docs/reference/runtime_session.md']
        section = page.split('## zipzi.Session.meshes.get\n', 1)[1].split('\n## ', 1)[0]
        self.assertNotIn('Alias of', section)
        self.assertIn('[Mesh]', section)
        self.assertEqual(page.count('## Type references'),1)
        self.assertIn('[Mesh]: meshes.md#zipzimeshesmesh',page)
        self.assertIn('zipzi.meshes.Mesh',catalog['zipzi.Session.meshes.get']['type_refs'])
        self.assertIn('zipzi.Snapshot',catalog['zipzi.Session.meshes.get']['type_refs'])
        for name in ('create','rename','set_positions'):
            documentation=catalog['zipzi.Edit.meshes.'+name]['documentation']
            self.assertIn('OutcomeUnknown', documentation)
            self.assertIn('abort the candidate', documentation)
            self.assertNotIn('(...)', documentation)

    def test_reference_types_resolve_import_aliases_and_forward_annotations(self):
        original=gen._api_codegen.render_api
        def renamed(*args,**kwargs):
            import re
            output=original(*args,**kwargs)
            path='client/python/zipzi/_generated/facades.py'
            source=output[path].replace('import Source,','import Source as CapturedSource,')
            source=re.sub(r'expected: Source \| None', "expected: 'CapturedSource | None'",source)
            output[path]=source
            return output
        with patch.object(gen._api_codegen,'render_api',side_effect=renamed):
            output=render_fixture(self.contract())
        catalog=catalog_from_output(output)
        refs=catalog['zipzi.Session.meshes.get']['type_refs']
        self.assertIn('zipzi.Source',refs)
        self.assertIn('zipzi.Snapshot',refs)
        self.assertIn('zipzi.meshes.Mesh',refs)
        for row in catalog.values():
            for ref in row.get('type_refs',()):self.assertIn(ref,catalog)

    def test_reference_reports_missing_type_semantics_without_boilerplate(self):
        contract=self.contract()
        def remove(node):
            if isinstance(node,dict):
                if node.get('name','').endswith('::MeshGeometry'):node.pop('doc',None)
                for child in node.values():remove(child)
            elif isinstance(node,list):
                for child in node:remove(child)
        remove(contract)
        output=render_fixture(contract)
        report=json.loads(output['client/python/zipzi/docs/type-documentation.json'])
        gap=next(item for item in report['missing'] if 'zipzi.meshes.Geometry' in item['paths'])
        self.assertEqual(gap['source'],'rust:zipzi_contract::authoring::MeshGeometry')
        catalog=catalog_from_output(output)
        self.assertEqual(catalog['zipzi.meshes.Geometry']['documentation'],'')
        self.assertTrue(catalog['zipzi.meshes.Geometry']['fields'])
        self.assertIn('Constructors normalize declared collections',output['client/python/zipzi/docs/reference/common.md'])
        for path,text in output.items():
            if path.endswith(('.py','.md')):
                self.assertNotIn('Construction owns declared collections;',text,path)
                self.assertNotIn('Immutable typed value. See field semantics',text,path)
                self.assertNotIn('Typed public API entry.',text,path)
        with self.assertRaisesRegex(ValueError,'Missing type documentation:'):
            gen.check_type_documentation(output)

    def test_reference_strict_documentation_gate_accepts_complete_coverage(self):
        report={'type_count':1,'documented_type_count':1,'missing_type_count':0,'missing':[]}
        self.assertIsNone(gen.check_type_documentation({'client/python/zipzi/docs/type-documentation.json':json.dumps(report)}))

    def test_reference_strict_documentation_cli_checks_without_writing(self):
        report={'type_count':1,'documented_type_count':0,'missing_type_count':1,
                'missing':[{'paths':['zipzi.Example'],'source':'rust:example::Example'}]}
        output={'client/python/zipzi/docs/type-documentation.json':json.dumps(report)}
        with patch.object(gen.sys,'argv',['generate.py','--check-docs']), \
             patch.object(gen,'export',return_value={}), \
             patch.object(gen,'cached_render',return_value=output), \
             patch.object(gen,'write_outputs',return_value=[]) as write:
            with self.assertRaisesRegex(SystemExit,'Missing type documentation: 1 types'):
                gen.main()
        self.assertTrue(write.call_args.args[1])

    def test_reference_semantics_follow_contract_type_variant_and_member_docs(self):
        contract=self.contract()
        def mutate(node):
            if isinstance(node,dict):
                name=node.get('name','')
                if name.endswith('::MeshGeometry'):node['doc']='Updated mesh geometry meaning.'
                if name.endswith('::ManualMeshOperation'):node['doc']='Updated mesh operation union meaning.'
                if name.endswith('::MeshOperation'):
                    next(v for v in node['variants'] if v['rust_name']=='Rename')['doc']='Updated rename meaning.'
                if name.endswith('::ParameterOperation'):
                    next(v for v in node['variants'] if v['rust_name']=='Patch')['doc']='Updated partial parameter replacement meaning.'
                if name.endswith('::MeshConstraintKind'):
                    next(v for v in node['variants'] if v['rust_name']=='User')['doc']='Updated constraint meaning.'
                for child in node.values():mutate(child)
            elif isinstance(node,list):
                for child in node:mutate(child)
        mutate(contract)
        output=render_fixture(contract)
        catalog=catalog_from_output(output)
        cases=(('zipzi.meshes.Geometry','Updated mesh geometry meaning.','meshes.py'),
               ('zipzi.meshes.Rename','Updated rename meaning.','mesh_ops.py'),
               ('zipzi.Edit.meshes.rename','Updated rename meaning.','facades.py'))
        for path,meaning,source in cases:
            self.assertIn(meaning,catalog[path]['documentation'])
            self.assertIn(meaning,output['client/python/zipzi/_generated/'+source])
            self.assertIn(meaning,output['client/python/zipzi/docs/reference/'+('runtime_edit' if 'Edit.' in path else 'meshes')+'.md'])
        self.assertIn('Updated mesh operation union meaning.',catalog['zipzi.modeling.mesh.ManualMeshOperation']['documentation'])
        self.assertIn('Updated mesh operation union meaning.',output['client/python/zipzi/docs/reference/modeling_mesh.md'])
        self.assertIn('Updated constraint meaning.',str(catalog['zipzi.meshes.ConstraintKind']['fields']))
        self.assertIn('Updated constraint meaning.',output['client/python/zipzi/docs/reference/modeling_mesh.md'])
        for path in ('zipzi.parameters.Patch','zipzi.Edit.parameters.patch'):
            self.assertIn('Updated partial parameter replacement meaning.',catalog[path]['documentation'])
        self.assertIn('Updated partial parameter replacement meaning.',output['client/python/zipzi/_generated/parameter_values.py'])
        self.assertIn('Updated partial parameter replacement meaning.',output['client/python/zipzi/_generated/facades.py'])
        self.assertIn('Updated partial parameter replacement meaning.',output['client/python/zipzi/docs/reference/parameters.md'])
        self.assertNotIn('Updated partial parameter replacement meaning.',catalog['zipzi.Edit.meshes.rename']['documentation'])

    def test_reference_rejects_missing_example_target(self):
        from dataclasses import replace
        original=gen._api_codegen.render_api
        def explicit_example(*args,**kwargs):
            output=original(*args,**kwargs)
            bindings=kwargs['documentation']
            key='python:zipzi._generated.facades.Meshes.get'
            bindings[key]=replace(bindings[key],example='docs/missing-example.py')
            return output
        with patch.object(gen._api_codegen,'render_api',side_effect=explicit_example):
            with self.assertRaisesRegex(ValueError, 'Missing API example: docs/missing-example.py'):
                render_fixture(self.contract())

    def test_reference_links_and_fragments_resolve(self):
        import re
        import posixpath
        output = render_fixture(self.contract())
        prefix = 'client/python/zipzi/docs/'
        documents = {path: text for path, text in output.items()
                     if path.startswith(prefix) and path.endswith('.md')}
        for path in (PACKAGE / prefix).rglob('*.md'):
            documents.setdefault(path.relative_to(PACKAGE).as_posix(), path.read_text(encoding='utf-8'))
        for path, text in documents.items():
            if '/reference/' not in path and not path.endswith('reference.md'):
                continue
            for target in re.findall(r'\[[^\]]+\]\(([^)]+)\)', text)+re.findall(r'^\[[^\]]+\]: (\S+)$',text,re.MULTILINE):
                file, _, fragment = target.partition('#')
                if ':' in file:continue
                destination = posixpath.normpath(posixpath.join(posixpath.dirname(path), file)) if file else path
                self.assertIn(destination, documents, (path, target))
                if fragment:
                    headings = re.findall(r'^#+ (.+)$', documents[destination], re.MULTILINE)
                    anchors = {re.sub(r'[^\w -]', '', heading.lower()).replace(' ', '-') for heading in headings}
                    self.assertIn(fragment, anchors, (path, target))

    def test_generated_files_keep_lf_in_windows_checkout(self):
        manifest = json.loads((PACKAGE / 'generated-manifest.json').read_text(encoding='utf-8'))
        paths = ['crates/zipzi-python2/' + entry['path'] for entry in manifest['files']]
        paths.append('crates/zipzi-python2/generated-manifest.json')
        attributes = subprocess.check_output(
            ['git', 'check-attr', '--stdin', '-z', 'eol'], cwd=ROOT,
            input=('\0'.join(paths) + '\0').encode()).split(b'\0')
        values = dict(zip(attributes[0::3], attributes[2::3]))
        self.assertEqual({path for path in paths if values[path.encode()] != b'lf'}, set())

        generated = 'crates/zipzi-python2/client/python/zipzi/_generated/__init__.py'
        with tempfile.TemporaryDirectory() as folder:
            subprocess.run(['git', '-c', 'core.autocrlf=true', 'checkout-index',
                            '--prefix', folder + '/', '--', generated], cwd=ROOT, check=True)
            data = (Path(folder) / generated).read_bytes()
        self.assertTrue(gen._outputs.marked(generated, data))
        self.assertNotIn(b'\r\n', data)

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

        def render_under_lock(*args, **kwargs):
            require_lock()
            return {'client/python/zipzi/docs/type-documentation.json':json.dumps(
                {'type_count':0,'documented_type_count':0,'missing_type_count':0})}

        for check in (False, True):
            args = ['generate.py'] + (['--check'] if check else [])
            with patch.object(gen.sys, 'argv', args), patch.object(gen, 'file_lock', lock), \
                 patch.object(gen, 'export', side_effect=require_lock), \
                 patch.object(gen, 'cached_render', side_effect=render_under_lock), \
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
            (client / '_generated').mkdir()
            (client / '_generated/old.py').write_text('stale output')
            (root / 'client/pyproject.toml').write_text('version')
            cache = root / 'cache.json'
            output = {path: 'generated' for path in gen.TARGETS}
            with patch.object(gen, 'PACKAGE', root), patch.object(gen.subprocess, 'check_output', return_value='rustfmt 1'), patch.object(gen, 'render', return_value=output) as render:
                contract = {'version': 1}
                self.assertEqual(gen.cached_render(contract, cache), output)
                self.assertEqual(gen.cached_render(contract, cache), output)
                self.assertEqual(render.call_count, 1)
                (client / '_generated/old.py').write_text('changed stale output')
                gen.cached_render(contract, cache)
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
        output=render_fixture(self.contract())
        catalog=catalog_from_output(output)
        for path in catalog:
            parent=path.rpartition('.')[0]
            if parent in catalog:
                self.assertIn(path,catalog[parent]['children'])
            row=catalog[path]
            if row.get('fields'):
                self.assertNotIn('Fields:',row['documentation'],path)
                self.assertTrue(all(name and doc for name,doc in row['fields']),path)
        for path in ('Runtime.sessions','Runtime.resources','Session.project'):
            self.assertTrue(catalog['zipzi.'+path]['children'])
        signature=catalog['zipzi.modeling.ModelingQuery']['signature']
        self.assertNotIn('field(',signature)
        self.assertIn('assets: tuple[str, ...] = ()',signature)
        self.assertIn('physics: bool = False',signature)
        self.assertIn('`'+signature+'`',output['client/python/zipzi/docs/reference/modeling_queries.md'])
        self.assertIn('modeling_queries.md#zipzimodelingqueriesmodelingquery',
                      output['client/python/zipzi/docs/reference/modeling.md'])
        self.assertTrue(signature.startswith('(*, '),signature)
        parameter=catalog['zipzi.modeling.parameters.Parameter']['signature']
        self.assertIn('kind: ParameterKind = ParameterKind.NORMAL',parameter)
        self.assertNotIn('read_',parameter)
        self.assertNotIn('field(',parameter)
        self.assertIn('zipzi.mesh_generation',catalog['zipzi']['children'])
        self.assertIn('zipzi.mesh_generation.MeshSettings',catalog['zipzi.mesh_generation']['children'])

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
            self.assertEqual(full.count('field: '+help.fields[0][1]),1)
            query=modeling.ModelingQuery()
            self.assertEqual(query.assets,())
            self.assertIs(query.physics,False)
            self.assertIs(query.pose,False)

    def test_modeling_freezes_only_values_requiring_conversion(self):
        output=render_fixture(self.contract())
        for path,source in output.items():
            if not path.startswith('client/python/zipzi/_generated/values/') or not path.endswith('.py'):continue
            for node in ast.walk(ast.parse(source)):
                if not isinstance(node,ast.Call) or len(node.args)!=3:continue
                func,obj,name,value=node.func,*node.args
                if (isinstance(func,ast.Attribute) and isinstance(func.value,ast.Name)
                        and func.value.id=='object' and func.attr=='__setattr__'
                        and isinstance(obj,ast.Name) and obj.id=='self'
                        and isinstance(name,ast.Constant) and isinstance(value,ast.Attribute)
                        and isinstance(value.value,ast.Name) and value.value.id=='self'):
                    self.assertNotEqual(name.value,value.attr)

    def test_private_codecs_have_domain_imports_without_duplicate_export_layers(self):
        output=render_fixture(self.contract())
        root=ast.parse(output['client/python/zipzi/_generated/codecs/__init__.py'])
        imports=[n for n in root.body if isinstance(n,(ast.Import,ast.ImportFrom))]
        self.assertTrue(imports)
        for node in imports:
            self.assertIsInstance(node,ast.ImportFrom)
            self.assertIsNone(node.module)
            for alias in node.names:
                self.assertIn('client/python/zipzi/_generated/codecs/'+alias.name+'.py',output)
        for path,source in output.items():
            if not path.endswith('.py') or path.endswith('/__init__.py'):continue
            if not any('/_generated/'+domain+'/' in path for domain in ('codecs','values','binary')):continue
            for node in ast.walk(ast.parse(source)):
                if not isinstance(node,(ast.Import,ast.ImportFrom)):continue
                for alias in node.names:
                    self.assertNotEqual(alias.name,alias.asname,(path,node.lineno))

    def test_flattened_physics_extensions_roundtrip_and_cannot_shadow_fields(self):
        import sys
        with patch.object(sys, 'path', [str(PACKAGE / 'client/python'), *sys.path]):
            from zipzi._generated import values
            from zipzi._generated.codecs import physics as codec
            domain = values.PhysicsRange(Minimum=-1., Default=0., Maximum=1.)
            nested = {'enabled': True, 'levels': [1, 2]}
            value = values.PhysicsNormalization(Position=domain, Angle=domain,
                extensions={'VendorNormalization': nested})
            nested['levels'].append(3)
            wire = codec.encode_PhysicsNormalization(value)
            self.assertEqual(wire, {'Position': {'Minimum': -1., 'Default': 0., 'Maximum': 1.},
                'Angle': {'Minimum': -1., 'Default': 0., 'Maximum': 1.},
                'VendorNormalization': {'enabled': True, 'levels': [1, 2]}})
            self.assertEqual(codec.read_PhysicsNormalization(wire), value)
            codec.validate_PhysicsNormalization(value)
            shadow = values.PhysicsNormalization(Position=domain, Angle=domain,
                extensions={'Position': 42})
            with self.assertRaisesRegex(ValueError, 'overlap declared fields'):
                codec.validate_PhysicsNormalization(shadow)

    def test_single_variant_type_alias_keeps_fields_and_public_methods_in_catalog(self):
        output=render_fixture(self.contract())
        catalog=catalog_from_output(output)
        alias='zipzi.modeling.physics.PhysicsAssetProperty'
        concrete='zipzi.modeling.physics.PhysicsAssetPropertyFps'
        self.assertEqual(catalog[alias]['kind'],'type')
        self.assertEqual(catalog[alias]['fields'],catalog[concrete]['fields'])
        self.assertTrue(dict(catalog[alias]['fields'])['value'])
        for method in ('summary','__repr__'):
            self.assertEqual(catalog[alias+'.'+method]['kind'],'callable')
            self.assertEqual(catalog[alias+'.'+method]['documentation'],
                             catalog[concrete+'.'+method]['documentation'])

    def test_catalog_version_follows_distribution_metadata(self):
        output=render_fixture(self.contract())
        namespace={}
        exec(output['client/python/zipzi/_generated/metadata.py'],namespace)
        import tomllib
        version=tomllib.loads((PACKAGE/'client/pyproject.toml').read_text())['project']['version']
        self.assertEqual(namespace['PACKAGE_VERSION'],version)
        self.assertIn('Package '+version+';',catalog_from_output(output)['zipzi']['documentation'])
        self.assertEqual(namespace['CONTRACT'],self.contract()['protocol'])
        changed_protocol=deepcopy(self.contract());changed_protocol['protocol']='test-runtime-contract'
        protocol_output=render_fixture(changed_protocol)
        exec(protocol_output['client/python/zipzi/_generated/metadata.py'],namespace)
        self.assertEqual(namespace['CONTRACT'],'test-runtime-contract')
        self.assertIn('protocol test-runtime-contract;',catalog_from_output(protocol_output)['zipzi']['documentation'])
        with patch.object(gen._api_codegen._python_api.tomllib,'loads',return_value={'project':{'version':'9.8.7a6'}}):
            changed=render_fixture(self.contract())
        exec(changed['client/python/zipzi/_generated/metadata.py'],namespace)
        self.assertEqual(namespace['PACKAGE_VERSION'],'9.8.7a6')
        self.assertIn('Package 9.8.7a6;',changed['client/python/zipzi/docs/reference/runtime.md'])

    def test_handwritten_context_and_representation_methods_have_public_entries(self):
        output=render_fixture(self.contract())
        catalog=catalog_from_output(output)
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
            render_fixture(contract)

    def test_field_semantics_are_shared_by_docstrings_catalog_and_reference(self):
        output=render_fixture(self.contract())
        catalog=catalog_from_output(output)
        examples={
            'zipzi.parameters.ParameterPatch':('repeat','None preserves the current value'),
            'zipzi.bindings.Keyform':('keys','axis order'),
            'zipzi.resources.ImagePixels':('rgba','straight alpha'),
            'zipzi.Source':('version','formal base version'),
            'zipzi.Snapshot':('source','captured atomically'),
            'zipzi.scene.VertexPatch':('position','BlendShape XY displacement'),
            'zipzi.modeling.project.CdiNamespaceIds':('parameters','document-ID to runtime-ID map'),
            'zipzi.modeling.bindings.BindingGridEditEdit':('operation','Typed axis/sample grid edit'),
            'zipzi.modeling.bindings.DeltaOffscreenKeyform':('multiply','neutral (1, 1, 1)'),
            'zipzi.modeling.common.Vec2':('x','Horizontal coordinate'),
            'zipzi.modeling.common.PreciseVec2':('y','Vertical coordinate'),
            'zipzi.mesh_generation.MeshSettings':('alpha_threshold','Alpha cutoff'),
        }
        reference='\n'.join(value for path,value in output.items() if path.startswith('client/python/zipzi/docs/reference/'))
        import sys
        with patch.object(sys,'path',[str(PACKAGE/'client/python'),*sys.path]):
            import zipzi
        for path,(field,meaning) in examples.items():
            with self.subTest(path=path,field=field):
                entry=catalog[path]
                self.assertIn(meaning,' '.join(dict(entry['fields'])[field].split()))
                self.assertNotIn('Fields:',entry['documentation'])
                self.assertIn(meaning,' '.join(reference.split()))
                value=zipzi
                for part in path.split('.')[1:]:value=getattr(value,part)
                self.assertIn(meaning,' '.join((inspect.getdoc(value) or '').split()))
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
        output=render_fixture(contract)
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
        output = render_fixture(contract)
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
                render_fixture(invalid)
    def test_binary_contract_guards_compiled_source_layout_and_typed_array_outputs(self):
        contract=self.contract()
        output=render_fixture(contract)
        binary='\n'.join(value for path,value in output.items() if path.startswith('client/python/zipzi/_generated/binary/') and path.endswith('.py'))
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
            with self.assertRaises(ValueError):render_fixture(invalid)

    def contract(self):
        logical = gen.schema.read(PACKAGE / 'schema/authoring.json')
        return fixture.expand(logical['root'], logical['definitions'])

    def files(self, folder):
        return {p.relative_to(folder).as_posix(): p.read_bytes() for p in folder.rglob('*') if p.is_file()}

    def test_compiled_contract_matches_committed_outputs_without_writing(self):
        contract = gen.export()
        output = render_fixture(contract)
        self.assertEqual(gen.write_outputs(output, check=True), [])
        self.assertEqual(output, render_fixture(self.contract()))

    def test_generation_is_deterministic_and_check_only_never_repairs_drift(self):
        output = render_fixture(self.contract())
        self.assertEqual(output, render_fixture(self.contract()))
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

    def test_render_ignores_stale_generated_python_in_checkout(self):
        contract=self.contract()
        expected=render_fixture(contract)
        with tempfile.TemporaryDirectory() as folder:
            package=Path(folder)
            shutil.copytree(PACKAGE/'client',package/'client')
            stale=package/'client/python/zipzi/_generated/modeling.py'
            stale.write_text('this is not valid Python source !!!\n')
            old=package/'client/python/zipzi/_generated/obsolete.py'
            old.write_text('also not Python !!!\n')
            with patch.object(gen,'PACKAGE',package):
                self.assertEqual(render_fixture(contract),expected)

    def test_renderers_consume_graph_without_legacy_expansion(self):
        with patch.object(fixture,'expand',side_effect=AssertionError('legacy expansion')):
            output=render_fixture(gen.schema.read(PACKAGE / 'schema/authoring.json'))
        self.assertEqual(gen.write_outputs(output,check=True),[])

    def test_projection_model_records_canonical_alias_and_special_reasons(self):
        original=gen._api_codegen.document_model
        projected=[]
        entries=[]
        def capture(*args):
            model=original(*args)
            projected.extend(model[-1])
            entries.extend(model[0])
            return model
        with patch.object(gen._api_codegen,'document_model',side_effect=capture):
            output=render_fixture(self.contract())
        paths={entry.path for entry in entries}
        graph=gen.TypeGraph.from_contract(gen.schema.read(PACKAGE/'schema/authoring.json'))
        for item in projected:
            with self.subTest(path=item.canonical_path):
                self.assertIn(item.rust_type_id,graph.definitions)
                self.assertIn(item.canonical_path,paths)
                source=output['client/python/'+item.implementation_module.replace('.','/')+'.py']
                symbols={node.name for node in ast.parse(source).body if isinstance(node,ast.ClassDef)}
                symbols.update(node.target.id for node in ast.parse(source).body
                               if isinstance(node,ast.AnnAssign) and isinstance(node.target,ast.Name))
                self.assertIn(item.canonical_path.rsplit('.',1)[-1],symbols)
        canonical=next(item for item in projected if item.rust_type_id=='zipzi_contract::model::MeshConstraintKind' and item.projection_id=='canonical')
        self.assertIn('zipzi.meshes.ConstraintKind',canonical.aliases)
        shard=ast.parse(output['client/python/zipzi/_generated/catalog/meshes.py'])
        data=next(ast.literal_eval(node.value) for node in shard.body
                  if isinstance(node,ast.AnnAssign) and node.target.id=='DATA')
        self.assertEqual(data['zipzi.meshes.ConstraintKind'],{'alias':canonical.canonical_path})
        alias_page=output['client/python/zipzi/docs/reference/meshes.md']
        self.assertIn('modeling_mesh.md#zipzimodelingmeshmeshconstraintkind',alias_page)
        special=[item for item in projected if item.projection_id!='canonical']
        self.assertTrue(special)
        self.assertTrue(all(item.reason for item in special))

    def test_handwritten_files_unowned_paths_and_symlinks_fail_before_any_write(self):
        output = render_fixture(self.contract())
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
                render_fixture(contract)

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
        def change(value):
            if isinstance(value,dict):
                if value.get('name') == checks['name'] and value.get('kind') in ('record','serde_record'):
                    next(f for f in value['fields'] if f['name'] == 'min_triangle_area')['default'] = 0.5
                for item in value.values():change(item)
            elif isinstance(value,list):
                for item in value:change(item)
        change(contract)
        output = render_fixture(contract)
        self.assertIn('min_triangle_area: float = 0.5',
                      output['client/python/zipzi/_generated/sampling_values.py'])
        self.assertIn('min_triangle_area: float=0.5',
                      output['client/python/zipzi/_generated/catalog/sampling.py'])

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
        output = render_fixture(contract)
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
        output = render_fixture(contract)
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
                render_fixture(contract)

class PythonReference(unittest.TestCase):
    # Pages only enter this set after full page acceptance. Cross-page pilots
    # are exact paths; accepting modeling does not accept modeling.* domains.
    ACCEPTED_PAGES = frozenset({'meshes'})
    # P1 has no structural gaps; cross-domain semantic acceptance remains open.
    STRUCTURAL_PAGES = frozenset('runtime runtime_session runtime_edit modeling modeling_common modeling_runtime ops modeling_commands modeling_queries events'.split())
    BATCH_PAGES = {
        'S': 'meshes',
        'P1': 'runtime runtime_session runtime_edit modeling modeling_common modeling_runtime ops modeling_commands modeling_queries events',
        'P2': 'parameters bindings modeling_parameters modeling_bindings modeling_bindings_edits',
        'P3': 'resources scene modeling_resources modeling_scene modeling_scene_edits modeling_mesh',
        'P4': 'references modeling_addressing modeling_addressing_kinds modeling_addressing_patches modeling_addressing_selectors modeling_deletion modeling_deletion_decisions',
        'P5': 'modeling_project',
        'P6': 'animation sampling modeling_motion modeling_expression modeling_physics modeling_pose modeling_preview modeling_sampling',
        'P7': 'spatial observation inspection mesh_generation modeling_observation modeling_spatial',
    }
    PILOT_PATHS = frozenset({
        'zipzi.ApiDescription', 'zipzi.ApiDescription.format',
        'zipzi.Session.meshes', 'zipzi.Edit.meshes',
        *(f'zipzi.{owner}.meshes.{method}' for owner in ('Session','Edit') for method in ('get','get_many','list')),
        *(f'zipzi.Edit.meshes.{method}' for method in ('create','rename','set_positions')),
        'zipzi.Edit.parameters.patch', 'zipzi.parameters.ParameterPatch', 'zipzi.parameters.Patch',
        'zipzi.modeling.mesh.MeshConstraintKind', 'zipzi.meshes.ConstraintKind',
        'zipzi.modeling.spatial.ObjectTarget', 'zipzi.modeling.spatial.ObjectTargetMesh',
        'zipzi.modeling.spatial.ObjectTargetPart', 'zipzi.spatial.Target',
    })

    @classmethod
    def setUpClass(cls):
        original=gen._api_codegen.document_model
        def capture(*args,**kwargs):
            cls.model=original(*args,**kwargs)
            return cls.model
        with patch.object(gen._api_codegen,'document_model',side_effect=capture):
            cls.output=gen.render(gen.schema.read(PACKAGE/'schema/authoring.json'))
        cls.catalog=catalog_from_output(cls.output)
        cls.coverage=json.loads(cls.output['client/python/zipzi/docs/type-documentation.json'])
        cls.quality=json.loads(cls.output['client/python/zipzi/docs/documentation-quality.json'])

    def test_reference_scoped_coverage_and_stable_debt(self):
        from codegen.documentation_audit import scoped_missing, debt_keys, validate_debt
        self.assertTrue(self.PILOT_PATHS<=self.catalog.keys(),self.PILOT_PATHS-self.catalog.keys())
        self.assertFalse(scoped_missing(self.coverage,self.quality,self.ACCEPTED_PAGES | self.STRUCTURAL_PAGES,self.PILOT_PATHS))
        baseline=json.loads((ROOT/'tools/python/python-reference-debt.json').read_text())
        validate_debt(debt_keys(self.coverage,self.quality),{tuple(key) for key in baseline['debt']})
        current={('type-summary','rust:example::Old|projection:canonical','')}
        with self.assertRaisesRegex(ValueError,'New documentation debt'):
            validate_debt({('type-summary','rust:example::New|projection:canonical','')},current)
        with self.assertRaisesRegex(ValueError,'Remove resolved'):
            validate_debt(set(),current)
        for item in self.coverage['missing']:
            self.assertIsInstance(item['identity'],str)
            self.assertTrue(item['identity'].startswith(('python:','rust:')))

    def test_reference_routing_uses_exact_pages(self):
        from codegen.reference_routing import page_group
        domains=self.model[1]
        modeling=tuple(path.rsplit('.',1)[-1] for path,row in self.catalog.items()
                       if row['kind']=='module' and path.startswith('zipzi.modeling.') and path.count('.')==2)
        self.assertEqual(page_group('zipzi.modeling',domains,modeling),'modeling')
        self.assertEqual(page_group('zipzi.modeling.mesh.ManualMeshOperation',domains,modeling),'modeling_mesh')
        self.assertEqual(page_group('zipzi.modeling.addressing.AddressPatch',domains,modeling),'modeling_addressing_patches')
        assigned=[page for pages in self.BATCH_PAGES.values() for page in pages.split()]
        self.assertEqual(len(assigned),len(set(assigned)))
        self.assertEqual(set(assigned),set(self.quality['pages']))
        self.assertTrue((self.ACCEPTED_PAGES | self.STRUCTURAL_PAGES)<=set(assigned))
        from codegen.documentation_audit import scoped_missing
        missing=scoped_missing(self.coverage,self.quality,{'modeling'})
        self.assertFalse(any('modeling_mesh' in row.get('pages',()) and row.get('pages')==['modeling_mesh'] for _,row in missing))
        self.assertTrue(set(self.quality['pages'])-self.ACCEPTED_PAGES)  # Future batches remain visible.

    def test_reference_rule_bodies_aliases_and_examples_are_deduplicated(self):
        from codegen.documentation_audit import PLACEHOLDERS,validate_reference_declarations
        rules=self.model[-2]
        pages={path:text for path,text in self.output.items() if '/docs/reference/' in path}
        validate_reference_declarations(pages)
        for rule in rules.values():
            for path,text in pages.items():
                self.assertEqual(text.count(rule.documentation),int(path.endswith('/common.md')),(path,rule.key))
        for row in self.catalog.values():self.assertFalse(row.get('example'))
        for pattern in PLACEHOLDERS:
            self.assertFalse(any(pattern in text for text in pages.values()),pattern)
        for path,source in self.output.items():
            if '/_generated/catalog/' not in path or not path.endswith('.py'):continue
            data=next((ast.literal_eval(n.value) for n in ast.parse(source).body
                       if isinstance(n,ast.AnnAssign) and getattr(n.target,'id','')=='DATA'),{})
            for name,row in data.items():
                if 'alias' not in row or self.catalog[name]['kind'] not in {'type','enum','alias'}:continue
                section=next(text for text in pages.values() if '## '+name+'\n' in text).split('## '+name+'\n',1)[1].split('\n## ',1)[0]
                self.assertIn('Alias of',section,name)
                self.assertNotIn('**Fields**',section,name)
        with self.assertRaisesRegex(ValueError,'Duplicate/ambiguous'):
            validate_reference_declarations({'example.md':'[Mesh]: a.md\n[mesh]: b.md\n'})

    def test_reference_pilot_preserves_effects_fields_and_projection_roles(self):
        for owner in ('Session','Edit'):
            for method,result in (('get','Snapshot'),('get_many','Batch'),('list','Page')):
                row=self.catalog[f'zipzi.{owner}.meshes.{method}']
                self.assertIn(result,row['signature'])
                self.assertIn('committed Session' if owner=='Session' else 'this Edit candidate',row['documentation'])
                self.assertNotIn('session.meshes',row['documentation'])
                self.assertTrue(row['rules'])
        for method,result in (('create','str'),('rename','WriteResult'),('set_positions','WriteResult')):
            row=self.catalog[f'zipzi.Edit.meshes.{method}']
            text=row['documentation']+' '.join(body for _,body in row['rules'])
            for fact in ('candidate','abort the candidate','OutcomeUnknown','Job[Receipt]','recovery_action'):
                self.assertIn(fact,text)
            self.assertTrue(row['signature'].endswith('-> '+result))
        patch=self.catalog['zipzi.parameters.ParameterPatch']
        self.assertIn('None',patch['documentation'])
        self.assertIn('preserve',patch['documentation'])
        self.assertIn('None preserves',dict(patch['fields'])['kind'])
        self.assertNotIn('kind',{name for name,_,_ in patch.get('field_roles',())})
        create=self.catalog['zipzi.meshes.Create']
        self.assertEqual(create['field_roles'],(('kind','fixed-discriminator',"'create'"),))
        self.assertNotIn('kind:',create['signature'])
        self.assertIn('fixed-discriminator',create['rule_refs'])
        self.assertIn('persistent',dict(self.catalog['zipzi.modeling.spatial.ObjectTargetMesh']['fields'])['value'].lower())

    def test_reference_runtime_query_and_event_sources(self):
        # Contract variant descriptions reach both the projected docstring and
        # its catalog/reference consumers; the event remains a distinct type.
        event = self.catalog['zipzi.events.Published']
        self.assertIn('candidate was published', event['documentation'])
        self.assertIn('candidate was published', self.output['client/python/zipzi/_generated/event_values.py'])
        self.assertIn('candidate was published', self.output['client/python/zipzi/docs/reference/events.md'])
        query = self.catalog['zipzi.modeling.queries.CollectionQueryExpressionEntries']
        self.assertIn('expression asset', query['documentation'])
        command = self.catalog['zipzi.modeling.commands.ModelingCommandManualMesh']
        self.assertIn('source-checked', command['documentation'])

    def test_reference_runtime_enum_and_private_union_navigation(self):
        state = self.catalog['zipzi.EditState']
        members = dict(state['fields'])
        self.assertIn('original owner', members['unknown'])
        self.assertIn('publication', members['committed'])
        self.assertIn('start a new Edit', members['aborted'])
        for owner in ('Session', 'Edit'):
            refs = self.catalog[f'zipzi.{owner}.plan_async']['type_refs']
            self.assertIn('zipzi.modeling.commands.ModelingCommandDeletion', refs)
            self.assertIn('zipzi.modeling.commands.ModelingCommandScene', refs)
        refs = self.catalog['zipzi.ops.PlannableOperation']['type_refs']
        self.assertIn('zipzi.modeling.commands.ModelingCommandDeletion', refs)
        p1 = set(self.BATCH_PAGES['P1'].split())
        self.assertFalse([row for row in self.quality['diagnostics']
                          if p1.intersection(row['pages'])])
        self.assertIn('factory-owned', self.catalog['zipzi.events.SessionSubscription']['signature'])

    def test_reference_missing_branches_and_unknown_sdk_types_are_diagnosed(self):
        contract=gen.schema.read(PACKAGE/'schema/authoring.json')
        contract['definitions']['zipzi_contract::model::MeshConstraintKind']['variants'][0]['doc']=''
        contract['definitions']['zipzi_contract::spatial::ObjectTarget']['variants'][0]['doc']=''
        original=gen._api_codegen.render_api
        def unknown(*args,**kwargs):
            output=original(*args,**kwargs)
            path='client/python/zipzi/_generated/facades.py'
            output[path]=output[path].replace('-> Snapshot[meshes.Mesh]:','-> Snapshot[meshes.UnknownSdkType]:')
            return output
        with patch.object(gen._api_codegen,'render_api',side_effect=unknown):
            output=gen.render(contract)
        quality=json.loads(output['client/python/zipzi/docs/documentation-quality.json'])
        categories={row['category'] for row in quality['diagnostics']
                    if any(path in self.PILOT_PATHS for path in row['paths'])}
        self.assertTrue({'enum-member','union-branch','type-reference'}<=categories)
        self.assertTrue({'builtin','typing','type-variable','public-sdk'}<=self.quality['reference_classifications'].keys())
        self.assertTrue(any(row['status']=='review_required' and row['category']=='callable-return' for row in self.quality['semantic_review']))
        self.assertTrue(any(row['status']=='not_applicable' for row in self.quality['semantic_review']))

    def test_reference_private_union_does_not_hide_unknown_members(self):
        original = gen._api_codegen.render_api
        def unknown(*args, **kwargs):
            output = original(*args, **kwargs)
            path = 'client/python/zipzi/_generated/facades.py'
            source = output[path].replace('-> Snapshot[meshes.Mesh]:', '-> _PrivateResult:')
            output[path] = source + '\n_PrivateResult = meshes.Mesh | meshes.UnknownSdkType\n'
            return output
        with patch.object(gen._api_codegen, 'render_api', side_effect=unknown):
            output = gen.render(gen.schema.read(PACKAGE/'schema/authoring.json'))
        quality = json.loads(output['client/python/zipzi/docs/documentation-quality.json'])
        self.assertTrue(any(row['category'] == 'type-reference' and row['subject'] == 'meshes.UnknownSdkType'
                            for row in quality['diagnostics']))

    def test_reference_rule_and_descriptor_changes_reach_all_consumers(self):
        from dataclasses import replace
        from codegen.model import DocRule,validate_rules
        marker='Changed collection source rule.'
        original_rule=gen._api_codegen.collection_rules
        original_api=gen._api_codegen.render_api
        def changed_rules(package):
            rules=original_rule(package)
            rules['collection-read']=replace(rules['collection-read'],documentation=marker)
            return rules
        def changed_api(*args,**kwargs):
            saved=gen._api_codegen.WRITES['Meshes']
            rows=deepcopy(saved)
            rows[0]=(*rows[0][:-1],'Changed Python create projection.')
            with patch.dict(gen._api_codegen.WRITES,{'Meshes':rows}):
                return original_api(*args,**kwargs)
        with patch.object(gen._api_codegen,'collection_rules',side_effect=changed_rules), \
             patch.object(gen._api_codegen._python_api,'collection_rules',side_effect=changed_rules), \
             patch.object(gen._api_codegen,'render_api',side_effect=changed_api):
            changed=gen.render(gen.schema.read(PACKAGE/'schema/authoring.json'))
        catalog=catalog_from_output(changed)
        self.assertIn(marker,changed['client/python/zipzi/_generated/facades.py'])
        self.assertEqual(catalog['zipzi.Session.meshes.get']['rules'][0][1],marker)
        self.assertIn(marker,changed['client/python/zipzi/docs/reference/common.md'])
        self.assertIn('Changed Python create projection.',catalog['zipzi.Edit.meshes.create']['documentation'])
        self.assertEqual(catalog['zipzi.Edit.parameters.create'],self.catalog['zipzi.Edit.parameters.create'])
        with self.assertRaisesRegex(ValueError,'Missing documentation rule'):
            validate_rules({},('absent',))
        with self.assertRaisesRegex(ValueError,'must be leaves'):
            validate_rules({'cycle':DocRule('cycle','Cycle','Body','python:source',('cycle',))},('cycle',))
        with self.assertRaisesRegex(ValueError,'Duplicate documentation rule'):
            validate_rules(self.model[-2],('collection-read','collection-read'))

    def test_reference_projection_binding_mistakes_fail(self):
        original=gen._api_codegen.render_api
        def wrong(*args,**kwargs):
            output=original(*args,**kwargs)
            bindings=kwargs['documentation']
            bindings['python:zipzi._generated.facades.Parameters.wrong']=bindings.pop('python:zipzi._generated.facades.Meshes.get')
            return output
        with patch.object(gen._api_codegen,'render_api',side_effect=wrong):
            with self.assertRaisesRegex(ValueError,'Unbound documentation projection'):
                gen.render(gen.schema.read(PACKAGE/'schema/authoring.json'))

    def test_reference_offline_format_is_complete_and_loads_only_direct_rules(self):
        code='''import sys
sys.path.insert(0, {package!r})
import zipzi
assert not any(name.startswith('zipzi._generated.catalog.') for name in sys.modules)
entry=zipzi.describe('Edit.meshes.create')
assert entry.rule_refs == ('edit-write',)
text=entry.format()
for fact in ('candidate', 'abort the candidate', 'OutcomeUnknown', 'Job[Receipt]'):
    assert fact in text, fact
assert text.count(entry.rules[0][1]) == 1
assert not entry.example
assert set(name.removeprefix('zipzi._generated.catalog.') for name in sys.modules if name.startswith('zipzi._generated.catalog.')) == {{'index','runtime_edit','rules'}}
assert not any(name.startswith(('zipzi_native','wgpu')) for name in sys.modules)
'''.format(package=str(PACKAGE/'client/python'))
        result=subprocess.run([__import__('sys').executable,'-I','-c',code],capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stderr)


if __name__ == '__main__':
    unittest.main()
