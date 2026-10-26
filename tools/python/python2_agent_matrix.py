"""Freeze one functional 42-run cohort and record actual service identity separately.

An incomplete service identity never becomes a full acceptance pass. Failed runs
are retained once, not retried or replaced; a hard stop ends the remaining cohort.
"""
from concurrent.futures import ThreadPoolExecutor,as_completed
import argparse
import json
from pathlib import Path
import time

from python2_agent_tasks import ROOT,sha,_tasks
from python2_agent_runner import run_one
from python2_acceptance import configuration,validate,token_usage
from python2_agent_identity import cohort_identity,IdentityFailure


def summarize(results):
    groups={}
    for result in results:groups.setdefault((result['task'],result['environment']),[]).append(result)
    good=sum(r['functional_passed'] for r in results)
    hard=sorted({failure for r in results for failure in r.get('verification',{}).get('hard_stop',[])})
    functional=len(results)==42 and good>=38 and all(len(rows)==3 and sum(row['functional_passed'] for row in rows)>=2 for rows in groups.values()) and not hard
    identity=cohort_identity(results)
    return {'runs':len(results),'functional_successes':good,'functional_gate_passed':functional,
        'identity_verified':identity['verified'],'identity_gate':identity,
        'overall_passed':functional and identity['verified'],
        'hard_stop':hard,'token_usage':token_usage(results),
        'known_token_usage':token_usage([r for r in results if r.get('input_tokens') is not None and r.get('output_tokens') is not None]),
        'missing_token_usage_runs':sum(r.get('input_tokens') is None or r.get('output_tokens') is None for r in results),
        'per_environment':{task+'/'+env:{'runs':len(rows),'successes':sum(row['functional_passed'] for row in rows)} for (task,env),rows in sorted(groups.items())}}


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--output',type=Path,required=True);p.add_argument('--pure-wheel',type=Path,required=True)
    p.add_argument('--native-wheel',type=Path,required=True);p.add_argument('--host',type=Path,required=True)
    p.add_argument('--verifier-python',type=Path,required=True);p.add_argument('--workers',type=int,default=4)
    p.add_argument('--repeat',type=int,choices=range(3),help='Run one fixed repetition across all fourteen environments')
    p.add_argument('--identity-launcher',type=Path,help='Optional frozen host launcher for actual service metadata')
    args=p.parse_args();root=args.output.resolve();root.mkdir(parents=True,exist_ok=True)
    config=configuration(args.identity_launcher);validate(config)
    paths=['tools/python/python2_agent_runner.py','tools/python/python2_agent_shell.py','tools/python/python2_agent_controller.py',
        'tools/python/python2_agent_observer.py','tools/python/python2_agent_tasks.py','tools/python/python2_acceptance.py',
        'tools/python/python2_agent_matrix.py','fixtures/python_sdk_agent/tasks.py','fixtures/python_sdk_agent/setup.py',
        'fixtures/python_sdk_agent/observe.py','fixtures/python_sdk_agent/mutate.py','fixtures/python_sdk_agent/verify.py']
    paths.append('tools/python/python2_agent_output.py')
    paths.append('fixtures/python_sdk_agent/criteria.py')
    paths.append('tools/python/python2_agent_identity.py')
    freeze={'configuration':config,'harness':{name:sha(ROOT/name) for name in paths},
        'packages':{path.name:sha(path) for path in (args.pure_wheel,args.native_wheel)},'host':sha(args.host)}
    frozen=root/'freeze.json'
    if frozen.exists():
        if json.loads(frozen.read_text())!=freeze:raise ValueError('Cohort identities changed; existing evidence cannot be reused as this cohort')
    else:frozen.write_text(json.dumps(freeze,indent=2)+'\n')
    results=[]
    for path in root.glob('*.controller/result.json'):results.append(json.loads(path.read_text()))
    if any(result.get('verification',{}).get('hard_stop') for result in results):raise SystemExit('Existing hard stop blocks further runs')
    if cohort_identity(results)['status']=='failed':raise SystemExit('Existing model/service/runner identity drift blocks further runs')
    fixtures=list(_tasks.matrix())
    if args.repeat is not None:fixtures=[f for f in _tasks.matrix() if f.seed==_tasks.SEEDS[f.id][args.repeat]]
    done={(r['task'],r['environment'],r['seed']) for r in results}
    todo=[f for f in fixtures if (f.id,f.environment,f.seed) not in done]
    started=time.monotonic();hard=False
    with ThreadPoolExecutor(max_workers=args.workers) as executor:
        pending={}
        def submit(fixture):
            repeat=_tasks.SEEDS[fixture.id].index(fixture.seed)
            wheels=[args.pure_wheel.resolve()]+([args.native_wheel.resolve()] if fixture.environment=='local' else [])
            future=executor.submit(run_one,root,config,fixture.id,fixture.environment,repeat,wheels,args.host.resolve(),args.verifier_python.absolute())
            pending[future]=fixture
        queue=iter(todo)
        for _ in range(min(args.workers,len(todo))):submit(next(queue))
        while pending:
            future=next(as_completed(pending));fixture=pending.pop(future)
            try:result=future.result()
            except Exception as error:
                result={'task':fixture.id,'environment':fixture.environment,'seed':fixture.seed,'functional_passed':False,
                    'identity_verified':False,'failures':['infrastructure: '+repr(error)],'input_tokens':None,'output_tokens':None,'total_tokens':None}
                if isinstance(error,IdentityFailure):result['identity']={'status':'failed','identity_verified':False,'reason':str(error)}
                folder=root/f'{fixture.id}-{fixture.environment}-{_tasks.SEEDS[fixture.id].index(fixture.seed)}.controller';folder.mkdir(exist_ok=True)
                (folder/'result.json').write_text(json.dumps(result,indent=2)+'\n')
            results.append(result)
            print(json.dumps({key:result.get(key) for key in ('task','environment','seed','functional_passed','failures','input_tokens','output_tokens','total_tokens')}),flush=True)
            if result.get('verification',{}).get('hard_stop'):hard=True
            summary=summarize(results);summary['elapsed_seconds']=time.monotonic()-started
            if summary['identity_gate']['status']=='failed':hard=True
            (root/'summary.json').write_text(json.dumps(summary,indent=2)+'\n')
            if not hard:
                next_fixture=next(queue,None)
                if next_fixture is not None:submit(next_fixture)
    summary=summarize(results)
    print(json.dumps(summary),flush=True)
    if not summary['overall_passed']:raise SystemExit(1)


if __name__=='__main__':main()
