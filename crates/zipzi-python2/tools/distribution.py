#!/usr/bin/env python3
"""Build paired sdists, regenerate-check their sources, rebuild/install both wheels.

All acceptance runs use isolated interpreters and the archived production host.
No import or wheel installation invokes the authoring generator.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tomllib
import zipfile
from _support import exclusive_run

ROOT = Path(__file__).resolve().parents[3]
PACKAGE = ROOT / 'crates/zipzi-python2'


def extract(archive, target):
    with tarfile.open(archive) as source:
        source.extractall(target, filter='data')
    root=target / archive.name.removesuffix('.tar.gz')
    # Reproducible sdists normalize mtimes to zero. Refresh materialized inputs
    # so an explicitly reused Cargo cache cannot consider old dependency rmeta
    # fresh after source bytes change. Archive contents remain untouched.
    for path in root.rglob('*'):
        if path.is_file():os.utime(path,None)
    return root


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--observation',action='store_true',help='Also run installed GPU/inspection recipe; requires a usable device')
    parser.add_argument('--cargo-target', type=Path,
                        help='Optional isolated SDK build cache; never the workspace target')
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    environment = {k:v for k,v in os.environ.items() if k not in {'PYTHONPATH','PYTHONHOME','ZIPZI_DEV_SESSION_DIR'} and not k.startswith('ZIPZI_TEST_')}
    cargo_target = (args.cargo_target or output / 'cargo-target').resolve()
    if cargo_target == (ROOT / 'target').resolve():
        parser.error('--cargo-target must be isolated from the workspace target')
    environment['CARGO_TARGET_DIR'] = str(cargo_target)
    commands = []
    def run(name, command, cwd=ROOT, env=environment):
        with (output / (name+'.log')).open('w') as log:
            result = subprocess.run([str(x) for x in command], cwd=cwd, env=env, stdout=log, stderr=subprocess.STDOUT)
        commands.append(dict(name=name,command=[str(x) for x in command],cwd=str(cwd),exit_code=result.returncode))
        (output/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
        if result.returncode:
            print((output/(name+'.log')).read_text()[-6000:],file=sys.stderr)
            result.check_returncode()
        print(name+': passed',flush=True)
    with exclusive_run(output/'runner.lock'):
        version = tomllib.loads((PACKAGE/'client/pyproject.toml').read_text())['project']['version']
        run('client-sdist',['uv','build','--sdist','--out-dir',output/'sdists',PACKAGE/'client'])
        run('native-sdist',['uv','build','--sdist','--out-dir',output/'sdists',PACKAGE])
        client=extract(output/f'sdists/zipzi-{version}.tar.gz',output/'sources')
        native=extract(output/f'sdists/zipzi_native-{version}.tar.gz',output/'sources')
        archived=native/'crates/zipzi-python2'
        # No Git directory, no reference back to the live workspace.
        assert not (native/'.git').exists()
        run('source-generator-check',[sys.executable,'-I',archived/'tools/generate.py','--check'],cwd=native)
        for source in (PACKAGE/'client/python/zipzi').rglob('*'):
            if not source.is_file() or '__pycache__' in source.parts:continue
            relative=source.relative_to(PACKAGE/'client')
            assert (client/relative).read_bytes()==source.read_bytes(),relative
            assert (archived/'client'/relative).read_bytes()==source.read_bytes(),relative
        run('client-wheel',['uv','build','--wheel','--out-dir',output/'wheels',client])
        run('native-wheel',['uv','build','--wheel','--out-dir',output/'wheels',native])
        pure=next((output/'wheels').glob(f'zipzi-{version}-*.whl'))
        extension=next((output/'wheels').glob(f'zipzi_native-{version}-*.whl'))
        with zipfile.ZipFile(pure) as wheel:
            for source in (PACKAGE/'client/python/zipzi/_generated').glob('*.py'):
                assert wheel.read('zipzi/_generated/'+source.name)==source.read_bytes()
        run('production-host',['cargo','build','--locked','--release','-p','zipzi-runtime','--features','gpu-observation','--example','local_host_fixture'],cwd=native)
        import shutil
        host=output/'local_host_fixture'
        shutil.copy2(cargo_target/'release/examples/local_host_fixture',host)
        for mode in ('connected','local'):
            venv=output/(mode+'-venv')
            run(mode+'-venv',['uv','venv','--python','3.14',venv])
            python=venv/'bin/python'
            run(mode+'-install',['uv','pip','install','--python',python,pure,*([extension] if mode=='local' else [])])
            run(mode+'-recipe',[python,'-I',archived/'tests/installed_recipe.py',mode,*(['--observation'] if args.observation else [])],cwd=output,
                env=dict(environment,ZIPZI_PYTHON2_HOST=str(host)))
        artifacts=[*sorted((output/'sdists').glob('*')),*sorted((output/'wheels').glob('*')),host]
        report=dict(schema=1,status='passed',base_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
            dirty_files=subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).splitlines(),commands=commands,
            artifacts=[dict(path=str(p),sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in artifacts],
            source_contract_sha256=hashlib.sha256((archived/'schema/authoring.json').read_bytes()).hexdigest(),
            native_source_scope='SDK dependency closure plus complete Python client, native adapter, Rust declarations, exporter and generator; pure client sdist carries the same Python sources and generated outputs')
        (output/'results.json').write_text(json.dumps(report,indent=2)+'\n')
        print(output/'results.json')

if __name__=='__main__':main()
