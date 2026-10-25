"""Prepare installed-only P3A agent packets and retain bounded, immutable run records.

This tool never runs a model or reports success from an agent claim. P3A-6 supplies
the isolated runner, host/controller events and independent model/file verifier.
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

ROOT=Path(__file__).resolve().parents[2]
_spec=importlib.util.spec_from_file_location('zipzi_fixed_agent_tasks',ROOT/'fixtures/python_sdk_agent/tasks.py')
_tasks=importlib.util.module_from_spec(_spec)
import sys
sys.modules[_spec.name]=_tasks
_spec.loader.exec_module(_tasks)
from python2_acceptance import BUDGET,COMMON_PROMPT,PROMPTS,validate


def sha(path:Path)->str:return hashlib.sha256(path.read_bytes()).hexdigest()


def prepare(destination:Path,config:dict,task_id:str,environment:str,repeat:int,wheels:list[Path],host_folder:Path,endpoint:Path|None)->Path:
    validate(config)
    fixture=_tasks.task(task_id,environment,repeat)
    if not host_folder.is_absolute():raise ValueError('Fixture host folder must be absolute')
    if environment=='connected' and (endpoint is None or not endpoint.is_absolute()):raise ValueError('Connected requires a host endpoint')
    if environment=='local' and not any('native' in p.name for p in wheels):raise ValueError('Local requires the paired normal native wheel')
    if environment=='connected' and any('native' in p.name for p in wheels):raise ValueError('Connected packet must use only the pure wheel')
    if any(not p.is_file() or p.suffix!='.whl' for p in wheels):raise ValueError('Only normal built wheel inputs are supported')
    destination.mkdir(parents=True,exist_ok=False)
    venv=destination/'environment'
    subprocess.run(['uv','venv','--python','3.14',str(venv)],check=True)
    subprocess.run(['uv','pip','install','--python',str(venv/'bin/python'),*[str(p.resolve()) for p in wheels]],check=True)
    data=dict(fixture.data,environment=environment,host_folder=str(host_folder),endpoint=None if endpoint is None else str(endpoint),seed=fixture.seed)
    (destination/'task.json').write_text(json.dumps(data,ensure_ascii=False,indent=2)+'\n')
    (destination/'TASK.md').write_text(COMMON_PROMPT+'\n'+PROMPTS[task_id]+'\n\nPython: '+str((venv/'bin/python').resolve())+'\n')
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
        self.calls=0;self.input_tokens=0;self.output_tokens=0;self.log_bytes=0
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
            self.input_tokens+=int(payload.get('input_tokens',0));tokens=int(payload.get('output_tokens',0));self.output_tokens+=tokens
            if tokens>BUDGET['output_tokens_per_model_call']:self.failures.append('model_output_budget')
        if kind=='exploration':self.exploration+=1
        if kind=='invented_api':self.invented_api+=1
        if kind=='recovery':self.actions.append(str(payload['action']))
        limits={'wall_seconds':elapsed,'tool_calls':self.calls,'input_tokens':self.input_tokens,'output_tokens':self.output_tokens,'total_tokens':self.input_tokens+self.output_tokens}
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
        if verification.get('hard_stop'):self.failures.extend(verification['hard_stop'])
        self._stream.close()
        success=not self.failures
        lines=['# P3A agent run result','',f"Task: {self.identity['task']} / {self.identity['environment']}; seed: {self.identity['seed']}",
               'Status: '+('passed' if success else 'failed'),'',f'Tool calls: {self.calls}; exploration: {self.exploration}; invented API calls: {self.invented_api}',
               f'Tokens: {self.input_tokens} input / {self.output_tokens} output; log bytes: {self.log_bytes}',
               'Recovery actions: '+', '.join(self.actions),'Independent evidence digest: '+str(verification.get('evidence_digest','missing')),
               'Failures: '+('; '.join(self.failures) or 'none')]
        lines += ['', 'Run identity:']+[f'- {key}: {value}' for key,value in sorted(self.identity.items())]
        target=self.output/'result.md';target.write_text('\n'.join(lines)+'\n');return target


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--list',action='store_true',help='Print the fixed 42 task environments without running them')
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
    if not all((args.prepare,args.config,args.task,args.environment,args.host_folder,args.wheel)):parser.error('Preparation requires config/task/environment/wheel/host-folder')
    print(prepare(args.prepare,json.loads(args.config.read_text()),args.task,args.environment,args.repeat,args.wheel,args.host_folder,args.endpoint))

if __name__=='__main__':main()
