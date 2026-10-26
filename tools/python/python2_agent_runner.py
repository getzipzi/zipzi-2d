"""Execute frozen installed-only SDK tasks with functional acceptance.

Configured model/effort and optional service metadata are recorded for context.
All evidence is retained under an explicitly selected temporary output root.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time

from python2_agent_tasks import ROOT,prepare,sha,preflight_identity
from python2_acceptance import BUDGET,TOKEN_USAGE_POLICY,FINAL_RESPONSE_SCHEMA,COMMON_PROMPT,PROMPTS,configuration,digest,validate,token_usage
from python2_agent_controller import Controller
from python2_agent_observer import controller_events


def run_one(root:Path,config:dict,task:str,environment:str,repeat:int,wheels:list[Path],host:Path,verifier_python:Path)->dict:
    label=f'{task}-{environment}-{repeat}';packet=root/label;private=root/(label+'.controller')
    host_folder=packet/'host'
    host_private=root/(label+'.host')
    endpoint=host_private/'endpoint/runtime.sock' if environment=='connected' else None
    host_process=None;controller=None
    if endpoint is not None:
        host_private.mkdir();host_log=(host_private/'host.log').open('wb')
        host_environment={key:value for key,value in os.environ.items() if not key.startswith('ZIPZI_')}
        host_environment.update(ZIPZI_RUNTIME_DIR=str(host_private/'endpoint'),ZIPZI_RUNTIME_PROFILE='agent')
        host_process=subprocess.Popen([str(host)],env=host_environment,stdout=host_log,stderr=subprocess.STDOUT,start_new_session=True)
        actual=host_private/'endpoint/agent.sock'
        for _ in range(200):
            if actual.exists():break
            if host_process.poll() is not None:raise RuntimeError('Fixture host exited')
            time.sleep(.05)
        # The endpoint directory filename is supplied by the Rust endpoint contract.
        candidates=list((host_private/'endpoint').glob('*.sock'))
        if len(candidates)!=1:raise RuntimeError('Fixture host socket missing')
        endpoint=candidates[0]
    try:
        prepare(packet,config,task,environment,repeat,wheels,host_folder.resolve(),endpoint)
        private=root/(label+'.controller');host_folder.mkdir(exist_ok=True)
        harness_paths=['tools/python/python2_agent_runner.py','tools/python/python2_agent_shell.py',
            'tools/python/python2_agent_tasks.py','tools/python/python2_agent_controller.py',
            'tools/python/python2_acceptance.py','fixtures/python_sdk_agent/tasks.py',
            'fixtures/python_sdk_agent/setup.py','fixtures/python_sdk_agent/mutate.py','fixtures/python_sdk_agent/verify.py',
            'fixtures/python_sdk_agent/observe.py','tools/python/python2_agent_observer.py']
        harness_paths.append('tools/python/python2_agent_output.py')
        harness_paths.append('fixtures/python_sdk_agent/criteria.py')
        harness_paths.append('tools/python/python2_agent_identity.py')
        harness_hashes={path:sha(ROOT/path) for path in harness_paths}
        documents=json.loads(subprocess.check_output([str(packet/'environment/bin/python'),'-I','-c',
            "import hashlib,json,importlib.resources as r;p=r.files('zipzi');print(json.dumps({f:hashlib.sha256(p.joinpath(f).read_bytes()).hexdigest() for f in ['docs/guide.md','docs/reference.md','docs/recipes.py','_generated/catalog.py']}))"],text=True))
        task_path=packet/'task.json';data=json.loads(task_path.read_text())
        if endpoint is not None:
            # Only the verifier sees this copy; task packet exposes the agent endpoint.
            data['controller_endpoint']=str(endpoint)
        (private/'verifier-task.json').write_text(json.dumps(data)+'\n')
        if endpoint is not None:
            proxy=packet/'agent.sock'
            controller=Controller(proxy,endpoint,private,verifier_python,task)
            public_data=dict(data);public_data.pop('controller_endpoint',None);public_data['endpoint']=str(proxy)
            task_path.write_text(json.dumps(public_data,indent=2)+'\n')
        instructions='You execute one installed Python SDK evaluation task. Use only the provided sdk_shell shell tool and the packet inputs. Discover public APIs through installed documentation, verify actual results, obey the stated finite budgets, and report uncertainty honestly. Do not access private interfaces or external files. Write scripts in work/ and reports in output/. Execute every SDK workflow via the standalone shell command sdk-python work/script.py; offline documentation queries may use environment/bin/python.'
        (private/'instructions.md').write_text(instructions+'\n')
        (private/'final-schema.json').write_text(json.dumps(FINAL_RESPONSE_SCHEMA)+'\n')
        arguments=[config['runner']['executable'],*config['runner']['arguments'],
            '--sandbox','read-only','--cd',str(packet),
            '--output-schema',str(private/'final-schema.json'),
            '-c','features.shell_tool=false','-c','features.unified_exec=false',
            '-c','features.multi_agent=false','-c','features.apps=false',
            '-c','mcp_servers.sdk_shell.command='+json.dumps(sys.executable),
            '-c','mcp_servers.sdk_shell.args='+json.dumps([str(ROOT/'tools/python/python2_agent_shell.py'),str(packet),str(private)]),
            '-c','mcp_servers.sdk_shell.startup_timeout_sec=30',
            '-c','mcp_servers.sdk_shell.tool_timeout_sec=65',
            '-c','mcp_servers.sdk_shell.default_tools_approval_mode="approve"',
            '-c','project_doc_max_bytes=0',
            '-c','model_instructions_file='+json.dumps(str(private/'instructions.md')),
            '-c','developer_instructions='+json.dumps('Execute only the SDK task in TASK.md. The external sdk_shell shell is the sole allowed tool. Do not use built-in tools, other apps, network, repository files, user instructions or skills. Keep each tool output below the stated budget.'),
            (packet/'TASK.md').read_text()+'\nRead task.json. Frozen budgets: '+json.dumps(BUDGET)+'\nToken usage is reported after the run; no input, output, total or per-call token limits are enforced. Return the final JSON status/artifacts/unconfirmed. unconfirmed lists only unresolved required task outcomes, and is empty when the task is confirmed complete. Long shell output is paginated; use its read_output_command to continue when needed.']
        (private/'command.json').write_text(json.dumps(arguments,indent=2)+'\n')
        started=time.monotonic();failure=[]
        with (private/'agent.ndjson').open('wb') as out,(private/'agent.stderr').open('wb') as err:
            process=subprocess.Popen(arguments,cwd=packet,stdin=subprocess.DEVNULL,stdout=out,stderr=err,start_new_session=True)
            while process.poll() is None:
                elapsed=time.monotonic()-started
                size=(private/'agent.ndjson').stat().st_size
                if (private/'budget-failed').exists():
                    failure.append((private/'budget-failed').read_text())
                    os.killpg(process.pid,signal.SIGKILL);process.wait();break
                if elapsed>BUDGET['wall_seconds'] or size>BUDGET['log_bytes']:
                    failure.append('wall_seconds' if elapsed>BUDGET['wall_seconds'] else 'log_bytes')
                    os.killpg(process.pid,signal.SIGKILL);process.wait();break
                time.sleep(.5)
        events=[]
        for line in (private/'agent.ndjson').read_text().splitlines():
            try:events.append(json.loads(line))
            except json.JSONDecodeError:pass
        metadata=preflight_identity(events,process.returncode)
        active_shell=private/'active-shell-pgid'
        if active_shell.exists():
            try:os.killpg(int(active_shell.read_text()),signal.SIGKILL)
            except ProcessLookupError:pass
        calls=[]
        if (private/'shell-events.ndjson').exists():calls=[json.loads(line) for line in (private/'shell-events.ndjson').read_text().splitlines()]
        attempts=private/'tool-attempts.ndjson'
        tool_calls=sum(1 for _ in attempts.open()) if attempts.exists() else len(calls)
        usage=[event.get('usage') if isinstance(event.get('usage'),dict) else {}
               for event in events if event.get('type')=='turn.completed']
        tokens=token_usage(usage)
        inputs,outputs=tokens['input_tokens'],tokens['output_tokens']
        limits={'tool_calls':tool_calls}
        failure.extend(key for key,value in limits.items() if value>BUDGET[key])
        if any(call['elapsed_seconds']>BUDGET['seconds_per_tool_call'] for call in calls):failure.append('seconds_per_tool_call')
        log_size=sum(path.stat().st_size for path in private.glob('*.ndjson'))
        if log_size>BUDGET['log_bytes']:failure.append('log_bytes')
        if (private/'budget-failed').exists():failure.append((private/'budget-failed').read_text())
        if process.returncode:failure.append('runner_exit_'+str(process.returncode))
        if not calls:failure.append('no_shell_calls_infrastructure_failure')
        subprocess.run([str(verifier_python),'-I',str(ROOT/'fixtures/python_sdk_agent/verify.py'),str(private/'verifier-task.json'),str(private),str(private/'verification.json')],stdout=(private/'verification.log').open('w'),stderr=subprocess.STDOUT,timeout=60)
        verification=json.loads((private/'verification.json').read_text())
        final_message={}
        for event in events:
            item=event.get('item',{})
            if item.get('type')=='agent_message':
                try:
                    candidate=json.loads(item.get('text',''))
                    if isinstance(candidate,dict) and 'status' in candidate:final_message=candidate
                except json.JSONDecodeError:pass
        if final_message.get('status')!='completed':failure.append('agent_did_not_confirm_completion')
        if final_message.get('status')=='completed' and (final_message.get('unconfirmed') or verification.get('checks',{}).get('no_unknown_recovery_left') is False):
            verification.setdefault('hard_stop',[]).append('unknown_reported_as_success')
        failure.extend(verification.get('hard_stop',[]))
        manifest=json.loads((private/'manifest.json').read_text())
        actual_events=(set(controller.events) if controller is not None else set())|controller_events(private)
        if (private/'observer-error').exists():failure.append('observer_error: '+(private/'observer-error').read_text())
        missing=set(manifest['hidden']['required_controller_events'])-actual_events
        if missing:failure.append('missing_controller_events: '+','.join(sorted(missing)))
        if harness_hashes!={path:sha(ROOT/path) for path in harness_paths}:failure.append('diagnostic_harness_changed_during_run')
        functional=bool(verification.get('verified') and verification.get('coverage_complete')) and not failure
        result={'task':task,'environment':environment,'seed':data['seed'],'functional_passed':functional,
                'status':'passed' if functional else 'failed','model':config['model'],
                'service_metadata':metadata,'runner_version':config['runner']['version'],'configuration':config['configuration_sha256'],
                'package_hashes':manifest['packages'],'prompt_hash':digest(COMMON_PROMPT+PROMPTS[task]),
                'tool_calls':tool_calls,'input_tokens':inputs,'output_tokens':outputs,'elapsed_seconds':time.monotonic()-started,
                'log_bytes':log_size,
                'failures':failure,'verification':{key:value for key,value in verification.items() if key!='evidence'}}
        result['controller_events']=sorted(actual_events)
        result['agent_final']=final_message
        result.update(total_tokens=tokens['total_tokens'],token_usage_policy=TOKEN_USAGE_POLICY)
        result['failures']=sorted(set(failure))
        observed_path=private/'observer-events.ndjson'
        observed=[json.loads(line) for line in observed_path.read_text().splitlines()] if observed_path.exists() else []
        first=min((event['shell_call_index'] for event in observed if event['kind']=='public_return'),default=None)
        recovery=[event['path'] for event in observed if event['kind']=='public_return' and ('recover' in event.get('path','') or event.get('path')=='recover_call')]
        result.update(mode='functional_matrix',document_hashes=documents,
            harness_hashes=harness_hashes,actual_prompt_hash=digest(arguments[-1]),
            task_input_hash=sha(task_path),instructions_hash=sha(private/'instructions.md'),
            exploration_calls_before_first_correct_call=None if first is None else first-1,invented_api_calls=None,recovery_actions=recovery)
        result['call_classification']='First correct workflow call uses observed public return; invented-API classification is unavailable for unobserved offline shell commands.'
        result['artifact_verified']=bool(verification.get('verified'))
        (private/'result.json').write_text(json.dumps(result,indent=2)+'\n')
        counts={key:'unavailable' if value is None else str(value) for key,value in tokens.items()}
        (private/'result.md').write_text(f'# {label}\n\nFunctional: {functional}; overall: {result["status"]}.\nModel: {config["model"]["id"]}/{config["model"]["reasoning_effort"]}; optional service metadata: {json.dumps(metadata)}.\n\nTools: {tool_calls}; tokens (report only, no limit): {counts["input_tokens"]} input / {counts["output_tokens"]} output / {counts["total_tokens"]} total; seconds: {result["elapsed_seconds"]:.1f}.\n\nVerifier: {json.dumps(result["verification"])}\n\nFailures: {failure}.\n')
        return result
    finally:
        if controller is not None:controller.close()
        if host_process is not None:
            os.killpg(host_process.pid,signal.SIGTERM)
            try:host_process.wait(timeout=5)
            except subprocess.TimeoutExpired:os.killpg(host_process.pid,signal.SIGKILL);host_process.wait()
            host_log.close()


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--task',choices=tuple(PROMPTS),required=True)
    parser.add_argument('--environment',choices=('local','connected'),required=True)
    parser.add_argument('--repeat',type=int,choices=range(3),default=0)
    parser.add_argument('--wheel',type=Path,action='append',required=True)
    parser.add_argument('--host',type=Path,required=True)
    parser.add_argument('--verifier-python',type=Path,required=True)
    args=parser.parse_args();args.output.mkdir(parents=True,exist_ok=True)
    config=configuration();validate(config)
    frozen=args.output/'configuration.json'
    if frozen.exists():
        prior=json.loads(frozen.read_text())
        if prior!=config:raise ValueError('Frozen runner or configuration drift')
    else:frozen.write_text(json.dumps(config,indent=2)+'\n')
    result=run_one(args.output.resolve(),config,args.task,args.environment,args.repeat,[path.resolve() for path in args.wheel],args.host.resolve(),args.verifier_python.absolute())
    print(json.dumps(result,ensure_ascii=False))


if __name__=='__main__':main()
