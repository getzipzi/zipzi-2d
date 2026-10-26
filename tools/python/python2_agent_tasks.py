"""Prepare installed-only packets, probe service identity and retain run records.

--preflight makes one bounded model service probe, separate from SDK tasks.
Packet preparation never executes a task or reports success from an agent claim.
P3A-6 supplies the isolated runner, host/controller events and independent verifier.
JSON task inputs live only in the selected temporary packet, never in plan folders.
"""
from __future__ import annotations
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import time
import tempfile
import os
from python2_agent_identity import evaluate,prepare_identity,runner_fingerprint,identity_command,IdentityFailure

ROOT=Path(__file__).resolve().parents[2]
_spec=importlib.util.spec_from_file_location('zipzi_fixed_agent_tasks',ROOT/'fixtures/python_sdk_agent/tasks.py')
_tasks=importlib.util.module_from_spec(_spec)
import sys
sys.modules[_spec.name]=_tasks
_spec.loader.exec_module(_tasks)
from python2_acceptance import BUDGET,COMMON_PROMPT,PROMPTS,validate,configuration,token_usage


def sha(path:Path)->str:return hashlib.sha256(path.read_bytes()).hexdigest()


def preflight_identity(events:list[dict],returncode:int,**evidence)->dict:
    """Accept only runner/service metadata, never model-written identity claims."""
    return evaluate(events,returncode,**evidence)


def preflight(output:Path,identity_launcher:Path|None=None)->dict:
    """One bounded service/identity probe, separate from the 42 fixed SDK tasks."""
    output.mkdir(parents=True,exist_ok=False)
    config=configuration(identity_launcher);validate(config)
    binding,identity_env=prepare_identity(output.resolve(),config)
    before=runner_fingerprint(config)
    if not before['verified']:raise IdentityFailure('Frozen runner/identity launcher changed before preflight')
    (output/'configuration.json').write_text(json.dumps(config,ensure_ascii=False,indent=2)+'\n')
    with tempfile.TemporaryDirectory(prefix='zz-agent-preflight-',dir='/tmp') as folder:
        command=[config['runner']['executable'],*config['runner']['arguments'],'--sandbox','read-only','--cd',folder,
                 'Service availability probe only. Do not use any tools or read files. Reply exactly READY.']
        command=identity_command(command,config)
        try:
            result=subprocess.run(command,cwd=folder,env={**os.environ,**identity_env},capture_output=True,text=True,timeout=60)
            stdout,stderr,returncode=result.stdout,result.stderr,result.returncode
        except subprocess.TimeoutExpired as error:
            stdout=error.stdout or b'';stderr=error.stderr or b''
            stdout=stdout.decode() if isinstance(stdout,bytes) else stdout
            stderr=stderr.decode() if isinstance(stderr,bytes) else stderr
            stderr+='\nService identity preflight timed out after 60 seconds.\n';returncode=124
    (output/'stdout.ndjson').write_text(stdout);(output/'stderr.log').write_text(stderr)
    events=[]
    for line in stdout.splitlines():
        try:value=json.loads(line)
        except json.JSONDecodeError:continue
        if isinstance(value,dict):events.append(value)
    after=runner_fingerprint(config)
    summary=preflight_identity(events,returncode,evidence_path=output/'service-identity.json',binding=binding,
        allow_sidecar=config['runner']['identity_launcher'] is not None,runner_verified=before['verified'] and after['verified'])
    summary['runner_fingerprint']=after
    summary.update(runner_version=config['runner']['version'],returncode=returncode,sdk_tasks_executed=0)
    (output/'result.json').write_text(json.dumps(summary,indent=2)+'\n')
    return summary


