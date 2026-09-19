#!/usr/bin/env python3
"""Run M1 acceptance, retaining a report for every required gate (macOS arm64)."""
import argparse
import fcntl
import hashlib
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import uuid

from validate_m1_core import source_fingerprint

ROOT = Path(__file__).resolve().parents[1]

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--godot',type=Path,default=Path('/Applications/Godot_mono.app/Contents/MacOS/Godot'))
    parser.add_argument('--sdk',type=Path,default=ROOT/'third_party/CubismSdkForNative-5-r.5')
    args=parser.parse_args()
    output=ROOT/'target/kasane/runs'/uuid.uuid4().hex
    output.mkdir(parents=True,exist_ok=True)
    core_output=output/'core'
    godot_output=output/'godot'
    gpu_output=output/'gpu'
    names=['core','gd_cubism_build','gd_kasane_build','godot','gpu','purism_c99_bundle']
    checks=[dict(name=name,status='not_run') for name in names]
    def git(*cmd):return subprocess.check_output(['git',*cmd],cwd=ROOT,text=True).strip()
    report=dict(milestone='M1',status='failed',run_id=output.name,source_sha256=source_fingerprint(),git_revision=git('rev-parse','HEAD'),working_tree=git('status','--short'),
                submodules=git('submodule','status'),purism_working_tree=git('-C','modules/purism-core','status','--short'),
                platform=platform.platform(),architecture=platform.machine(),checks=checks)
    def run(name,command):
        check=next(c for c in checks if c['name']==name)
        check['command']=list(map(str,command));check['status']='failed'
        env=os.environ.copy()
        env.pop('CUBISM_CORE_LIBRARY',None)
        env['CUBISM_CORE_PROVIDER']='cubism'
        result=subprocess.run(check['command'],cwd=ROOT,env=env,text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
        log=output/(name+'.log');log.write_text(result.stdout);check['log']=str(log);check['returncode']=result.returncode
        if result.returncode:raise RuntimeError(f'{name} failed; see {log}:\n{result.stdout[-1600:]}')
        check['status']='passed'
    try:
        if platform.system()!='Darwin' or platform.machine()!='arm64':
            raise RuntimeError('This acceptance driver requires macOS arm64; core CMake remains portable')
        run('core',[sys.executable,ROOT/'tools/validate_m1_core.py','--sdk',args.sdk,'--output-dir',core_output])
        run('gd_cubism_build',[sys.executable,'-m','SCons','-C',ROOT/'modules/gd-cubism','platform=macos','arch=arm64','target=template_release','CUBISM_SDK_ROOT='+str(args.sdk.resolve()),'-j8'])
        run('gd_kasane_build',[sys.executable,'-m','SCons','-C',ROOT/'modules/gd-kasane','platform=macos','arch=arm64','target=template_debug','-j8'])
        run('godot',[sys.executable,ROOT/'tools/validate_m1_godot.py','--godot',args.godot,'--output-dir',godot_output])
        run('gpu',[sys.executable,ROOT/'tools/validate_m1_gpu.py','--godot',args.godot,'--output-dir',gpu_output,'--core-build',core_output/'build'])
        bundle=subprocess.run(['sh',str(ROOT/'modules/purism-core/scripts/bundle.sh')],cwd=ROOT,text=True,stdout=subprocess.PIPE,stderr=subprocess.PIPE,check=True).stdout
        (output/'PurismCoreBundle.h').write_text(bundle)
        (output/'bundle.c').write_text('#define PURISM_CORE_IMPLEMENTATION\n#include "PurismCoreBundle.h"\nint main(void){return csmGetVersion()==0;}\n')
        run('purism_c99_bundle',['cc','-std=c99',output/'bundle.c','-lm','-o',output/'bundle'])
        subprocess.run([str(output/'bundle')],check=True,stdout=subprocess.PIPE)
        report['core']=json.loads((core_output/'report.json').read_text())
        report['godot']=json.loads((godot_output/'report.json').read_text())
        report['gpu']=json.loads((gpu_output/'report.json').read_text())
        if any(report[name]['status']!='passed' for name in ('core','godot','gpu')):raise RuntimeError('Child report did not pass')
        report['status']='passed'
        report['known_limitations']=['Godot 4.7.2 mono first dynamic editor import can crash at exit; also reproduced on pre-refactor HEAD. Tests preregister extensions.',
            'M2 packaged-project migration, M3 import, M4 shared renderer and M5 UI are separate milestones.']
    except Exception as exc:
        report['error']=str(exc)
        print(exc)
    files=[]
    for root in (core_output,gpu_output,godot_output):
        for p in sorted(root.rglob('*')):
            if p.is_file() and '.godot' not in p.parts and p.suffix in ('.png','.moc3','.json','.log','.gdshader','.dylib'):
                files.append(dict(path=str(p),sha256=hashlib.sha256(p.read_bytes()).hexdigest()))
    reference=ROOT/'modules/gd-cubism/addons/gd_cubism/bin/libgd_cubism.cubism.macos.release.framework/libgd_cubism.cubism.macos.release'
    if reference.is_file():files.append(dict(path=str(reference),sha256=hashlib.sha256(reference.read_bytes()).hexdigest()))
    report['artifacts']=files
    (output/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(f'M1 {report["status"]}: {output / "report.json"}')
    return 0 if report['status']=='passed' else 1

def locked_main():
    # SCons still writes its extension objects to fixed module build directories.
    # Serialize complete acceptance runs while keeping reports and fixtures isolated.
    lock_path=ROOT/'target/kasane/.m1-acceptance.lock'
    lock_path.parent.mkdir(parents=True,exist_ok=True)
    with lock_path.open('w') as lock:
        fcntl.flock(lock,fcntl.LOCK_EX)
        return main()

if __name__=='__main__':raise SystemExit(locked_main())
