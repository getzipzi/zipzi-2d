#!/usr/bin/env python3
"""Freeze an explicit, symmetric P3A API migration of the six existing P0 cases.

The original script/policy/binaries remain intact. Both frozen clients run the
same new worker. Only fixture API bindings differ; timing/probe loops, input
sizes, sample counts, independent assertions and thresholds remain unchanged.
"""
import argparse
import ast
import hashlib
import json
from pathlib import Path
import shutil
import sys

ROOT=Path(__file__).resolve().parents[2]
WORKLOAD=ROOT/'crates/zipzi-python2/tools/baseline.py'
API_BINDINGS_SHA256='497b6d52e02fdb2f690b7893a3bd4b327cd04d29702f1ff9484e265d79f08fe1'


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def core_hash(path):
    tree=ast.parse(path.read_text())
    tree.body=[node for node in tree.body if not isinstance(node,ast.FunctionDef) or node.name!='fixture']
    return hashlib.sha256(ast.dump(tree,include_attributes=False).encode()).hexdigest()


def fixture_hash(path):
    fixture=next(node for node in ast.parse(path.read_text()).body if isinstance(node,ast.FunctionDef) and node.name=='fixture')
    body=[]
    for node in fixture.body:
        if isinstance(node,ast.If) and isinstance(node.test,ast.Call) and ast.unparse(node.test.func)=='hasattr':
            if hashlib.sha256(ast.dump(node,include_attributes=False).encode()).hexdigest()!=API_BINDINGS_SHA256:
                raise ValueError('Unreviewed P3A API adapter')
        else:body.append(node)
    fixture.body=body
    class NormalizeAPI(ast.NodeTransformer):
        def visit_Call(self,node):
            self.generic_visit(node);name=ast.unparse(node.func)
            def call(name,args):return ast.Call(func=ast.Name(id=name,ctx=ast.Load()),args=args,keywords=[])
            if name=='session.meshes.plan':
                intent=node.args[0]
                if ast.unparse(intent.func)!='meshes.RenameIntent':raise ValueError('Unreviewed legacy planning call')
                return call('same_plan',intent.args)
            if name=='plan_method':return call('same_plan',[ast.Name(id='face',ctx=ast.Load()),*node.args[0].args])
            if name=='edit.apply_plan':return call('same_apply',[ast.Name(id='edit',ctx=ast.Load()),*node.args])
            if name=='apply_plan':return call('same_apply',node.args)
            if name in {'ops.Rename','rename_operation'}:return call('same_rename',node.args)
            if name=='domain_value':return node.args[0]
            return node
    fixture=NormalizeAPI().visit(fixture)
    return hashlib.sha256(ast.dump(fixture,include_attributes=False).encode()).hexdigest()


def client_hash(directory):
    digest=hashlib.sha256()
    for path in sorted((directory/'client/python').rglob('*.py')):
        digest.update(path.relative_to(directory).as_posix().encode());digest.update(b'\0');digest.update(path.read_bytes())
    return digest.hexdigest()


def validate_migration(manifest_path,policy_path,directories):
    manifest=json.loads(manifest_path.read_text());policy=json.loads(policy_path.read_text())
    if manifest.get('schema')!=1 or manifest.get('scope')!='p3a-api-bindings-only':
        raise ValueError('Unreviewed workload migration')
    original=manifest_path.parent/'original-workload.py'
    expected={
        'original_policy_sha256':sha(policy_path),
        'original_workload_sha256':policy['measurement_sha256'],
        'common_workload_sha256':sha(WORKLOAD),
        'measurement_core_sha256':core_hash(original),
        'fixture_semantics_sha256':fixture_hash(original),
    }
    if any(manifest.get(key)!=value for key,value in expected.items()):
        raise ValueError('Migration identity or original policy changed')
    if sha(original)!=policy['measurement_sha256'] or core_hash(original)!=core_hash(WORKLOAD):
        raise ValueError('Migration changed measurement loops, probes, samples or policy')
    if fixture_hash(original)!=fixture_hash(WORKLOAD):
        raise ValueError('Migration changed fixture inputs, operation sequence or independent assertions')
    for side,directory in directories.items():
        if sha(directory/'tools/baseline.py')!=manifest['common_workload_sha256']:
            raise ValueError('Both API generations must run the identical common workload')
        if client_hash(directory)!=manifest['clients'][side]:
            raise ValueError('Frozen migration client changed')
    return manifest