def prepare(destination:Path,config:dict,task_id:str,environment:str,repeat:int,wheels:list[Path],host_folder:Path,endpoint:Path|None)->Path:
    validate(config)
    fixture=_tasks.task(task_id,environment,repeat)
    if not host_folder.is_absolute():raise ValueError('Fixture host folder must be absolute')
    if environment=='connected' and (endpoint is None or not endpoint.is_absolute()):raise ValueError('Connected requires a host endpoint')
    if environment=='local' and not any('native' in p.name for p in wheels):raise ValueError('Local requires the paired normal native wheel')
    if environment=='connected' and any('native' in p.name for p in wheels):raise ValueError('Connected packet must use only the pure wheel')
    if any(not p.is_file() or p.suffix!='.whl' for p in wheels):raise ValueError('Only normal built wheel inputs are supported')
    destination.mkdir(parents=True,exist_ok=False,mode=0o700)
    venv=destination/'environment'
    subprocess.run(['uv','venv','--python','3.14',str(venv)],check=True)
    subprocess.run(['uv','pip','install','--python',str(venv/'bin/python'),*[str(p.resolve()) for p in wheels]],check=True)
    data=dict(fixture.data,environment=environment,host_folder=str(host_folder),endpoint=None if endpoint is None else str(endpoint),seed=fixture.seed)
    (destination/'task.json').write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n')
    (destination/'TASK.md').write_text(COMMON_PROMPT+'\n'+PROMPTS[task_id]+'\n\nPython: '+str(venv/'bin/python')+'\nInstalled distribution: zipzi-python2 (import zipzi). Offline documentation resources: zipzi/docs/guide.md, zipzi/docs/reference.md, zipzi/docs/recipes.py.\nSDK workflow execution: write a script in work/ and invoke sdk-python work/script.py as a standalone shell call. This runs the installed Python with the external fixture observer; use it for every workflow that opens a Runtime. Offline help queries can use environment/bin/python.\nDocumentation discovery: read guide.md first. The full reference has thousands of headings and exceeds the frozen 16384-byte tool output budget; listing every heading also exceeds it. Use zipzi.describe(public_path) and print documentation/signature/children for only the few relevant paths, or extract one named reference section. Keep each shell output below 16384 bytes.\n')
    # Hidden expectations and package fingerprints are stored outside the agent's readable packet.
    private=destination.parent/(destination.name+'.controller')
    private.mkdir(exist_ok=False)
    manifest={'task':task_id,'environment':environment,'seed':fixture.seed,'fixture':_tasks.FIXTURE_VERSION,
              'verifier':_tasks.VERIFIER_VERSION,'configuration':config['configuration_sha256'],
              'packages':{p.name:sha(p) for p in wheels},'hidden':fixture.hidden}
    (private/'manifest.json').write_text(json.dumps(manifest,sort_keys=True,indent=2)+'\n')
    subprocess.run([str(venv/'bin/python'),'-I',str(ROOT/'fixtures/python_sdk_agent/setup.py'),str(destination/'task.json'),str(private/'manifest.json')],check=True)
    return destination


