"""P0 API migration cannot change inputs, measurements, policy or frozen clients."""
import ast
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

ROOT=Path(__file__).resolve().parents[2]
SPEC=importlib.util.spec_from_file_location('p0_api_migration',ROOT/'tools/python/python2_p0_migration.py')
migration=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(migration)


def legacy_source():
    tree=ast.parse(migration.WORKLOAD.read_text())
    fixture=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='fixture')
    fixture.body=[n for n in fixture.body if not (isinstance(n,ast.If) and isinstance(n.test,ast.Call) and ast.unparse(n.test.func)=='hasattr')]
    class LegacyCalls(ast.NodeTransformer):
        def visit_Call(self,node):
            self.generic_visit(node);name=ast.unparse(node.func)
            def call(expr,args):return ast.Call(func=ast.parse(expr,mode='eval').body,args=args,keywords=[])
            if name=='plan_method':return call('session.meshes.plan',[call('meshes.RenameIntent',[ast.Name(id='face',ctx=ast.Load()),*node.args[0].args])])
            if name=='apply_plan':return call('edit.apply_plan',node.args[1:])
            if name=='rename_operation':return call('ops.Rename',node.args)
            if name=='domain_value':return node.args[0]
            return node
    return ast.unparse(ast.fix_missing_locations(LegacyCalls().visit(tree)))


class Python2P0Migration(unittest.TestCase):
    def test_api_mapping_preserves_real_fixture_and_measurement_core(self):
        with tempfile.TemporaryDirectory() as folder:
            original=Path(folder)/'original.py';original.write_text(legacy_source())
            self.assertEqual(migration.fixture_hash(original),migration.fixture_hash(migration.WORKLOAD))
            self.assertEqual(migration.core_hash(original),migration.core_hash(migration.WORKLOAD))
            for old,new in [('image_size = 256','image_size = 512'),('range(100)','range(99)'),('1 + writes, 0','2 + writes, 0')]:
                changed=Path(folder)/'changed.py';changed.write_text(migration.WORKLOAD.read_text().replace(old,new))
                self.assertNotEqual(migration.fixture_hash(original),migration.fixture_hash(changed))
            changed.write_text(migration.WORKLOAD.read_text().replace('SAMPLES, WARMUP = 32, 4','SAMPLES, WARMUP = 16, 4'))
            self.assertNotEqual(migration.core_hash(original),migration.core_hash(changed))

    def test_adapter_cannot_hide_a_different_operation(self):
        with tempfile.TemporaryDirectory() as folder:
            changed=Path(folder)/'changed.py'
            changed.write_text(migration.WORKLOAD.read_text().replace('return edit.apply(plan)','return edit.apply_many((plan,))'))
            with self.assertRaisesRegex(ValueError,'Unreviewed P3A API adapter'):
                migration.fixture_hash(changed)

    def test_manifest_keeps_original_policy_and_both_clients_immutable(self):
        with tempfile.TemporaryDirectory() as folder:
            folder=Path(folder);original=folder/'original-workload.py';original.write_text(legacy_source())
            policy=folder/'policy.json';policy.write_text(json.dumps({'measurement_sha256':migration.sha(original),'thresholds':{'p50_ratio':1.5,'frozen':True}}))
            directories={side:folder/side for side in ('baseline','candidate')}
            for directory in directories.values():
                (directory/'tools').mkdir(parents=True);(directory/'tools/baseline.py').write_bytes(migration.WORKLOAD.read_bytes())
                (directory/'client/python').mkdir(parents=True);(directory/'client/python/api.py').write_text('version=1\n')
            manifest={'schema':1,'scope':'p3a-api-bindings-only','original_policy_sha256':migration.sha(policy),
                'original_workload_sha256':migration.sha(original),'common_workload_sha256':migration.sha(migration.WORKLOAD),
                'measurement_core_sha256':migration.core_hash(original),'fixture_semantics_sha256':migration.fixture_hash(original),
                'clients':{side:migration.client_hash(path) for side,path in directories.items()}}
            path=folder/'migration.json';path.write_text(json.dumps(manifest))
            self.assertEqual(migration.validate_migration(path,policy,directories),manifest)
            policy.write_text(policy.read_text().replace('1.5','2.0'))
            with self.assertRaisesRegex(ValueError,'policy changed'):
                migration.validate_migration(path,policy,directories)
            policy.write_text(policy.read_text().replace('2.0','1.5'))
            (directories['candidate']/'client/python/api.py').write_text('version=2\n')
            with self.assertRaisesRegex(ValueError,'client changed'):
                migration.validate_migration(path,policy,directories)

if __name__=='__main__':unittest.main()
