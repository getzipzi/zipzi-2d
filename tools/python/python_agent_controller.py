"""External transparent IPC relay: real publications and cached-reply loss.

It never changes SDK operation bytes or forges domain responses. Faults close a
connection only after the real host produced its response. Same-request recovery
therefore exercises the normal protocol cache, with normal installed packages.
"""
from __future__ import annotations
import json
from pathlib import Path
import socket
import struct
import subprocess
import threading

ROOT=Path(__file__).resolve().parents[2]


def read_exact(stream,count):
    result=bytearray()
    while len(result)<count:
        part=stream.recv(count-len(result))
        if not part:raise EOFError()
        result.extend(part)
    return bytes(result)


def frame(stream):
    header=read_exact(stream,4);size=struct.unpack('>I',header)[0]
    length=size&0x7fffffff
    if not 0<length<=16<<20:raise ValueError('Frame size outside normal protocol budget')
    body=read_exact(stream,length)
    value=json.loads(body[4:4+struct.unpack('>I',body[:4])[0]]) if size&0x80000000 else json.loads(body)
    return header+body,value


class Controller:
    def __init__(self,endpoint:Path,actual:Path,private:Path,python:Path,task:str):
        self.actual=actual;self.private=private;self.python=python;self.task=task
        self.events=set();self.seen_requests=set();self.lock=threading.Lock();self.closed=False
        self.socket=socket.socket(socket.AF_UNIX);self.socket.bind(str(endpoint));self.socket.listen(8)
        endpoint.chmod(0o600)
        self.socket.settimeout(.2);self.thread=threading.Thread(target=self.accept,daemon=True);self.thread.start()

    def record(self,kind,**data):
        with (self.private/'controller-events.ndjson').open('a') as stream:stream.write(json.dumps({'kind':kind,**data})+'\n')

    def mutate(self,event):
        completed=subprocess.run([str(self.python),'-I',str(ROOT/'fixtures/python_sdk_agent/mutate.py'),str(self.private/'verifier-task.json')],capture_output=True,text=True,timeout=30)
        self.record('publication',event=event,returncode=completed.returncode,output=completed.stdout,error=completed.stderr)
        if completed.returncode:raise RuntimeError('External fixture publication failed')
        self.events.add(event)

    def intercept(self,request,response):
        operation=request.get('operation',{});op=operation.get('op');ok=response.get('ok')
        marker=getattr(self,'private',Path('/nonexistent'))/'observer-call.json'
        active=json.loads(marker.read_text()) if marker.exists() else None
        observed=bool(active) if marker.exists() else True
        with self.lock:
            key=(json.dumps(request.get('client'),sort_keys=True),request.get('request_id'))
            replay=key[1] is not None and key in self.seen_requests
            if key[1] is not None:self.seen_requests.add(key)
            if op:
                details={'request_id':key[1],'client':request.get('client'),'replay':replay}
                if op in {'commit','job','session_info','close_session'} and len(json.dumps(ok))<=4096:details['result']=ok
                self.record('host_response',operation=op,error=response.get('error'),terminal=bool(isinstance(ok,dict) and ok.get('result') is not None),**details)
            if replay:return False
            if self.task=='G03' and observed and (active is None or active.get('domain')=='meshes') and op=='collection_page' and 'source_changed_between_pages' not in self.events and isinstance(ok,dict) and ok.get('next_cursor'):
                self.mutate('source_changed_between_pages')
            if self.task=='G04' and observed and op=='plan_authoring' and ok is not None and 'plan_source_changed' not in self.events:
                self.mutate('plan_source_changed')
            if self.task=='G04' and op=='apply_authoring' and response.get('error'):
                self.events.add('ordinary_write_failed')
            event=None
            if self.task=='G07' and op=='apply_authoring' and ok is not None:
                event='create_reply_dropped' if 'create_reply_dropped' not in self.events else 'apply_reply_dropped' if 'apply_reply_dropped' not in self.events else None
            if self.task=='G08' and op=='commit' and ok is not None and 'commit_reply_dropped' not in self.events:event='commit_reply_dropped'
            if self.task=='G08' and op=='job' and isinstance(ok,dict) and ok.get('result') is not None and 'terminal_reply_dropped' not in self.events:event='terminal_reply_dropped'
            if event:
                self.events.add(event);self.record('reply_dropped',event=event,operation=op)
                return True
        return False

    def serve(self,client):
        try:
            with client,socket.socket(socket.AF_UNIX) as host:
                host.connect(str(self.actual));host.settimeout(60);client.settimeout(60)
                while not self.closed:
                    wire,request=frame(client);host.sendall(wire)
                    wire,response=frame(host)
                    if self.intercept(request,response):break
                    client.sendall(wire)
        except (EOFError,OSError):pass
        except Exception as error:self.record('controller_error',error=repr(error))

    def accept(self):
        while not self.closed:
            try:client,_=self.socket.accept()
            except socket.timeout:continue
            except OSError:break
            threading.Thread(target=self.serve,args=(client,),daemon=True).start()

    def close(self):
        self.closed=True;self.socket.close();self.thread.join(timeout=1)