class RunRecorder:
    """One append-only result per run. Never truncates an over-budget run into success."""
    def __init__(self,output:Path,identity:dict):
        self.output=output;self.identity=identity;self.started=time.monotonic()
        self.calls=0;self.usage=[];self.log_bytes=0
        self.exploration=0;self.invented_api=0;self.actions=[];self.failures=[]
        output.mkdir(parents=True,exist_ok=False)
        self._stream=(output/'events.ndjson').open('x')
    def event(self,kind:str,payload:dict)->None:
        elapsed=time.monotonic()-self.started
        if kind=='tool':
            self.calls+=1
            size=len(str(payload.get('output','')).encode())
            if size>BUDGET['tool_output_bytes_per_call']:self.failures.append('tool_output_budget')
            if payload.get('elapsed_seconds',0)>BUDGET['seconds_per_tool_call']:self.failures.append('tool_time_budget')
        if kind=='usage':
            self.usage.append(dict(payload))
        if kind=='exploration':self.exploration+=1
        if kind=='invented_api':self.invented_api+=1
        if kind=='recovery':self.actions.append(str(payload['action']))
        limits={'wall_seconds':elapsed,'tool_calls':self.calls}
        for key,value in limits.items():
            if value>BUDGET[key]:self.failures.append(key)
        line=json.dumps({'kind':kind,'elapsed_seconds':elapsed,**payload},ensure_ascii=False)+'\n'
        self.log_bytes+=len(line.encode())
        if self.log_bytes>BUDGET['log_bytes']:
            self.failures.append('log_bytes');raise RuntimeError('Run log budget exhausted; this run failed')
        self._stream.write(line);self._stream.flush()
        if self.failures:raise RuntimeError('Run exceeded a frozen budget: '+','.join(sorted(set(self.failures))))
    def finish(self,verification:dict,controller_events:set[str],required_events:set[str])->Path:
        if time.monotonic()-self.started>BUDGET['wall_seconds']:self.failures.append('wall_seconds')
        for key in ('model','service_revision','runner_version','package_hashes','document_hashes','prompt_hash'):
            if not self.identity.get(key):self.failures.append('missing_identity_'+key)
        missing=required_events-controller_events
        if missing:self.failures.append('missing_controller_events: '+','.join(sorted(missing)))
        if not verification.get('verified') or not verification.get('evidence_digest'):self.failures.append('independent_verifier_missing')
        if verification.get('coverage_complete') is not True:self.failures.append('independent_verifier_coverage_incomplete')
        if verification.get('hard_stop'):self.failures.extend(verification['hard_stop'])
        self._stream.close()
        success=not self.failures
        usage=token_usage(self.usage)
        counts={key:'unavailable' if value is None else str(value) for key,value in usage.items()}
        lines=['# P3A agent run result','',f"Task: {self.identity['task']} / {self.identity['environment']}; seed: {self.identity['seed']}",
               'Status: '+('passed' if success else 'failed'),'',f'Tool calls: {self.calls}; exploration: {self.exploration}; invented API calls: {self.invented_api}',
               f'Tokens (report only, no limit): {counts["input_tokens"]} input / {counts["output_tokens"]} output / {counts["total_tokens"]} total; log bytes: {self.log_bytes}',
               'Recovery actions: '+', '.join(self.actions),'Independent evidence digest: '+str(verification.get('evidence_digest','missing')),
               'Failures: '+('; '.join(self.failures) or 'none')]
        lines += ['', 'Run identity:']+[f'- {key}: {value}' for key,value in sorted(self.identity.items())]
        target=self.output/'result.md';target.write_text('\n'.join(lines)+'\n');return target


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--list',action='store_true',help='Print the fixed 42 task environments without running them')
    parser.add_argument('--preflight',type=Path,help='Run one 60-second service/identity probe in an empty directory; does not execute SDK tasks')
    parser.add_argument('--identity-launcher',type=Path,help='Optional pinned host transport-metadata launcher; never a model self-report')
    parser.add_argument('--prepare',type=Path,help='Create one installed-only packet, without running a model')
    parser.add_argument('--config',type=Path,help='Previously frozen P3A-0 configuration')
    parser.add_argument('--task',choices=tuple(PROMPTS));parser.add_argument('--environment',choices=('local','connected'))
    parser.add_argument('--repeat',type=int,choices=range(3),default=0)
    parser.add_argument('--wheel',type=Path,action='append',default=[])
    parser.add_argument('--host-folder',type=Path);parser.add_argument('--endpoint',type=Path)
    args=parser.parse_args()
    if args.list:
        for fixture in _tasks.matrix():print(f'{fixture.id} {fixture.environment} {fixture.seed}')
        return
    if args.preflight:
        result=preflight(args.preflight,args.identity_launcher);print(json.dumps(result,ensure_ascii=False))
        if not result['identity_verified']:raise SystemExit(1)
        return
    if not all((args.prepare,args.config,args.task,args.environment,args.host_folder,args.wheel)):parser.error('Preparation requires config/task/environment/wheel/host-folder')
    print(prepare(args.prepare,json.loads(args.config.read_text()),args.task,args.environment,args.repeat,args.wheel,args.host_folder,args.endpoint))

if __name__=='__main__':main()
