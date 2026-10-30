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
import shutil
import subprocess
import sys
import tarfile
import time
import tomllib
import zipfile
from _support import exclusive_run, executable, executable_name, venv_python

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


def build_environment(cargo_target):
    environment = {k:v for k,v in os.environ.items() if k not in {'PYTHONPATH','PYTHONHOME','ZIPZI_DEV_SESSION_DIR'} and not k.startswith('ZIPZI_TEST_')}
    environment['CARGO_TARGET_DIR'] = str(cargo_target)
    # PEP 517 creates a new temporary venv for each wheel. Use the same base
    # interpreter for schema export and Maturin so PyO3's environment fingerprint
    # does not change between the two release builds.
    environment['PYO3_PYTHON'] = str(Path(sys._base_executable or sys.executable).resolve())
    environment['MATURIN_PEP517_USE_BASE_PYTHON'] = '1'
    return environment


def build_source_examples(source, output, run):
    run('source-exporter', ['cargo', 'build', '--locked', '--release',
        '--message-format=json-render-diagnostics', '-p', 'zipzi-contract',
        '--bin', 'authoring-schema'], cwd=source)
    messages = '\n'.join(line for line in (output/'source-exporter.log').read_text(encoding='utf-8').splitlines() if line.startswith('{'))
    exporter = executable(messages, 'authoring-schema', 'bin')
    run('source-host', ['cargo', 'build', '--locked', '--release',
        '--message-format=json-render-diagnostics', '-p', 'zipzi-runtime',
        '--features', 'gpu-observation', '--example', 'local_host_fixture'], cwd=source)
    messages = '\n'.join(line for line in (output/'source-host.log').read_text(encoding='utf-8').splitlines() if line.startswith('{'))
    host = output/executable_name('local_host_fixture')
    shutil.copy2(executable(messages, 'local_host_fixture', 'example'), host)
    run('source-contract-export', [exporter], cwd=source)
    return host, output/'source-contract-export.log'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--observation',action='store_true',help='Also run installed GPU/inspection recipe; requires a usable device')
    parser.add_argument('--cargo-target', type=Path,
                        help='Optional Cargo cache (may be shared with CI app builds); never the default workspace target')
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    cargo_target = (args.cargo_target or output / 'cargo-target').resolve()
    if cargo_target == (ROOT / 'target').resolve():
        parser.error('--cargo-target must be isolated from the workspace target')
    environment = build_environment(cargo_target)
    commands = []
    def run(name, command, cwd=ROOT, env=environment):
        started = time.perf_counter()
        with (output / (name+'.log')).open('w', encoding='utf-8') as log:
            result = subprocess.run([str(x) for x in command], cwd=cwd, env=env, stdout=log, stderr=subprocess.STDOUT)
        elapsed = round(time.perf_counter() - started, 3)
        commands.append(dict(name=name,command=[str(x) for x in command],cwd=str(cwd),exit_code=result.returncode,duration_seconds=elapsed))
        (output/'commands.json').write_text(json.dumps(commands,indent=2)+'\n')
        if result.returncode:
            print((output/(name+'.log')).read_text(encoding='utf-8')[-6000:],file=sys.stderr)
            result.check_returncode()
        print(f'{name}: passed ({elapsed:.3f}s)',flush=True)
    with exclusive_run(output/'runner.lock'):
        version = tomllib.loads((PACKAGE/'client/pyproject.toml').read_text(encoding='utf-8'))['project']['version']
        run('client-sdist',['uv','build','--sdist','--out-dir',output/'sdists',PACKAGE/'client'])
        run('native-sdist',['uv','build','--sdist','--out-dir',output/'sdists',PACKAGE])
        client=extract(output/f'sdists/zipzi-{version}.tar.gz',output/'sources')
        native=extract(output/f'sdists/zipzi_native-{version}.tar.gz',output/'sources')
        archived=native/'crates/zipzi-python2'
        # No Git directory, no reference back to the live workspace.
        assert not (native/'.git').exists()
        host, contract = build_source_examples(native, output, run)
        run('source-generator-check',[sys.executable,'-I',archived/'tools/generate.py','--check','--schema',contract],cwd=native)
        for source in (PACKAGE/'client/python/zipzi').rglob('*'):
            if not source.is_file() or '__pycache__' in source.parts:continue
            relative=source.relative_to(PACKAGE/'client')
            assert (client/relative).read_bytes()==source.read_bytes(),relative
            assert (archived/'client'/relative).read_bytes()==source.read_bytes(),relative
        run('client-wheel',['uv','build','--wheel','--out-dir',output/'wheels',client])
        run('native-wheel',['uv','build','--wheel','--config-setting','build-args=--profile release --locked','--out-dir',output/'wheels',native])
        pure=next((output/'wheels').glob(f'zipzi-{version}-*.whl'))
        extension=next((output/'wheels').glob(f'zipzi_native-{version}-*.whl'))
        with zipfile.ZipFile(pure) as wheel:
            for source in (PACKAGE/'client/python/zipzi/_generated').glob('*.py'):
                assert wheel.read('zipzi/_generated/'+source.name)==source.read_bytes()
        for mode in ('connected','local'):
            venv=output/(mode+'-venv')
            run(mode+'-venv',['uv','venv','--python','3.14',venv])
            python=venv_python(venv)
            run(mode+'-install',['uv','pip','install','--python',python,pure,*([extension] if mode=='local' else [])])
            run(mode+'-recipe',[python,'-I',archived/'tests/installed_recipe.py',mode,*(['--observation'] if args.observation else [])],cwd=output,
                env=dict(environment,ZIPZI_PYTHON2_HOST=str(host)))
        artifacts=[*sorted((output/'sdists').glob('*')),*sorted((output/'wheels').glob('*')),host]
        report=dict(schema=1,status='passed',base_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True, encoding='utf-8').strip(),
            dirty_files=subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True, encoding='utf-8').splitlines(),commands=commands,
            artifacts=[dict(path=str(p),sha256=hashlib.sha256(p.read_bytes()).hexdigest()) for p in artifacts],
            source_contract_sha256=hashlib.sha256((archived/'schema/authoring.json').read_bytes()).hexdigest(),
            native_source_scope='SDK dependency closure plus complete Python client, native adapter, Rust declarations, exporter and generator; pure client sdist carries the same Python sources and generated outputs')
        (output/'results.json').write_text(json.dumps(report,indent=2)+'\n')
        print(output/'results.json')

if __name__=='__main__':main()
