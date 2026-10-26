"""Trusted script bootstrap, supplied by the external shell through -c.

Normal installed zipzi code is observed without modifying it. Events and fixture
commands travel on an inherited private socket, never stdout or packet files.
The model's script has its own globals and no fixture API. This is instrumentation
for cooperative SDK evaluation, not isolation against hostile Python introspection.
"""
from __future__ import annotations
from collections.abc import Mapping
from dataclasses import fields,is_dataclass
from enum import Enum
import hashlib
import inspect
import json
import os
from pathlib import Path
import runpy
import socket
import sys
import traceback

import zipzi


def plain(value):
    if value is None or type(value) in (str,int,float,bool):return value
    if isinstance(value,Enum):return value.value
    if isinstance(value,Path):return str(value)
    if isinstance(value,bytes):return list(value) if len(value)<=64 else {'bytes':len(value),'sha256':hashlib.sha256(value).hexdigest()}
    if isinstance(value,zipzi.Session):return {'session_id':plain(value.id)}
    if isinstance(value,zipzi.Runtime):return {'type':'Runtime','identity':id(value)}
    if callable(value):return {'callable':getattr(value,'__qualname__',type(value).__name__)}
    if is_dataclass(value):return {f.name:plain(getattr(value,f.name)) for f in fields(value)}
    if isinstance(value,Mapping):return {str(k):plain(v) for k,v in value.items()}
    if isinstance(value,(tuple,list)):return [plain(v) for v in value[:512]]
    return {'type':type(value).__name__}


def snapshot(session):
    result={'info':plain(session.info()),'document_id':session.modeling.records(zipzi.modeling.ModelingQuery()).value.document_id}
    for domain in ('meshes','resources','parameters','bindings','scene'):
        collection=getattr(session,domain);cursor=None;values={};summaries={}
        while True:
            page=collection.list(cursor=cursor)
            for item in page.items:
                summaries[item.id]=plain(item)
                if domain!='scene':values[item.id]=plain(collection.get(item.id).value)
            cursor=page.next_cursor
            if cursor is None:break
        result[domain]=values;result[domain+'_summaries']=summaries
    return result


