"""Host-side model/service evidence and cohort consistency, never model claims.

The optional pinned launcher wraps the exact frozen Codex executable and writes
transport metadata into the private controller directory. The host launcher is
trusted, like the CLI emitting lifecycle events; this is not a remote-signature
verification protocol. A configured model name alone is never evidence.
"""
from __future__ import annotations
import hashlib
import json
import os
from pathlib import Path
from uuid import uuid4

LIFECYCLE_EVENTS=frozenset({'session.started','thread.started','turn.started','turn.completed','service.identity'})
IDENTITY_POLICY={
    'version':'p3a-service-identity-1','missing_evidence':'unverified',
    'mismatch_or_drift':'failed; stop dispatch','configured_model_is_evidence':False,
    'sources':['runner lifecycle event roots','private run-bound transport metadata from a pinned host launcher'],
    'sidecar_binding':['run_id','configuration_sha256','runner_executable_sha256','requested_model'],
    'cohort_consistency':['resolved_model','service_revision','runner_version','runner_executable_sha256','identity_launcher_sha256','configuration_sha256']}


class IdentityFailure(ValueError):
    """A frozen executable or explicit service identity changed."""


def sha(path:Path)->str:return hashlib.sha256(path.read_bytes()).hexdigest()


def launcher_spec(path:Path|None)->dict|None:
    if path is None:return None
    resolved=path.resolve(strict=True)
    if not resolved.is_file() or not os.access(resolved,os.X_OK):raise ValueError('Identity launcher must be an executable file')
    return {'path':str(resolved),'sha256':sha(resolved)}


def prepare_identity(private:Path,config:dict)->tuple[dict,dict]:
    binding={'run_id':str(uuid4()),'configuration_sha256':config['configuration_sha256'],
        'runner_executable_sha256':config['runner']['executable_sha256'],'requested_model':config['model']['id']}
    request=dict(schema=1,**binding,result_path=str(private/'service-identity.json'))
    (private/'identity-request.json').write_text(json.dumps(request,indent=2)+'\n')
    environment={'ZIPZI_AGENT_IDENTITY_REQUEST':str(private/'identity-request.json'),
                 'ZIPZI_AGENT_IDENTITY_RESULT':str(private/'service-identity.json')}
    return binding,environment


def runner_fingerprint(config:dict)->dict:
    runner=config['runner'];path=Path(runner['executable'])
    launcher=runner.get('identity_launcher');error=None
    try:
        actual=sha(path);actual_launcher=None if launcher is None else sha(Path(launcher['path']))
    except OSError as failure:
        actual=actual_launcher=None;error=str(failure)
    return {'version':runner['version'],'executable_sha256':actual,'identity_launcher_sha256':actual_launcher,
        'verified':error is None and actual==runner['executable_sha256'] and (launcher is None or actual_launcher==launcher['sha256']),
        'error':error}


def identity_command(command:list[str],config:dict)->list[str]:
    launcher=config['runner'].get('identity_launcher')
    return command if launcher is None else [launcher['path'],*command]


def evaluate(events:list[dict],returncode:int,*,expected_model='gpt-6.1-sol',
             evidence_path:Path|None=None,binding:dict|None=None,allow_sidecar=False,runner_verified=True)->dict:
    completed=returncode==0 and any(e.get('type')=='turn.completed' for e in events)
    values={'resolved_model':set(),'service_revision':set()};failures=[];sources=[]
    def collect(record,source):
        found=False
        for key in values:
            if key not in record:continue
            value=record[key]
            if value is None or isinstance(value,str) and not value.strip():continue
            if not isinstance(value,str):failures.append('invalid_'+key);continue
            values[key].add(value);found=True
        if found:sources.append(source)
    for event in events:
        if event.get('type') in LIFECYCLE_EVENTS:collect(event,'runner_event:'+event['type'])
    if evidence_path is not None and evidence_path.exists():
        try:
            if not allow_sidecar:raise ValueError('unconfigured identity sidecar')
            if evidence_path.is_symlink() or evidence_path.stat().st_size>65536:raise ValueError('unsafe identity sidecar')
            evidence=json.loads(evidence_path.read_text())
            if not isinstance(evidence,dict) or evidence.get('schema')!=1 or evidence.get('source')!='transport_metadata':raise ValueError('unsupported identity provenance')
            if binding is None or any(evidence.get(key)!=binding[key] for key in IDENTITY_POLICY['sidecar_binding']):raise ValueError('identity evidence belongs to another run/configuration')
            collect(evidence,'pinned_launcher:'+sha(evidence_path))
        except (OSError,ValueError,KeyError,TypeError) as error:failures.append('identity_evidence: '+str(error))
    if not runner_verified:failures.append('runner_executable_drift')
    for key,observed in values.items():
        if len(observed)>1:failures.append(key+'_drift_within_run')
    model=next(iter(values['resolved_model'])) if len(values['resolved_model'])==1 else None
    revision=next(iter(values['service_revision'])) if len(values['service_revision'])==1 else None
    if model is not None and model!=expected_model:failures.append('resolved_model_mismatch')
    verified=completed and bool(model and revision) and not failures
    result={'service_completed':completed,'identity_verified':verified,
        'status':'failed' if failures else 'ready' if verified else 'unverified',
        'reason':'; '.join(failures) if failures else None if verified else 'service incomplete or actual model/service metadata unavailable',
        'failures':failures,'evidence_sources':sorted(set(sources))}
    if model is not None:result['resolved_model']=model
    if revision is not None:result['service_revision']=revision
    if binding is not None:result['run_id']=binding['run_id']
    return result


def cohort_identity(results:list[dict])->dict:
    pairs=set();runners=set();configs=set();run_ids=[];missing=[];failures=[]
    for row in results:
        label=f"{row.get('task')}/{row.get('environment')}/{row.get('seed')}"
        identity=row.get('identity',{});runner=row.get('runner_fingerprint',{})
        model=identity.get('resolved_model');revision=identity.get('service_revision')
        if model is not None and model!='gpt-6.1-sol':failures.append(label+':resolved_model_mismatch')
        if model and revision:pairs.add((model,revision))
        if runner.get('version') and runner.get('executable_sha256'):
            runners.add((runner['version'],runner['executable_sha256'],runner.get('identity_launcher_sha256')))
        else:missing.append(label+':runner')
        if row.get('configuration'):configs.add(row['configuration'])
        else:missing.append(label+':configuration')
        if identity.get('run_id'):run_ids.append(identity['run_id'])
        else:missing.append(label+':run_binding')
        if identity.get('status')=='failed' or runner.get('verified') is False:failures.append(label+':identity_failed')
        if not (row.get('identity_verified') and identity.get('identity_verified') and model and revision and runner.get('verified')):missing.append(label+':actual_identity')
    if len(pairs)>1:failures.append('model_or_service_revision_drift_across_runs')
    if len(runners)>1:failures.append('runner_or_identity_launcher_drift_across_runs')
    if len(configs)>1:failures.append('configuration_drift_across_runs')
    if len(set(run_ids))!=len(run_ids):failures.append('identity_evidence_reused_across_runs')
    verified=bool(results) and not missing and not failures
    return {'verified':verified,'status':'failed' if failures else 'verified' if verified else 'unverified',
            'failures':sorted(set(failures)),'missing':missing,
            'resolved_identities':[{'resolved_model':model,'service_revision':revision} for model,revision in sorted(pairs)]}
