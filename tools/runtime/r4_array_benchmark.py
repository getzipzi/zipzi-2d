#!/usr/bin/env python3
"""Measure installed R4 large arrays; timing and wire probes use separate calls."""
import argparse
from collections import Counter
from contextlib import contextmanager
from dataclasses import asdict
import gc
import json
import math
import os
from pathlib import Path
import platform
import resource
import statistics
import subprocess
import sys
import tempfile
import time


def rss(pid):
    return int(subprocess.check_output(['ps','-o','rss=','-p',str(pid)],text=True).strip())*1024


@contextmanager
def runtime(mode,host):
    import zipzi
    if mode=='local':
        with zipzi.local() as value:yield value,None
        return
    with tempfile.TemporaryDirectory(prefix='zzr4-',dir='/tmp') as folder:
        env={k:v for k,v in os.environ.items() if not k.startswith('ZIPZI_') and k not in {'PYTHONPATH','PYTHONHOME'}}
        env.update(ZIPZI_RUNTIME_DIR=folder,ZIPZI_RUNTIME_PROFILE='python2')
        process=subprocess.Popen([host],env=env,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)
        try:
            deadline=time.monotonic()+30
            while True:
                try:value=zipzi.connect(endpoint=Path(folder)/'python2.sock');break
                except (FileNotFoundError,ConnectionRefusedError):
                    if process.poll() is not None:raise RuntimeError(process.stderr.read().decode())
                    if time.monotonic()>deadline:raise TimeoutError('host startup')
                    time.sleep(.005)
            with value:yield value,process.pid
        finally:
            process.terminate();process.wait(timeout=15)


def worker(args):
    import zipzi
    from zipzi import meshes,bindings,parameters,modeling as m
    n=args.side
    corners=((0.,0.),(float(n-1),0.),(float(n-1),float(n-1)),(0.,float(n-1)))
    rectangle=meshes.Geometry((0,1,2,3),corners,((0.,0.),(1.,0.),(1.,1.),(0.,1.)),((0,1,2),(0,2,3)))
    rows={}
    with runtime(args.worker,args.host) as (rt,pid):
        initial_rss=rss(os.getpid())
        host_initial_rss=rss(pid) if pid else None
        s=rt.sessions.create('79800000-0000-4000-8000-000000000001')
        with s.edit('measured fixture') as edit:
            mesh=edit.meshes.create(meshes.MeshSpec('Grid',rectangle))
            parameter=edit.parameters.create(parameters.ParameterSpec('P',0.,1.,0.))
            binding=edit.bindings.create(bindings.MeshBindingSpec(mesh,(bindings.Axis(parameter,(0.,1.)),),(bindings.Keyform((0.,),corners),bindings.Keyform((1.,),tuple((x+2.,y) for x,y in corners)))))
            edit.apply(m.ModelingCommandRemeshRectangleGrid(id=mesh,columns=n-1,rows=n-1))
        before=s.info()
        fixtures={'mesh':lambda:s.meshes.get(mesh),'binding':lambda:s.bindings.get(binding),'sample_4':lambda:s.modeling.sample(({parameter:0.},{parameter:.25},{parameter:.75},{parameter:1.}))}
        for name,invoke in fixtures.items():
            times=[]
            for i in range(args.samples+4):
                start=time.perf_counter_ns();value=invoke();elapsed=(time.perf_counter_ns()-start)/1e6
                if i>=4:times.append(elapsed)
                if name=='mesh':assert len(value.geometry.positions)==n*n
                elif name=='binding':assert len(value.keyforms[0].positions)==n*n
                else:assert value.frames[-1].drawables[0].positions[-1]==(float(n+1),float(n-1))
                del value
            # Count actual socket writes/reads in an additional untimed operation.
            counts=Counter()
            if args.worker=='connected':
                import zipzi.backends.connected as transport
                exchange=transport._exchange
                class Counted:
                    def __init__(self,stream):self.stream=stream
                    def sendall(self,data):counts['request_wire_bytes']+=len(data);return self.stream.sendall(data)
                    def recv(self,length):
                        data=self.stream.recv(length);counts['response_wire_bytes']+=len(data);return data
                def measured(stream,body):
                    counts['roundtrips']+=1
                    return exchange(Counted(stream),body)
                transport._exchange=measured
                try:value=invoke()
                finally:transport._exchange=exchange
            else:
                def profile(frame,event,value):
                    if event=='c_call' and type(getattr(value,'__self__',None)).__name__=='_Runtime':counts['native_calls']+=1
                sys.setprofile(profile)
                try:value=invoke()
                finally:sys.setprofile(None)
            del value;gc.collect()
            ordered=sorted(times)
            rows[name]=dict(samples_ms=times,p50_ms=statistics.median(times),p95_ms=ordered[math.ceil(len(times)*.95)-1],counts=dict(counts),python_rss_bytes=rss(os.getpid()),host_rss_bytes=rss(pid) if pid else None,usage=asdict(rt.resources.usage()))
        assert s.info()==before
        assert all(rows[n]['usage'][k]==0 for n in rows for k in ('prepared_images','plans','preparing_bytes'))
    return dict(mode=args.worker,side=n,vertices=n*n,triangles=2*(n-1)**2,samples=args.samples,initial_python_rss_bytes=initial_rss,initial_host_rss_bytes=host_initial_rss,python_peak_rss_bytes=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,host_peak_rss_bytes=resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss if args.worker=='connected' else None,cases=rows)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--python',type=Path);p.add_argument('--host',required=True);p.add_argument('--output',type=Path,required=True);p.add_argument('--worker',choices=('local','connected'));p.add_argument('--side',type=int,default=256);p.add_argument('--samples',type=int,default=32);p.add_argument('--pairs',type=int,default=5)
    args=p.parse_args()
    if args.worker:
        args.output.write_text(json.dumps(worker(args),indent=2)+'\n');return
    if args.pairs<5 or args.samples<32:p.error('At least five pairs and 32 measured samples required')
    args.output.mkdir(exist_ok=False,parents=True)
    commands=[];runs=[]
    for pair in range(args.pairs):
        for mode in (('local','connected') if pair%2==0 else ('connected','local')):
            output=args.output/f'{pair+1}-{mode}.json'
            command=[str(args.python),'-I',str(Path(__file__).resolve()),'--worker',mode,'--host',args.host,'--output',str(output),'--side',str(args.side),'--samples',str(args.samples)]
            print('start',pair+1,mode,flush=True)
            subprocess.run(command,check=True);commands.append(command);runs.append(json.loads(output.read_text()))
    summary={}
    for mode in ('local','connected'):
        summary[mode]={}
        for case in runs[0]['cases']:
            rows=[r['cases'][case] for r in runs if r['mode']==mode]
            summary[mode][case]={metric:dict(median=statistics.median(v[metric] for v in rows),range=[min(v[metric] for v in rows),max(v[metric] for v in rows)]) for metric in ('p50_ms','p95_ms')}
            summary[mode][case]['counts']=rows[0]['counts']
    (args.output/'results.json').write_text(json.dumps(dict(schema=1,status='passed',platform=platform.platform(),python=sys.version,scope='installed Release wheels; per-process 4 warmup +32 measured calls; wire/native probes in separate untimed calls; macOS ru_maxrss bytes; no baseline algorithm speedup claim',commands=commands,summary=summary,runs=runs),indent=2)+'\n')

if __name__=='__main__':main()