class Observer:
    def __init__(self,stream):
        self.stream=stream.makefile('rw');self.sessions={};self.runtimes={};self.active={};self.codes=set()
        self.sequence=0;self.creation_starts=set();self.recovery_owners={}
        for module in tuple(sys.modules.values()):
            if module is None or not getattr(module,'__name__','').startswith('zipzi'):continue
            for name,value in tuple(vars(module).items()):
                if name.startswith('_'):continue
                if inspect.isfunction(value) and module.__name__=='zipzi' and name in zipzi.__all__:
                    self.codes.add(value.__code__)
                if isinstance(value,type) and value.__module__.startswith('zipzi'):
                    for method,member in vars(value).items():
                        if method.startswith('_') and method not in {'__enter__','__exit__'}:continue
                        member=member.fget if isinstance(member,property) else member
                        if inspect.isfunction(member):self.codes.add(member.__code__)

    def send(self,event,session=None):
        self.sequence+=1;event.update(sequence=self.sequence,pid=os.getpid())
        self.stream.write(json.dumps(event,default=str)+'\n');self.stream.flush()
        answer=json.loads(self.stream.readline())
        if answer.get('command')=='publish_fixture_parameter':
            if session is None:raise RuntimeError('Fixture command has no owning Session')
            actual=session.modeling.records(zipzi.modeling.ModelingQuery()).value.document_id
            if actual!=answer['document_id']:raise RuntimeError('Fixture target document mismatch')
            before=plain(session.info())
            with session.edit('external fixture publication') as edit:
                edit.parameters.create(zipzi.parameters.ParameterSpec('External fixture',0.,1.,0.))
            self.send({'kind':'fixture_publication','event':answer['event'],'before':before,
                       'after':plain(session.info())},session)

    def remember(self,value):
        if value is not None and (inspect.getattr_static(type(value),'pending_request',None) is not None or 'pending_request' in getattr(value,'__dict__',{})):
            self.recovery_owners[id(value)]=value
        if isinstance(value,zipzi.Runtime):self.runtimes[id(value)]=value
        if isinstance(value,zipzi.Session):
            self.sessions[id(value)]=value;self.runtimes[id(value._runtime)]=value._runtime

    def context(self,owner):
        if isinstance(owner,zipzi.Session):return owner,None
        if isinstance(owner,zipzi.Edit):return owner._session,owner
        return getattr(owner,'_session',None),getattr(owner,'_edit',None)

    def capture(self,session,phase):
        try:state=snapshot(session)
        except zipzi.SdkFailure as error:
            if error.code in {'INVALID_REFERENCE','SESSION_NOT_FOUND','INVALID_SESSION'}:return
            self.send({'kind':'observer_error','phase':phase,'error':repr(error)});return
        self.send({'kind':'state','phase':phase,'state':state},session)

    def registry(self,runtime,phase):
        try:self.send({'kind':'registry','phase':phase,'runtime':id(runtime),'sessions':plain(runtime.sessions.list())})
        except zipzi.SdkFailure:pass

    def trace(self,frame,event,arg):
        if event=='call':
            if frame.f_code not in self.codes:return None
            owner=frame.f_locals.get('self');self.remember(owner)
            session,edit=self.context(owner);self.remember(session)
            path=(type(owner).__name__+'.' if owner is not None else '')+frame.f_code.co_name
            outer=not self.active
            self.active[id(frame)]={'path':path,'owner':owner,'session':session,'edit':edit,'outer':outer,'exception':None}
            if outer:
                if type(owner).__name__=='Sessions' and frame.f_code.co_name=='create' and id(owner._runtime) not in self.creation_starts:
                    self.creation_starts.add(id(owner._runtime));self.registry(owner._runtime,'before_creations')
                if isinstance(owner,zipzi.Session) and frame.f_code.co_name=='close':self.capture(owner,'before_close')
                if isinstance(owner,zipzi.Runtime) and frame.f_code.co_name in {'close','__exit__'}:
                    for known in tuple(self.sessions.values()):
                        if known._runtime is owner:self.capture(known,'before_runtime_close')
                    self.registry(owner,'before_runtime_close')
                self.send({'kind':'public_call_begin','path':path,'domain':getattr(owner,'_domain',None)})
            return self.trace
        item=self.active.get(id(frame))
        if item is None:return None
        if event=='exception':
            error=arg[1];item['exception']=error
            if isinstance(error,zipzi.SdkFailure) or isinstance(error,zipzi.OutcomeUnknown):
                self.send({'kind':'public_exception','path':item['path'],'domain':getattr(item['owner'],'_domain',None),
                    'error_type':type(error).__name__,'code':getattr(error,'code',None),
                    'edit_identity':id(item['edit']) if item['edit'] is not None else None,
                    'edit_state':plain(item['edit'].state) if item['edit'] is not None else None,
                    'session':plain(item['session'].id) if item['session'] is not None else None})
        if event=='return':
            self.active.pop(id(frame),None)
            if item['outer']:
                self.remember(arg)
                inputs={k:plain(v) for k,v in frame.f_locals.items() if k not in {'self','__class__'} and not k.startswith('_')}
                # Public argument names only; locals created by implementation are not inputs.
                names=frame.f_code.co_varnames[:frame.f_code.co_argcount+frame.f_code.co_kwonlyargcount]
                inputs={key:value for key,value in inputs.items() if key in names}
                data={'kind':'public_return','path':item['path'],'domain':getattr(item['owner'],'_domain',None),
                      'return_type':type(arg).__name__,'value':plain(arg),'inputs':inputs,
                      'edit_identity':id(item['edit']) if item['edit'] is not None else None,
                      'session':plain(item['session'].id) if item['session'] is not None else None}
                runtime=getattr(item['owner'],'_runtime',None)
                if item['session'] is not None:runtime=item['session']._runtime
                if isinstance(arg,zipzi.Session):runtime=arg._runtime
                data['runtime_identity']=id(runtime) if runtime is not None else None
                self.send(data,item['session'])
                if isinstance(arg,zipzi.Session):self.capture(arg,'session_acquired')
                if isinstance(arg,zipzi.Runtime):self.registry(arg,'runtime_acquired')
                if item['session'] is not None and item['path'].endswith('.__exit__'):
                    self.capture(item['session'],'after_edit')
                if type(arg).__name__ in {'Receipt','SaveReceipt'}:
                    for known in tuple(self.sessions.values()):self.capture(known,'after_receipt')
            return self.trace
        return self.trace


def main():
    descriptor=int(sys.argv[1]);arguments=sys.argv[2:]
    channel=socket.socket(fileno=descriptor);observer=Observer(channel)
    observer.send({'kind':'observer_started'})
    sys.argv=arguments;sys.orig_argv=[sys.executable,*arguments]
    sys.settrace(observer.trace)
    status=0
    try:
        if arguments[0]=='-c':
            sys.argv=['-c',*arguments[2:]]
            exec(compile(arguments[1],'<string>','exec'),{'__name__':'__main__'})
        else:
            path=Path(arguments[0]).resolve();sys.path.insert(0,str(path.parent))
            runpy.run_path(str(path),run_name='__main__')
    except SystemExit as error:status=error.code or 0
    except BaseException:traceback.print_exc();status=1
    finally:
        intact=sys.gettrace()==observer.trace
        sys.settrace(None)
        pending=[]
        for owner in observer.recovery_owners.values():
            if owner.pending_request is not None:pending.append({'type':type(owner).__name__,'pending':plain(owner.pending_request)})
        observer.send({'kind':'observer_finished','intact':intact,'exit_status':status,'pending_recovery':pending})
        observer.stream.close();channel.close()
    raise SystemExit(status)


if __name__=='__main__':main()
