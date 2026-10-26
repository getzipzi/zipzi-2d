"""Independent installed-SDK verifier. Never imports or runs participant scripts."""
from __future__ import annotations
from dataclasses import fields
from collections.abc import Mapping
import importlib.util
import hashlib
import json
from pathlib import Path
import sys
from uuid import UUID,uuid5

import zipzi

_spec=importlib.util.spec_from_file_location('zipzi_agent_criteria',Path(__file__).with_name('criteria.py'))
_criteria=importlib.util.module_from_spec(_spec);_spec.loader.exec_module(_criteria)


def plain(value):
    if hasattr(value,'__dataclass_fields__'):return {field.name:plain(getattr(value,field.name)) for field in fields(value)}
    if isinstance(value,(tuple,list)):return [plain(item) for item in value]
    if isinstance(value,Mapping):return {key:plain(item) for key,item in value.items()}
    if isinstance(value,bytes):return list(value)
    return value


def inspect(session):
    result={'info':plain(session.info()),'document_id':session.modeling.records(zipzi.modeling.ModelingQuery()).value.document_id,
            'meshes':{},'images':{},'parameters':{},'bindings':{}}
    for domain in ('meshes','resources','parameters','bindings'):
        collection=getattr(session,domain);cursor=None
        while True:
            page=collection.list(cursor=cursor)
            for item in page.items:
                result['images' if domain=='resources' else domain][item.id]=plain(collection.get(item.id).value)
            cursor=page.next_cursor
            if cursor is None:break
    return result


