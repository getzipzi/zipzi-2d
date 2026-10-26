"""Direct finite binary adapters from compiled Rust field/layout declarations."""
import keyword
import re
TARGETS=frozenset({'client/python/zipzi/_generated/binary.py'})
PRELUDE='''from __future__ import annotations
from collections.abc import Mapping
from typing import cast
from uuid import UUID
import math
import struct
from .. import _operations as ops, meshes, resources
from . import modeling as m, scene
from .._values import Source,Version
from .._results import Snapshot
from .._values import record, sequence, string, integer, number
from .._extensions import ExtensionValue

MAX_BYTES = 256 << 20
MAX_ITEMS = 1 << 20

def _field(value: object, name: str) -> object:
    return getattr(value, name)

def _mapping(value: object) -> Mapping[object, object]:
    if not isinstance(value, Mapping): raise TypeError("Expected immutable mapping")
    return cast(Mapping[object, object], value)

class Writer:
    def __init__(self) -> None:
        self.data = bytearray()
    def raw(self, value: bytes | bytearray | memoryview) -> None:
        if len(self.data)+len(value) > MAX_BYTES: raise ValueError("Binary payload exceeds byte limit")
        self.data.extend(value)
    def scalar(self, format: str, value: object) -> None:
        if format in {"f", "d"}:
            v = number(value)
            if not math.isfinite(v): raise ValueError("Non-finite binary value")
            self.raw(struct.pack("<"+format, v))
        else:
            self.raw(struct.pack("<"+format, integer(value)))
    def u32(self, value: int) -> None:
        self.scalar("I", value)
    def boolean(self, value: object) -> None:
        if type(value) is not bool: raise TypeError("Expected bool")
        self.raw(b"\\1" if value else b"\\0")
    def text(self, value: object) -> None:
        data = string(value).encode("utf-8")
        self.u32(len(data)); self.raw(data)
    def bytes(self, value: object) -> None:
        if not isinstance(value, bytes): raise TypeError("Expected immutable bytes")
        self.u32(len(value)); self.raw(value)
    def numeric(self, format: str, values: tuple[object, ...]) -> None:
        if len(values) > MAX_ITEMS*4: raise ValueError("Binary array exceeds item limit")
        if len(self.data)+struct.calcsize("<"+format)*len(values)>MAX_BYTES: raise ValueError("Binary payload exceeds byte limit")
        self.raw(struct.pack("<"+str(len(values))+format, *values))
    def extension(self, value: object, depth: int = 0) -> None:
        if depth>128: raise ValueError("Extension nesting exceeds limit")
        if value is None: self.raw(b"\\0")
        elif isinstance(value,bool): self.raw(b"\\1"); self.boolean(value)
        elif isinstance(value,int):
            self.raw(b"\\2" if value<1<<63 else b"\\3"); self.scalar("q" if value<1<<63 else "Q",value)
        elif isinstance(value,float): self.raw(b"\\4"); self.scalar("d",value)
        elif isinstance(value,str): self.raw(b"\\5"); self.text(value)
        elif isinstance(value,Mapping):
            items=_mapping(cast(object,value))
            self.raw(b"\\7"); self.u32(len(items))
            for k,v in items.items(): self.text(k); self.extension(v,depth+1)
        else:
            values=sequence(value); self.raw(b"\\6"); self.u32(len(values))
            for v in values: self.extension(v,depth+1)

class Reader:
    def __init__(self, value: bytes):
        if len(value)>MAX_BYTES: raise ValueError("Binary payload exceeds byte limit")
        self.data=memoryview(value); self.offset=0
    def take(self, length: int) -> memoryview:
        end=self.offset+length
        if length<0 or end>len(self.data): raise ValueError("Truncated binary field")
        result=self.data[self.offset:end]; self.offset=end
        return result
    def scalar(self, format: str) -> object:
        return struct.unpack("<"+format,self.take(struct.calcsize("<"+format)))[0]
    def u32(self) -> int: return integer(self.scalar("I"))
    def count(self) -> int:
        count=self.u32()
        if count>MAX_ITEMS: raise ValueError("Binary array exceeds item limit")
        return count
    def boolean(self) -> bool:
        value=integer(self.scalar("B"))
        if value not in (0,1): raise ValueError("Invalid binary bool")
        return value==1
    def optional(self) -> bool:
        value=integer(self.scalar("B"))
        if value not in (0,1): raise ValueError("Invalid binary option")
        return value==1
    def text(self) -> str: return bytes(self.take(self.u32())).decode("utf-8")
    def bytes(self) -> bytes: return bytes(self.take(self.u32()))
    def numeric(self, format: str, count: int) -> tuple[tuple[object, ...], ...]:
        size=struct.calcsize("<"+format)
        return tuple(cast(tuple[object, ...], v) for v in struct.iter_unpack("<"+format,self.take(size*count)))
    def extension(self, depth: int = 0) -> ExtensionValue:
        if depth>128: raise ValueError("Extension nesting exceeds limit")
        tag=integer(self.scalar("B"))
        if tag==0: return None
        if tag==1: return self.boolean()
        if tag==2: return integer(self.scalar("q"))
        if tag==3: return integer(self.scalar("Q"))
        if tag==4: return number(self.scalar("d"))
        if tag==5: return self.text()
        if tag==6: return tuple(self.extension(depth+1) for _ in range(self.count()))
        if tag==7:
            result: dict[str, ExtensionValue]={}
            for _ in range(self.count()):
                key=self.text()
                if key in result: raise ValueError("Duplicate extension key")
                result[key]=self.extension(depth+1)
            return result
        raise ValueError("Unknown binary extension tag")
    def finish(self) -> None:
        if self.offset!=len(self.data): raise ValueError("Trailing binary fields")

'''

