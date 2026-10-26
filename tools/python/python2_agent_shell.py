"""External, bounded MCP shell for installed-only SDK evaluation packets.

The model sees one shell tool. macOS Seatbelt restricts each command to system
runtime files and its packet; controllers, source trees and credentials are not
readable. This process and its append-only evidence live outside that packet.
"""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import signal
import shlex
import subprocess
import sys
import time
from python2_agent_observer import ObservationChannel,ROOT
from python2_agent_output import OutputPages


def policy(packet: Path, python_base: Path, temporary: Path) -> str:
    def sub(path):return '(subpath '+json.dumps(str(path))+')'
    parents=' '.join('(literal '+json.dumps(str(p))+')' for p in set(packet.parents)|set(python_base.parents))
    readable='(require-any '+sub(packet)+' '+sub(python_base)+' '+parents+')'
    writable='(require-any '+sub(packet/'work')+' '+sub(packet/'output')+' '+sub(packet/'host')+' '+sub(temporary)+' (literal "/dev/null") (literal "/dev/tty"))'
    blocked='(require-any '+ ' '.join(sub(p) for p in ('/Users','/private/tmp','/private/var/folders','/Volumes','/opt'))+')'
    return '\n'.join(['(version 1)','(allow default)',
        '(deny file-read-data (require-all '+blocked+' (require-not '+readable+')))',
        '(deny file-write* (require-not '+writable+'))',
        '(deny network-inbound)',
        '(deny network-outbound (require-not (remote unix-socket)))'])


def execute(command: str, packet: Path, profile: Path, temporary: Path, child_record: Path|None=None, private: Path|None=None) -> dict:
    started=time.monotonic()
    environment={'PATH':str(packet/'environment/bin')+':/usr/bin:/bin:/usr/sbin:/sbin',
                 'HOME':str(packet/'work'),'TMPDIR':str(temporary)+'/',
                 'TMPPREFIX':str(temporary/'shell'),
                 'PYTHONDONTWRITEBYTECODE':'1','PYTHONNOUSERSITE':'1','LC_ALL':'en_US.UTF-8'}
    channel=None;pass_fds=()
    executable=['/bin/zsh','-f','-c',command]
    if command.strip().startswith('sdk-python '):
        arguments=shlex.split(command)
        if private is None or len(arguments)<2 or any(part in {';','&&','||','|','>','>>'} for part in arguments):
            return {'command':command,'returncode':2,'elapsed_seconds':0.,'output':'Use sdk-python work/script.py as a standalone shell call.'}
        channel=ObservationChannel(private);descriptor=channel.child.fileno();pass_fds=(descriptor,)
        executable=[str(packet/'environment/bin/python'),'-I','-c',
                    (ROOT/'fixtures/python_sdk_agent/observe.py').read_text(),str(descriptor),*arguments[1:]]
    with subprocess.Popen(['/usr/bin/sandbox-exec','-f',str(profile),*executable],
            cwd=packet,env=environment,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,
            start_new_session=True,pass_fds=pass_fds) as process:
        if child_record is not None:child_record.write_text(str(process.pid))
        try:
            output,_=process.communicate(timeout=60)
            code=process.returncode
        except subprocess.TimeoutExpired:
            os.killpg(process.pid,signal.SIGKILL);output,_=process.communicate();code=124
        finally:
            if child_record is not None:child_record.unlink(missing_ok=True)
            if channel is not None:channel.close()
    return {'command':command,'returncode':code,'elapsed_seconds':time.monotonic()-started,
            'output':output.decode(errors='replace')}


def serve(packet: Path, private: Path) -> None:
    for name in ('work','output'): (packet/name).mkdir(exist_ok=True)
    temporary=packet/'work/tmp';temporary.mkdir(exist_ok=True)
    base=Path(subprocess.check_output([str(packet/'environment/bin/python'),'-I','-c',
        'import sys;print(sys.base_prefix)'],text=True).strip()).resolve()
    profile=private/'shell.sb';profile.write_text(policy(packet,base,temporary))
    calls=0;started=time.monotonic();pages=OutputPages()
    for line in sys.stdin:
        try:
            request=json.loads(line);method=request.get('method');result={}
            if method=='initialize':
                result={'protocolVersion':request['params']['protocolVersion'],
                        'capabilities':{'tools':{}},'serverInfo':{'name':'sdk-shell','version':'1'}}
            elif method=='tools/list':
                result={'tools':[{'name':'shell','description':'Run a shell command inside this isolated SDK task packet. Execute SDK workflows with sdk-python work/script.py as a standalone call; offline help can use environment/bin/python. Results are paginated to 16384 bytes. When output_complete=false, read_output_command retrieves the next page through this same shell tool. Complete raw output is retained externally. Each command: 60 seconds maximum. Write scripts in work/ and results in output/.',
                    'inputSchema':{'type':'object','properties':{'command':{'type':'string'}},'required':['command'],'additionalProperties':False}}]}
            elif method=='tools/call':
                calls+=1
                command=request['params']['arguments']['command']
                with (private/'tool-attempts.ndjson').open('a') as stream:stream.write(json.dumps({'command':command})+'\n')
                if calls>160 or time.monotonic()-started>900:
                    (private/'budget-failed').write_text('tool_calls' if calls>160 else 'wall_seconds')
                    raise RuntimeError('Frozen run budget exhausted')
                if command.strip().startswith('read-output '):
                    page=pages.read(command.strip().split(None,1)[1])
                    value={'command':command,'returncode':page['returncode'],'elapsed_seconds':0.,'output':page['output']}
                else:
                    value=execute(command,packet,profile,temporary,private/'active-shell-pgid',private)
                    page=pages.first(value)
                value['delivered_output_bytes']=len(json.dumps(page,ensure_ascii=False).encode())
                with (private/'shell-events.ndjson').open('a') as stream:stream.write(json.dumps(value)+'\n')
                oversized=value['delivered_output_bytes']>16384
                if oversized:
                    (private/'budget-failed').write_text('tool_output_bytes_per_call')
                if value['elapsed_seconds']>60:(private/'budget-failed').write_text('seconds_per_tool_call')
                result={'content':[{'type':'text','text':json.dumps(page,ensure_ascii=False)}],
                        'isError':value['returncode']!=0 or oversized}
            elif method=='ping':pass
            elif 'id' not in request:continue
            else:raise ValueError('Unknown MCP method '+str(method))
            response={'jsonrpc':'2.0','id':request['id'],'result':result}
        except Exception as error:
            response={'jsonrpc':'2.0','id':request.get('id'),'error':{'code':-32603,'message':str(error)}}
        print(json.dumps(response),flush=True)


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('packet',type=Path);parser.add_argument('private',type=Path)
    arguments=parser.parse_args();serve(arguments.packet,arguments.private)