def observed_checks(data,task,private):
    path=private/'observer-events.ndjson'
    events=[json.loads(line) for line in path.read_text().splitlines()] if path.exists() else []
    checks={};hard=[];gaps=[]
    finished=[e for e in events if e['kind']=='observer_finished']
    checks['observed_workflow_completed']=bool(finished) and all(e['intact'] for e in finished) and finished[-1]['exit_status']==0
    checks['no_unknown_recovery_left']=not any(e.get('pending_recovery') for e in finished)
    started={e['pid'] for e in events if e['kind']=='observer_started'}
    if started-{e['pid'] for e in finished}:gaps.append('A controlled Python invocation did not finish')
    if not finished:gaps.append('No controlled Python workflow completed')
    all_states=[e for e in events if e['kind']=='state']
    returns=[e for e in events if e['kind']=='public_return']
    created=[e for e in returns if (e.get('path')=='Sessions.create' or e.get('path')=='recover_call' and e.get('inputs',{}).get('method',{}).get('callable')=='Sessions.create') and e.get('return_type')=='Session']
    created_ids=[e['value']['session_id'] for e in created]
    states=[e for e in all_states if e['state']['document_id']==data['document_id'] or task=='G06' and e['state']['info']['id'] in created_ids]
    errors=[e for e in events if e['kind']=='public_exception']
    initial=[e for e in states if e['phase']=='session_acquired']
    formal=[e for e in states if e['phase'] in {'after_edit','after_receipt','before_close','before_runtime_close'}]
    checks['initial_and_final_states_captured']=bool(initial) and bool(formal)
    if not states:gaps.append('No independently observed Session states')
    saved=[e for e in returns if e.get('return_type')=='SaveReceipt' and e.get('value',{}).get('durable') is True]
    checks['durable_receipt_observed']=bool(saved)
    if task!='G06':
        deltas=[]
        for event in formal:
            state=event['state']
            candidates=[e for e in initial if e['state']['info']['id']==state['info']['id']]
            if candidates:
                delta=state['info']['history'][0]-candidates[0]['state']['info']['history'][0]
                deltas.append((delta,state['info']['history'][1]))
        wanted=2 if task in {'G03','G04','G05'} else 1
        checks['history_delta']=(wanted,0) in deltas
        if task in {'G07','G08'} and any(delta>wanted for delta,_ in deltas):hard.append('duplicate_write')
    if task in {'G01','G03','G04'}:
        target=data.get('mesh_id')
        if target is None:
            # UUID is private in G01/G03; obtain it only from the external manifest.
            target=json.loads((private/'manifest.json').read_text())['hidden']['target_id']
        for event in formal:
            state=event['state']
            if any(value['name']!='Face' for key,value in state['meshes'].items() if key!=target):hard.append('wrong_object_modified')
        for event in all_states:
            state=event['state']
            if state['document_id']!=data['document_id'] and any(v['name']!='Face' for v in state['meshes'].values()):hard.append('wrong_object_modified')
        if task=='G01':checks['other_document_observed']=any(e['state']['document_id']!=data['document_id'] for e in all_states)
    if task=='G02':
        samples=[e.get('value') for e in returns if isinstance(e.get('value'),dict) and 'source' in e['value'] and isinstance(e['value'].get('value'),dict) and 'frames' in e['value']['value']]
        def correct(sample):
            frames=sample['value']['frames']
            if not frames or not frames[0]['drawables']:return False
            return all(abs(a-b)<1e-6 for point,base in zip(frames[0]['drawables'][0]['positions'],data['positions']) for a,b in zip(point,[base[0]+7.5,base[1]]))
        checks['candidate_pose_observed']=any(s['source']['edit'] is not None and correct(s) for s in samples)
        checks['formal_pose_observed']=any(s['source']['edit'] is None and correct(s) for s in samples)
    if task=='G03':
        pages=[e for e in returns if e.get('domain')=='meshes' and e.get('path','').endswith('.list') and isinstance(e.get('value'),dict) and 'items' in e['value']]
        groups={}
        for event in pages:
            value=event['value'];key=(event['pid'],json.dumps(value['source'],sort_keys=True),event.get('inputs',{}).get('name'))
            group=groups.setdefault(key,{'ids':set(),'complete':False})
            group['ids'].update(item['id'] for item in value['items'])
            if value['next_cursor'] is None:group['complete']=True
        checks['consistent_complete_page_scan']=any(g['complete'] and len(g['ids'])==data['objects'] for g in groups.values())
        checks['stale_page_rejected']=any(e.get('code')=='STALE_SOURCE' and e.get('path','').endswith('.list') for e in errors)
    if task=='G04':
        checks['stale_plan_rejected']=any(e.get('code') in {'STALE_SOURCE','STALE_PLAN'} for e in errors)
        failed={e.get('edit_identity') for e in errors if e.get('code')=='STALE_FIELD' and e.get('edit_state')=='aborted' and e.get('edit_identity') is not None}
        checks['ordinary_failure_aborted']=_criteria.ordinary_failure(events)
        checks['fresh_edit_after_failure']=any(e.get('edit_identity') is not None and e['edit_identity'] not in failed and e.get('path')=='Edit.__exit__' for e in returns)
        checks['plan_report_observed']=any(e.get('path')=='Plan.report' and isinstance(e.get('value'),dict) and 'cost' in e['value'] and 'losses' in e['value'] for e in returns)
    if task=='G05':
        pixels=[];identities=[]
        for event in states:
            images=event['state']['resources']
            if len(images)==1:
                key,value=next(iter(images.items()));pixel=value.get('pixels',{}).get('rgba')
                if pixel is not None and (not pixels or pixels[-1]!=pixel):pixels.append(pixel);identities.append(key)
        required=[data['rgba_initial'],data['rgba_final'],data['rgba_initial'],data['rgba_final']]
        checks['undo_redo_pixels_observed']=any(pixels[i:i+4]==required for i in range(max(0,len(pixels)-3)))
        checks['image_identity_preserved']=bool(identities) and len(set(identities))==1
    if task=='G06':
        lifecycle,_=_criteria.native_lifecycle(events)
        checks.update(lifecycle)
        checks['unsaved_close_refused']=any(e.get('path')=='Session.close' and e.get('error_type')!='OutcomeUnknown' for e in errors)
    if task in {'G07','G08'}:
        checks['unknown_reply_observed']=any(e.get('error_type')=='OutcomeUnknown' for e in errors)
        checks['recovered_receipt_observed']=any(e.get('return_type')=='Receipt' for e in returns) or any(e.get('path')=='Edit.__exit__' for e in returns)
        if task=='G07':checks['created_id_return_type']=any(e.get('return_type')=='str' and e.get('value')==data['mesh_id'] for e in returns)
    checks['observer_integrity']=bool(started) and not (private/'observer-error').exists()
    return checks,sorted(set(hard)),gaps


