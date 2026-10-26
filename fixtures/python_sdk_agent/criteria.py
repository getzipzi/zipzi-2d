"""Independent public Runtime/lifecycle predicates shared by fresh and supplemental grading."""
import json


def native_lifecycle(events):
    def operation(event):
        if event.get('path')=='recover_call':return event.get('inputs',{}).get('method',{}).get('callable')
        return event.get('path')
    def key(identity):return json.dumps(identity,sort_keys=True)
    returns=[e for e in events if e.get('kind')=='public_return']
    positions={id(event):index for index,event in enumerate(events)}
    created=[e for e in returns if operation(e)=='Sessions.create' and e.get('return_type')=='Session']
    groups={};client_runtime={}
    for event in returns:
        if event.get('return_type')=='Session':
            runtime=event['value']['session_id']['runtime']
            client_runtime[(event['pid'],event.get('runtime_identity'))]=runtime
    for event in created:groups.setdefault(event['value']['session_id']['runtime'],[]).append(event)
    closes=[e for e in returns if operation(e)=='Session.close' and e.get('return_type')=='SessionInfo']
    closed={key(e['value']['id']) for e in closes}
    observations=[]
    for event in events:
        if event.get('kind')=='registry':
            sessions=event['sessions'];client=event['runtime']
        elif event.get('kind')=='public_return' and operation(event)=='Sessions.list' and isinstance(event.get('value'),list):
            sessions=event['value'];client=event.get('runtime_identity')
        else:continue
        runtime=sessions[0]['id']['runtime'] if sessions else client_runtime.get((event['pid'],client))
        if runtime is not None:observations.append((positions[id(event)],runtime,{key(v['id']) for v in sessions}))
    long_groups={runtime:rows for runtime,rows in groups.items() if len(rows)>=40}
    closed_groups={runtime:rows for runtime,rows in long_groups.items() if all(key(e['value']['session_id']) in closed for e in rows)}
    restored=[];baselines={}
    for runtime,rows in closed_groups.items():
        first=min(positions[id(e)] for e in rows);baseline=set()
        # Replay the observed registry and successful open/attach/close calls
        # before the first creation, so a Local fixture opened after Runtime
        # construction is part of the real starting registry.
        snapshots={index:ids for index,host,ids in observations if host==runtime}
        for index,event in enumerate(events[:first]):
            if index in snapshots:baseline=set(snapshots[index])
            if event.get('kind')!='public_return':continue
            if event.get('return_type')=='Session' and event['value']['session_id']['runtime']==runtime:
                baseline.add(key(event['value']['session_id']))
            if operation(event)=='Session.close' and event.get('return_type')=='SessionInfo' and event['value']['id']['runtime']==runtime:
                baseline.discard(key(event['value']['id']))
        required={key(e['value']['session_id']) for e in rows}
        last_close=max(positions[id(e)] for e in closes if key(e['value']['id']) in required)
        if any(ids==baseline for index,host,ids in observations if host==runtime and index>last_close):restored.append(runtime)
        baselines[runtime]=sorted(baseline)
    return {'forty_sessions_created':bool(long_groups),'one_runtime_for_forty_sessions':bool(long_groups),
            'forty_sessions_closed':bool(closed_groups) and len(closed)>=41,'registry_restored':bool(restored)}, {
            'actual_created_sessions':len(created),'sessions_per_native_runtime':{host:len(rows) for host,rows in groups.items()},
            'restored_native_runtimes':restored,'registry_before_first_creation':baselines}



def ordinary_failure(events):
    return any(event.get('kind')=='public_exception' and event.get('code')=='STALE_FIELD'
               and event.get('edit_state')=='aborted' and event.get('edit_identity') is not None
               for event in events)