def render_binary(contract,header):
    if contract['version']!=1:raise ValueError('Unsupported binary contract')
    expected={'Modeling','Scene','MeshBinding','Parameter','Mesh','Resource'}
    if set(contract['edit_variants'])!=expected:raise ValueError('New Edit variant needs reviewed lifecycle/binary policy')
    types={};inputs=set();outputs=set()
    def visit(s,selected):
        k=s['kind']
        if k in {'serde_record','serde_enum','record','union','opaque'}:
            if s['name']=='__skipped':return
            if s['name'] in selected:return
            selected.add(s['name'])
            if s['name'] in types and types[s['name']]!=s:raise ValueError('Conflicting binary shape')
            types[s['name']]=s
            for f in s.get('fields',[]):
                if not f.get('skip'):visit(f['shape'],selected)
            for v in s.get('variants',[]):
                for f in v['fields']:
                    if not f.get('skip'):visit(f['shape'],selected)
        elif k in {'sequence','array','optional','shared'}:visit(s['item'],selected)
        elif k in {'string_map','index_map'}:
            if k=='index_map':visit(s['key'],selected)
            visit(s['value'],selected)
        elif k not in {'string','bool','u8','u16','u32','u64','usize','i32','i64','f32','f64','bytes','json'}:raise ValueError('Unselected binary primitive')
    for s in contract['inputs']:visit(s,inputs)
    visit(contract['output'],outputs)
    visit(contract['samples'],outputs)
    visit(contract['mesh'],outputs)
    visit(contract['image'],outputs)
    if contract['source_variants']!=['Formal','Edit'] or contract['source_fields']!=[['session','version'],['session','edit','base','sequence']]:raise ValueError('SourceRef binary lifecycle requires review')
    def name(s):return ''.join(re.findall(r'(?:^|::)([A-Z][A-Za-z0-9_]*)',s['name'])) if '<' in s['name'] else s['name'].rsplit('::',1)[-1]
    def fn(s):return name(s).lower()
    def fields(s):return [f for f in s.get('fields',[]) if not f.get('skip')]
    def attr(f):return f['name']+('_' if keyword.iskeyword(f['name']) else '')
    def variants(s):return [dict(v,name=v.get('name',v.get('wire_name')),form=v.get('form','record')) for v in s['variants']]
    def enum(s):return s['kind'] in {'serde_enum','union'}
    codes=dict(u8='B',u16='H',u32='I',u64='Q',usize='Q',i32='i',i64='q',f32='f',f64='d')
    def numeric(s):
        if s['kind'] in codes:return codes[s['kind']]
        if s['kind']=='array' and s['item']['kind'] in codes:return codes[s['item']['kind']]*s['length']
        if s.get('name','').endswith('::Vec2'):return 'ff'
        if s.get('name','').endswith('::PreciseVec2'):return 'dd'
        return None
    def getter(value,f):return '_field('+value+', '+repr(attr(f))+')'
    def write(s,e,indent,d=0):
        p=' '*indent;k=s['kind']
        if k=='shared':return write(s['item'],e,indent,d)
        if k in codes:return [p+'w.scalar('+repr(codes[k])+', '+e+')']
        if k=='string':return [p+'w.text('+e+')']
        if k=='bool':return [p+'w.boolean('+e+')']
        if k=='json':return [p+'w.extension('+e+')']
        if k=='bytes' or (k=='sequence' and s['item']['kind']=='u8'):return [p+'w.bytes('+e+')']
        if k=='optional':return [p+'if '+e+' is None:',p+'    w.raw(b"\\0")',p+'else:',p+'    w.raw(b"\\1")']+write(s['item'],e,indent+4,d)
        if k in {'sequence','array'}:
            v='items'+str(d);lines=[p+v+'=sequence('+e+')']
            if k=='sequence':lines += [p+'w.u32(len('+v+'))']
            else:lines += [p+'if len('+v+')!='+str(s['length'])+': raise ValueError("Invalid binary array length")']
            item=s['item'];fmt=numeric(item)
            if fmt and item['kind'] in codes: return lines+[p+'w.numeric('+repr(fmt)+', tuple('+v+'))']
            if fmt and item['kind']=='array':return lines+[p+'w.numeric('+repr(codes[item['item']['kind']])+', tuple(v for item in '+v+' for v in sequence(item)))']
            if fmt and item.get('name','').endswith(('::Vec2','::PreciseVec2')):return lines+[p+'w.numeric('+repr(fmt[0])+', tuple(v for item in '+v+' for v in _point(item)))']
            itemvar='item'+str(d)
            return lines+[p+'for '+itemvar+' in '+v+':']+write(item,itemvar,indent+4,d+1)
        if k in {'string_map','index_map'}:
            key={'kind':'string'} if k=='string_map' else s['key'];v='items'+str(d)
            return [p+v+'=_mapping('+e+')',p+'w.u32(len('+v+'))',p+'for key'+str(d)+', value'+str(d)+' in '+v+'.items():']+write(key,'key'+str(d),indent+4,d+1)+write(s['value'],'value'+str(d),indent+4,d+1)
        return [p+'_write_'+fn(s)+'('+e+', w)']
    def typ(s):
        k=s['kind']
        if k in codes:return 'float' if k.startswith('f') else 'int'
        if k in {'string','bool','bytes','json'}:return dict(string='str',bool='bool',bytes='bytes',json='ExtensionValue')[k]
        if k=='shared':return typ(s['item'])
        if k=='optional':return '('+typ(s['item'])+' | None)'
        if k=='sequence':return 'bytes' if s['item']['kind']=='u8' else 'tuple['+typ(s['item'])+', ...]'
        if k=='array':return 'tuple['+', '.join([typ(s['item'])]*s['length'])+']'
        if k in {'string_map','index_map'}:return 'Mapping['+('str' if k=='string_map' else 'int')+', '+typ(s['value'])+']'
        return {'Version':'Version','SourceRef':'Source','MeshSnapshot':'Snapshot[meshes.Mesh]','MeshGeometry':'meshes.Geometry','ImageSnapshot':'Snapshot[resources.Image]','ImagePixels':'resources.ImagePixels'}.get(name(s),'m.'+name(s))
    def read(s,d=0):
        k=s['kind']
        if k=='shared':return read(s['item'],d)
        if k in codes:return ('number' if k.startswith('f') else 'integer')+'(r.scalar('+repr(codes[k])+'))'
        if k in {'string','bool','json','bytes'}:return dict(string='r.text()',bool='r.boolean()',json='r.extension()',bytes='r.bytes()')[k]
        if k=='optional':return '('+read(s['item'],d)+' if r.optional() else None)'
        if k in {'sequence','array'}:
            item=s['item'];n='r.count()' if k=='sequence' else str(s['length'])
            if k=='sequence' and item['kind']=='u8':return 'r.bytes()'
            fmt=numeric(item)
            if fmt:
                if item['kind'] in codes:return 'cast('+repr(typ(s))+', tuple(v[0] for v in r.numeric('+repr(fmt)+', '+n+')))'
                if item['kind']=='array':return 'cast('+repr(typ(s))+', r.numeric('+repr(fmt)+', '+n+'))'
                expr='tuple(m.'+name(item)+'(x=number(v[0]), y=number(v[1])) for v in r.numeric('+repr(fmt)+', '+n+'))'
                return 'cast('+repr(typ(s))+', '+expr+')' if k=='array' else expr
            expr='tuple('+read(item,d+1)+' for _ in range('+n+'))'
            return 'cast('+repr(typ(s))+', '+expr+')' if k=='array' else expr
        if k in {'string_map','index_map'}:
            key={'kind':'string'} if k=='string_map' else s['key'];return '{'+read(key,d+1)+': '+read(s['value'],d+1)+' for _ in range(r.count())}'
        return '_read_'+fn(s)+'(r)'
    out=[header.rstrip(),PRELUDE,'INPUT_SIGNATURE = '+repr(contract['input_signature']),'MESH_SIGNATURE = '+repr(contract['mesh_signature']),'BINDING_SIGNATURE = '+repr(contract['binding_signature']),'SAMPLE_SIGNATURE = '+repr(contract['sample_signature']),'OUTPUT_SIGNATURE = '+repr(contract['output_signature']),'','def _point(value: object) -> tuple[object, object]:','    if isinstance(value, tuple):','        v = sequence(cast(object,value))','        if len(v)!=2: raise ValueError("Expected a point")','        return v[0],v[1]','    return _field(value,"x"),_field(value,"y")','']

    for s in types.values():
        if s['name'] in inputs and name(s) not in {'ParameterOperation','MeshBindingOperation','ResourceOperation'}:
            out+=['def _write_'+fn(s)+'(value: object, w: Writer) -> None:']
            if s['kind']=='opaque':
                if name(s)=='Version':
                    for f in ('session_id','generation','revision'):out+=['    w.scalar("Q", _field(value,'+repr(f)+'))']
                elif name(s) in {'ImageId','PlanId'}:out+=['    raw=record(value)','    w.raw(UUID(string(raw["runtime"])).bytes)','    w.scalar("Q",raw["serial"])']
                else:raise ValueError('Unreviewed opaque binary input')
            elif enum(s):
                vs=variants(s)
                if all(v['form']=='unit' for v in vs):out+=['    tag=string(_field(value,"value"))']
                else:out+=['    tag=string(getattr(value,"_variant",getattr(value,"kind",None)))']
                if name(s)=='SceneOperation':out+=['    tag=tag.removeprefix("scene_")']
                for i,v in enumerate(vs):
                    out+=['    '+('if' if i==0 else 'elif')+' tag=='+repr(v['name'])+':','        w.u32('+str(i)+')']
                    for f in fields(v):out+=write(f['shape'],getter('value',f),8)
                out+=['    else:','        raise TypeError("Unexpected '+name(s)+' variant")']
            elif s.get('name','').endswith(('::Vec2','::PreciseVec2')):
                fmt='f' if name(s)=='Vec2' else 'd';out+=['    x,y=_point(value)','    w.scalar('+repr(fmt)+',x)','    w.scalar('+repr(fmt)+',y)']
            else:
                for f in fields(s):out+=write(f['shape'],getter('value',f),4)
                if not fields(s):out+=['    pass']
            out+=['']
        if s['name'] in outputs:
            out+=['def _read_'+fn(s)+'(r: Reader) -> '+typ(s)+':']
            if s['kind']=='opaque':
                if name(s)=='Version':out+=['    return Version(integer(r.scalar("Q")),integer(r.scalar("Q")),integer(r.scalar("Q")))']
                elif name(s)=='SourceRef':out+=['    tag=r.u32()', '    runtime=str(UUID(bytes=bytes(r.take(16)))); session=integer(r.scalar("Q"))', '    edit=None', '    if tag==1:', '        if str(UUID(bytes=bytes(r.take(16))))!=runtime: raise ValueError("Mixed source runtimes")', '        edit=integer(r.scalar("Q"))', '    elif tag!=0: raise ValueError("Unknown source variant")', '    version=Version(integer(r.scalar("Q")),integer(r.scalar("Q")),integer(r.scalar("Q")))', '    sequence=integer(r.scalar("Q")) if tag==1 else 0', '    return Source(runtime,session,version,edit,sequence)']
                else:raise ValueError('Unreviewed opaque output')
            elif enum(s):
                out+=['    tag=r.u32()'];vs=variants(s)
                for i,v in enumerate(vs):
                    out+=['    '+('if' if i==0 else 'elif')+' tag=='+str(i)+':']
                    if all(v['form']=='unit' for v in vs):value='m.'+name(s)+'('+repr(v['name'])+')'
                    else:value='m.'+name(s)+v['rust_name']+'('+', '.join(attr(f)+'='+read(f['shape']) for f in fields(v))+')'
                    out+=['        return '+value]
                out+=['    raise ValueError("Unknown binary '+name(s)+' variant")']
            elif name(s) in {'MeshSnapshot','ImageSnapshot'}:
                if fields(s)[0]['name']!='source': raise ValueError('Mesh source must precede its payload')
                out+=['    return Snapshot('+read(fields(s)[0]['shape'])+', '+('meshes.Mesh' if name(s)=='MeshSnapshot' else 'resources.Image')+'('+', '.join(attr(f)+'='+read(f['shape']) for f in fields(s)[1:])+'))']
            else:out+=['    return '+typ(s)+'('+', '.join(attr(f)+'='+read(f['shape']) for f in fields(s))+')']
            out+=['']
    roots={name(s):s for s in contract['inputs']}
    def ordinal(root,variant):return next(i for i,v in enumerate(variants(roots[root])) if v['rust_name']==variant)
    tags={v:i for i,v in enumerate(contract['edit_variants'])}
    out+=['def encode_operations(operations: tuple[ops.WireOperation, ...]) -> bytes:','    if len(operations)>1024: raise ValueError("Too many authoring operations")','    w=Writer()','    w.raw(b"ZZA1"); w.raw(int(INPUT_SIGNATURE,16).to_bytes(16,"little")); w.u32(len(operations))','    for operation in operations:']
    out+=['        if isinstance(operation,m.OPERATION_TYPES):','            w.u32('+str(tags['Modeling'])+'); _write_modelingcommand(operation,w)','        elif isinstance(operation,scene.OPERATION_TYPES):','            w.u32('+str(tags['Scene'])+'); _write_sceneoperation(operation,w)','        elif isinstance(operation,(ops.Create,ops.Rename,ops.SetPositions)):','            w.u32('+str(tags['Mesh'])+'); _write_meshoperation(operation,w)']
    for cls,root,var,exprs in [('CreateParameter','ParameterOperation','Create',['operation.spec']),('PatchParameter','ParameterOperation','Patch',['operation.id','operation.patch']),('CreateMeshBinding','MeshBindingOperation','Create',['operation.spec']),('SetMeshKeyform','MeshBindingOperation','SetKeyform',['operation.id','operation.keyform']),('CreateImage','ResourceOperation','CreateImage',['operation','operation.image._token']),('ReplaceImage','ResourceOperation','ReplaceImage',['operation.id','operation.image._token']),('RenameImage','ResourceOperation','RenameImage',['operation.id','operation.name','operation.expected_name']),('ApplyPlan','ResourceOperation','ApplyPlan',['operation.plan._token'])]:
        domain={'ParameterOperation':'Parameter','MeshBindingOperation':'MeshBinding','ResourceOperation':'Resource'}[root];i=ordinal(root,var);v=variants(roots[root])[i]
        out+=['        elif isinstance(operation,ops.'+cls+'):','            w.u32('+str(tags[domain])+'); w.u32('+str(i)+')']
        if len(fields(v))!=len(exprs):raise ValueError('Changed ownership projection')
        for f,e in zip(fields(v),exprs):out+=write(f['shape'],e,12)
    out+=['        else:','            raise TypeError("Expected typed authoring operation")','    return bytes(w.data)','','def decode_records(value: bytes, signature: str) -> m.OwnedModelingRecords:','    if signature!=OUTPUT_SIGNATURE: raise ValueError("Binary record contract differs")','    r=Reader(value)','    result=_read_ownedmodelingrecords(r)','    r.finish()','    return result','']
    out+=['def decode_samples(value: bytes, signature: str) -> m.ModelingSamples:', '    if signature!=SAMPLE_SIGNATURE: raise ValueError("Binary sample contract differs")', '    r=Reader(value)', '    result=_read_modelingsamples(r)', '    r.finish()', '    return result', '']
    out+=['def decode_meshes(value: bytes,signature:str)->tuple[Snapshot[meshes.Mesh],...]:','    if signature!=MESH_SIGNATURE: raise ValueError("Binary mesh contract differs")','    r=Reader(value)','    result=tuple(_read_meshsnapshot(r) for _ in range(r.count()))','    r.finish()','    return result','']
    out+=['def decode_bindings(value:bytes,signature:str)->tuple[m.MeshBinding,...]:','    if signature!=BINDING_SIGNATURE: raise ValueError("Binary binding contract differs")','    r=Reader(value)','    result=tuple(_read_meshbinding(r) for _ in range(r.count()))','    r.finish()','    return result','']
    out+=['def decode_images(value:bytes,signature:str)->tuple[Snapshot[resources.Image],...]:', '    if signature!='+repr(contract['image_signature'])+': raise ValueError("Binary image contract differs")','    r=Reader(value)','    result=tuple(_read_imagesnapshot(r) for _ in range(r.count()))','    r.finish()','    return result','']
    return {'client/python/zipzi/_generated/binary.py':'\n'.join(out)}