def prepare(baseline,builds,policy_path,output):
    policy=json.loads(policy_path.read_text());original=baseline/'tools/baseline.py'
    if policy.get('schema')!=2 or not policy['thresholds'].get('frozen'):
        raise ValueError('Use the retained original frozen policy')
    if sha(original)!=policy['measurement_sha256'] or core_hash(original)!=core_hash(WORKLOAD):
        raise ValueError('Original workload identity or unchanged measurement core is missing')
    if fixture_hash(original)!=fixture_hash(WORKLOAD):
        raise ValueError('Fixture changed beyond the reviewed public API bindings')
    output.mkdir(parents=True,exist_ok=False)
    shutil.copy2(original,output/'original-workload.py')
    directories={side:output/side for side in ('baseline','candidate')}
    for side,directory in directories.items():
        source=baseline if side=='baseline' else ROOT/'crates/zipzi-python2'
        directory.mkdir()
        shutil.copytree(source/'client',directory/'client',ignore=shutil.ignore_patterns('__pycache__','*.pyc','*.egg-info'))
        (directory/'tools').mkdir();shutil.copy2(WORKLOAD,directory/'tools/baseline.py')
        artifacts=baseline if side=='baseline' else builds
        for role in ('timing','probe'):
            target=directory/role;target.mkdir()
            for name in ('build-metadata.json','local_host_fixture.exe' if sys.platform == 'win32' else 'local_host_fixture'):shutil.copy2(artifacts/role/name,target/name)
            shutil.copytree(artifacts/role/'native',target/'native',ignore=shutil.ignore_patterns('__pycache__','*.pyc'))
    manifest={'schema':1,'scope':'p3a-api-bindings-only','original_policy_sha256':sha(policy_path),
        'original_workload_sha256':sha(original),'common_workload_sha256':sha(WORKLOAD),
        'measurement_core_sha256':core_hash(original),'fixture_semantics_sha256':fixture_hash(original),
        'clients':{side:client_hash(path) for side,path in directories.items()},
        'api_mapping':{
            'batch':'ops.Rename -> meshes.Rename with identical id/name inputs and operation ordering',
            'plan':'meshes.RenameIntent/meshes.plan/apply_plan -> meshes.Rename/Session.plan/Edit.apply',
            'read_validation':'direct payload -> Snapshot.value outside timing/probe windows',
        },
        'shared_binding_cost':'Both sides execute one identical worker; API aliases are selected before invoke. Plan adapters add one symmetric Python call on each side.',
        'historical_policy':'Original thresholds and failed measurements are retained; this is a contemporaneous migrated-workload comparison, not a rewrite of the old result.'}
    path=output/'migration.json';path.write_text(json.dumps(manifest,indent=2)+'\n')
    validate_migration(path,policy_path,directories)
    return path


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--baseline',type=Path,required=True,help='Retained original frozen client/worker and timing/probe artifacts')
    parser.add_argument('--builds',type=Path,required=True,help='Current prebuilt timing/ and probe/ artifact directories')
    parser.add_argument('--policy',type=Path,required=True,help='Original frozen schema-2 policy; never modified')
    parser.add_argument('--output',type=Path,required=True,help='New frozen comparison directory; never overwritten')
    args=parser.parse_args()
    print(prepare(args.baseline.resolve(),args.builds.resolve(),args.policy.resolve(),args.output.resolve()))

if __name__=='__main__':main()