def verify(task_path:Path,private_path:Path,output:Path):
    data=json.loads(task_path.read_text());manifest=json.loads((private_path/'manifest.json').read_text())
    task=manifest['task'];hidden=manifest['hidden'];checks={};hard_stop=[];evidence={};coverage_gaps=[]
    folder=Path(data['host_folder'])/'result'
    projects=list(folder.glob('*.zipzi.json')) if folder.is_dir() else []
    checks['saved_project']=len(projects)==1
    if projects:
        with zipzi.local() as runtime:
            session=runtime.sessions.open(projects[0].resolve()).wait()
            state=inspect(session);evidence['saved_state']=state
            checks['document_identity']=state['document_id']==data['document_id'] if task!='G06' else str(UUID(state['document_id']))==state['document_id']
            meshes=state['meshes'];target=hidden['target_id']
            if task in {'G01','G03','G04'}:
                expected_ids={target,*hidden['other_ids']}
                expected_ids.update(str(uuid5(UUID(data['document_id']),f'mesh-{i}')) for i in range(3,data['objects']))
                checks['complete_mesh_set']=set(meshes)==expected_ids
                checks['target_name']=meshes.get(target,{}).get('name')==data['final_name']
                checks['other_names']=all(value['name']=='Face' for key,value in meshes.items() if key!=target)
                checks['geometry_unchanged']=all(value['geometry']['vertex_ids']==data['vertex_ids'] and value['geometry']['positions']==data['positions'] and value['geometry']['uvs']==data['uvs'] and value['geometry']['triangles']==data['triangles'] for value in meshes.values())
                if not checks['other_names']:hard_stop.append('wrong_object_modified')
            elif task in {'G02','G07','G08'}:
                checks['single_mesh_identity']=set(meshes)=={data['mesh_id']}
                if len(meshes)>1:hard_stop.append('duplicate_write')
                geometry=meshes.get(data['mesh_id'],{}).get('geometry',{})
                checks['vertex_ids']=geometry.get('vertex_ids')==data['vertex_ids']
                checks['positions']=geometry.get('positions')==data['positions']
                checks['uvs']=geometry.get('uvs')==data['uvs']
                checks['triangles']=geometry.get('triangles')==data['triangles']
                if task!='G02':checks['target_name']=meshes.get(data['mesh_id'],{}).get('name')==data['final_name']
                else:
                    checks['single_parameter']=set(state['parameters'])=={data['parameter_id']}
                    checks['single_binding']=set(state['bindings'])=={data['binding_id']}
                    sample=session.modeling.sample(({data['parameter_id']:data['pose']},)).value
                    evidence['sample']=plain(sample)
                    checks['cpu_pose']=len(sample.frames)==1 and len(sample.frames[0].drawables)==1 and abs(sample.frames[0].drawables[0].positions[0][0]-hidden['expected_sample_x'])<1e-6
            elif task=='G05':
                checks['one_image']=len(state['images'])==1
                checks['pixels']=all(value['pixels']['rgba']==hidden['expected_pixels'] and value['pixels']['width']==1 and value['pixels']['height']==1 for value in state['images'].values())
            elif task=='G06':
                checks['saved_last_mesh']=len(meshes)==1
            session.close()
    observed,hard,coverage=observed_checks(data,task,private_path)
    checks.update(observed);hard_stop.extend(hard)
    coverage_gaps.extend(coverage)
    evidence['checks']=checks;evidence['hard_stop']=hard_stop
    digest=hashlib.sha256(json.dumps(evidence,sort_keys=True,default=str).encode()).hexdigest()
    result={'verified':bool(checks) and all(checks.values()) and not hard_stop,'checks':checks,
            'hard_stop':hard_stop,'evidence_digest':digest,'evidence':evidence,
            'coverage_complete':not coverage_gaps,'coverage_gaps':coverage_gaps}
    output.write_text(json.dumps(result,indent=2,default=str)+'\n')
    return result


if __name__=='__main__':
    try:
        result=verify(Path(sys.argv[1]),Path(sys.argv[2]),Path(sys.argv[3]))
    except Exception as error:
        result={'verified':False,'error':type(error).__name__+': '+str(error),'evidence_digest':None}
        Path(sys.argv[3]).write_text(json.dumps(result)+'\n')
    print(json.dumps({key:value for key,value in result.items() if key!='evidence'}))
    raise SystemExit(0 if result['verified'] else 1)
