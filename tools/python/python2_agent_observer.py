"""External owner of observation logs and Local fixture decisions."""
from __future__ import annotations
import json
from pathlib import Path
import socket
import threading

ROOT=Path(__file__).resolve().parents[2]


class ObservationChannel:
    def __init__(self,private:Path):
        self.private=private
        self.manifest=json.loads((private/'manifest.json').read_text())
        self.data=json.loads((private/'verifier-task.json').read_text())
        (private/'observer-call.json').write_text('null\n')
        calls=private/'shell-events.ndjson'
        self.call_index=1+sum(1 for _ in calls.open()) if calls.exists() else 1
        self.parent,self.child=socket.socketpair()
        self.thread=threading.Thread(target=self.serve,daemon=True)
        self.thread.start()

    def serve(self):
        stream=self.parent.makefile('rw')
        try:
            for line in stream:
                event=json.loads(line)
                event['shell_call_index']=self.call_index
                marker=self.private/'observer-call.json'
                if event['kind']=='public_call_begin':marker.write_text(json.dumps(event)+'\n')
                elif event['kind'] in {'public_return','observer_finished'}:marker.write_text('null\n')
                with (self.private/'observer-events.ndjson').open('a') as log:log.write(json.dumps(event)+'\n')
                answer={};state_path=self.private/'observer-state.json'
                state=json.loads(state_path.read_text()) if state_path.exists() else {'events':[],'mutations':[]}
                task=self.manifest['task'];value=event.get('value') or {}
                if not isinstance(value,dict):value={}
                environment=self.manifest['environment']
                mutation=None
                if environment=='local' and task=='G03' and event.get('domain')=='meshes' and event.get('path','').endswith('.list') and value.get('next_cursor') is not None and value.get('source',{}).get('edit') is None:
                    mutation='source_changed_between_pages'
                if environment=='local' and task=='G04' and event.get('path')=='Session.plan' and event.get('return_type')=='Plan':mutation='plan_source_changed'
                identity=[event.get('pid'),event.get('session'),mutation]
                if mutation and identity not in state['mutations']:
                    state['mutations'].append(identity)
                    answer={'command':'publish_fixture_parameter','event':mutation,'document_id':self.data['document_id']}
                if event.get('kind')=='fixture_publication':
                    before,after=event['before'],event['after']
                    if before['version']==after['version'] or after['history'][0]!=before['history'][0]+1:
                        raise ValueError('Fixture publication lacks a real version/history transition')
                    state['events'].append(event['event'])
                if task=='G04' and event.get('kind')=='public_exception' and event.get('domain')=='meshes' and event.get('edit_state')=='aborted':
                    state['events'].append('ordinary_write_failed')
                state_path.write_text(json.dumps(state)+'\n')
                stream.write(json.dumps(answer)+'\n');stream.flush()
        except (BrokenPipeError,ConnectionResetError):pass
        except Exception as error:
            (self.private/'observer-error').write_text(repr(error))
        finally:stream.close();self.parent.close()

    def close(self):
        self.child.close();self.thread.join(timeout=5)
        if self.thread.is_alive():
            self.parent.shutdown(socket.SHUT_RDWR)
            self.thread.join(timeout=1)


def controller_events(private:Path)->set[str]:
    path=private/'observer-state.json'
    return set(json.loads(path.read_text())['events']) if path.exists() else set()
