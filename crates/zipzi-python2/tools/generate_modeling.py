"""Compiled, explicitly selected modeling shapes. No runtime schema service."""
import re
import keyword
import json
TARGETS=frozenset({'client/python/zipzi/_generated/modeling.py','src/generated/modeling.rs'})

def public_name(rust_name):
    return ''.join(re.findall(r'(?:^|::)([A-Z][A-Za-z0-9_]*)',rust_name)) if '<' in rust_name else rust_name.rsplit('::',1)[-1]

def render_modeling(contract, header, rust_header):
    shapes={}
    def visit(s):
        k=s['kind']
        if k in {'serde_record','serde_enum','record','union','opaque'}:
            if k=='serde_record' and s.get('tuple') and not s.get('transparent'):raise ValueError('Unselected tuple record projection')
            if s['name']=='__skipped': return
            if s['name'] in shapes:
                if shapes[s['name']]!=s:raise ValueError('Conflicting modeling shape')
                return
            shapes[s['name']]=s
            for f in s.get('fields',[]):
                if not f.get('skip'):visit(f['shape'])
            for v in s.get('variants',[]):
                for f in v['fields']:
                    if not f.get('skip'):visit(f['shape'])
        elif k in {'optional','sequence','array','shared'}:visit(s['item'])
        elif k in {'string_map','index_map'}:visit(s['value'])
        elif k not in {'string','bool','u8','u16','u32','u64','usize','i32','i64','f32','f64','json','bytes'}:raise ValueError('Unsupported modeling shape '+k)
    for s in contract['roots']:visit(s)
    public_shapes=set(shapes)
    for s in contract.get('native_only_roots',[]):visit(s)
    input_names=set()
    def mark(s):
        if 'name' in s and s['kind'] in {'serde_record','serde_enum','record','union','opaque'}:input_names.add(s['name'])
        for f in s.get('fields',[]):
            if not f.get('skip'):mark(f['shape'])
        for v in s.get('variants',[]):
            for f in v['fields']:
                if not f.get('skip'):mark(f['shape'])
        if 'item' in s:mark(s['item'])
        if 'value' in s and isinstance(s['value'],dict):mark(s['value'])
    for index in contract.get('input_roots',[0,1,2]):mark(contract['roots'][index])
    for s in contract.get('native_only_roots',[]):mark(s)
    names={n:public_name(n) for n in shapes}
    if len(set(names.values()))!=len(names):raise ValueError('Duplicate selected modeling name')
    def path(s):
        n=s['name']
        n=re.sub(r'zipzi_core::document::(?:[a-z_0-9]+::)*', 'zipzi_core::document::', n)
        n=re.sub(r'zipzi_sdk::(?:[a-z_0-9]+::)*', 'zipzi_sdk::', n)
        n=n.replace('zipzi_core::scene_reference::', 'zipzi_core::').replace('zipzi_core::evaluation::types::','zipzi_core::evaluation::')
        n=re.sub(r'zipzi_animation::(?:motion_preview|physics)::', 'zipzi_sdk::', n)
        return n
    def nm(s):return names[s['name']]
    def pn(f):return f['name'] + ('_' if keyword.iskeyword(f['name']) else '')
    def access(s,f):
        if nm(s) in {'Vec2','PreciseVec2','GeometryBounds'}:return 'value.get_item('+str(fields(s).index(f))+')?'
        return 'value.getattr("'+('intent' if nm(s)=='ModelingIntent' else pn(f))+'")?'
    def fields(s):return [f for f in s.get('fields',[]) if not f.get('skip')]
    def variants(s):
        return [dict(v, name=v.get('name',v.get('wire_name')),form=v.get('form','record')) for v in s['variants']]
    def enum(s):return s['kind'] in {'serde_enum','union'}
    def unit(s):return enum(s) and all(v['form']=='unit' for v in variants(s))
    def vn(s,v):return nm(s)+v['rust_name']
    def member(v):return re.sub(r'(?<!^)(?=[A-Z])','_',v['rust_name']).upper()
    def typ(s):
        k=s['kind']
        if k=='opaque' and nm(s)=='SourceRef':return '_Source'
        if k in {'u8','u16','u32','u64','usize','i32','i64'}:return 'int'
        if k in {'f32','f64'}:return 'float'
        if k in {'string','bool','bytes','json'}:return dict(string='str',bool='bool',bytes='bytes',json='ExtensionValue')[k]
        if k=='shared':return typ(s['item'])
        if k=='optional':return '('+typ(s['item'])+' | None)'
        if k=='sequence' and s['item']['kind']=='u8':return 'bytes'
        if k=='sequence':return 'tuple['+typ(s['item'])+', ...]'
        if k=='array':return 'tuple['+', '.join([typ(s['item'])]*s['length'])+']'
        if k in {'string_map','index_map'}:return 'Mapping['+('str' if k=='string_map' else 'int')+', '+typ(s['value'])+']'
        return nm(s)
    def freeze(s,e,d=0):
        k=s['kind']
        if k=='sequence' and s['item']['kind']=='u8':return e
        if k=='shared':return freeze(s['item'],e,d)
        if k=='optional':return f'None if {e} is None else '+freeze(s['item'],e,d)
        if k in {'array','sequence'}:
            item=s['item'];v='v'+str(d)
            if item['kind'] not in {'optional','sequence','array','shared','string_map','index_map','json'}:return f'tuple({e})'
            return 'tuple('+freeze(item,v,d+1)+f' for {v} in {e})'
        if k in {'string_map','index_map'}:
            v='v'+str(d);key='key'+str(d)
            return 'MappingProxyType({'+key+': '+freeze(s['value'],v,d+1)+f' for {key}, {v} in {e}.items()'+'})'
        if k=='json':return '_freeze_extension('+e+')'
        return e
    def check(s,e,indent,d=0):
        k=s['kind'];p=' '*indent
        if k=='sequence' and s['item']['kind']=='u8':return [p+f'if not isinstance({e}, bytes):',p+'    raise TypeError(\"Expected immutable bytes\")']
        if k=='shared':return check(s['item'],e,indent,d)
        if k=='string':cond=f'not isinstance({e}, str)'
        elif k=='bool':cond=f'type({e}) is not bool'
        elif k in {'u8','u16','u32','u64','usize','i32','i64'}:
            bits=dict(u8=8,u16=16,u32=32,u64=64,usize=64,i32=32,i64=64)[k];lo=-(1<<(bits-1)) if k.startswith('i') else 0;hi=(1<<(bits-1)) if k.startswith('i') else 1<<bits
            cond=f'type({e}) is not int or not {lo} <= {e} < {hi}'
        elif k in {'f32','f64'}:
            cond=f'isinstance({e}, bool) or not isinstance({e}, (float, int)) or not math.isfinite({e})'+(f' or abs({e}) > 3.4028234663852886e38' if k=='f32' else '')
        elif k=='optional':return [p+f'if {e} is not None:']+check(s['item'],e,indent+4,d)
        elif k in {'sequence','array'}:
            v='v'+str(d);out=[p+f'if not isinstance({e}, tuple):',p+'    raise TypeError("Expected an immutable tuple")']
            if k=='array':out += [p+f'if len({e}) != {s["length"]}:',p+'    raise ValueError("Invalid fixed array length")']
            return out+[p+f'for {v} in {e}:']+check(s['item'],v,indent+4,d+1)
        elif k in {'string_map','index_map'}:
            v='v'+str(d);key='key'+str(d)
            return [p+f'for {key}, {v} in {e}.items():']+check(({'kind':'string'} if k=='string_map' else s['key']),key,indent+4,d+1)+check(s['value'],v,indent+4,d+1)
        elif k=='json':return [p+f'_validate_extension({e})']
        elif k=='bytes':cond=f'not isinstance({e}, bytes)'
        else:return [p+f'validate_{nm(s)}({e})']
        return [p+f'if {cond}:',p+f'    raise ValueError("Expected {k}")']
    def encode(s,e,d=0):
        k=s['kind']
        if k=='shared':return encode(s['item'],e,d)
        if k=='optional':return f'None if {e} is None else '+encode(s['item'],e,d)
        if k in {'array','sequence'}:
            v='v'+str(d);return '['+encode(s['item'],v,d+1)+f' for {v} in {e}]'
        if k in {'string_map','index_map'}:
            v='v'+str(d);key='key'+str(d);return '{str('+key+'): '+encode(s['value'],v,d+1)+f' for {key}, {v} in {e}.items()'+'}'
        if k=='json':return '_encode_extension('+e+')'
        if k=='bytes':return 'list('+e+')'
        if k in {'string','bool','u8','u16','u32','u64','usize','i32','i64','f32','f64'}:return e
        return 'encode_'+nm(s)+'('+e+')'
    def decode(s,e,d=0):
        k=s['kind']
        if k=='sequence' and s['item']['kind']=='u8':return '_bytes('+e+')'
        if k=='shared':return decode(s['item'],e,d)
        if k=='optional':return f'None if {e} is None else '+decode(s['item'],e,d)
        if k in {'array','sequence'}:
            v='v'+str(d);expr='tuple('+decode(s['item'],v,d+1)+f' for {v} in sequence({e}))'
            return 'cast('+repr(typ(s))+', '+expr+')' if k=='array' else expr
        if k in {'string_map','index_map'}:
            v='v'+str(d);key='key'+str(d);return '{'+(key if k=='string_map' else 'int('+key+')')+': '+decode(s['value'],v,d+1)+f' for {key}, {v} in record({e}).items()'+'}'
        if k=='json':return '_read_extension('+e+')'
        if k=='bytes':return '_bytes('+e+')'
        if k in {'u8','u16','u32','u64','usize','i32','i64'}:return 'integer('+e+')'
        if k in {'f32','f64'}:return 'number('+e+')'
        if k in {'string','bool'}:return ('string' if k=='string' else '_boolean')+'('+e+')'
        return 'read_'+nm(s)+'('+e+')'
    py=[header.rstrip(),'from __future__ import annotations','from dataclasses import dataclass, field','from enum import Enum','from collections.abc import Mapping','from types import MappingProxyType','from typing import ClassVar, TypeAlias, NamedTuple, cast','import math','from .._values import record, sequence, string, integer, number, Version','from .._extensions import ExtensionValue, freeze as _freeze_extension, validate as _validate_extension, encode as _encode_extension, read as _read_extension','','def _bytes(value: object) -> bytes:', '    return value if isinstance(value, bytes) else bytes(integer(v) for v in sequence(value))', '', 'def _boolean(value: object) -> bool:','    if type(value) is not bool: raise TypeError("Expected bool")','    return value','']
    def record_class(n,fs,tag=None):
        if n in {'Vec2','PreciseVec2'}:
            py.extend(['class '+n+'(NamedTuple):', '    __module__ = \"zipzi.modeling\"']+[ '    '+pn(f)+': '+typ(f['shape']) for f in fs]+[''])
            return
        py.extend(['@dataclass(frozen=True, kw_only=True)','class '+n+':'])
        if tag is not None:py.append('    _variant: ClassVar[str] = '+repr(tag))
        if n.startswith('ModelingCommand'):py.append('    kind: ClassVar[str] = \"modeling\"')
        for f in fs:
            default=''
            if f['shape']['kind']=='optional' and f.get('default') in (None, {'kind':'null'}):
                default=' = None'
            elif f.get('default')=={'kind':'empty_string'}:
                default=' = ""'
            elif f.get('default') is not None:
                value=f['default']
                default=' = field(default_factory=lambda: '+decode(f['shape'],repr(value))+')'
            py.append('    '+pn(f)+': '+typ(f['shape'])+default)
        if not fs and tag is None:py.append('    pass')
        if fs:
            py.extend(['','    def __post_init__(self) -> None:'])
            for f in fs:py.append('        object.__setattr__(self, '+repr(pn(f))+', '+freeze(f['shape'],'self.'+pn(f))+')')
        py.extend([n+'.__module__ = "zipzi.modeling"', ''])
    for s in shapes.values():
        if nm(s)=='ModelingIntent' or s['name'] not in public_shapes: continue
        if s['kind']=='opaque':
            if nm(s) not in {'Version','SourceRef'}:raise ValueError('Unselected opaque modeling type')
            continue
        if unit(s):
            py.extend(['class '+nm(s)+'(str, Enum):','    __module__ = "zipzi.modeling"'])
            for v in variants(s):py.append('    '+member(v)+' = '+repr(v['name']))
            py.append('')
        elif enum(s):
            for v in variants(s):record_class(vn(s,v),fields(v),v['name'])
            py.extend([nm(s)+': TypeAlias = '+' | '.join(vn(s,v) for v in variants(s)),''])
        else:record_class(nm(s),fields(s))
    for s in shapes.values():
        if nm(s)=='ModelingIntent' or s['name'] not in public_shapes: continue
        n=nm(s);fs=fields(s)
        if s['kind']=='opaque' and n=='SourceRef':
            py+=['def validate_SourceRef(value: _Source) -> None:', '    if not isinstance(value, _Source): raise TypeError("Expected Source")', '', 'def encode_SourceRef(value: _Source) -> object:', '    return value._wire()', '', 'def read_SourceRef(value: object) -> _Source:', '    return value if isinstance(value, _Source) else _Source._read(value)', '']
            continue
        if s['kind']=='opaque':
            py+=['def validate_Version(value: Version) -> None:', '    if not isinstance(value, Version): raise TypeError(\"Expected Version\")', '', 'def encode_Version(value: Version) -> object:', '    return dict(session_id=value.session_id, generation=value.generation, revision=value.revision)', '', 'def read_Version(value: object) -> Version:', '    return Version.read(value)', '']
            continue
        py+=['def validate_'+n+'(value: '+n+') -> None:']
        if enum(s) and not unit(s):
            for i,v in enumerate(variants(s)):
                py.append('    '+('if' if i==0 else 'elif')+' isinstance(value, '+vn(s,v)+'):')
                for f in fields(v):py+=check(f['shape'],'value.'+pn(f),8)
                if not fields(v):py.append('        pass')
            py+=['    else:','        raise TypeError("Expected '+n+'")','']
        else:
            py+=['    if not isinstance(value, '+n+'):','        raise TypeError("Expected '+n+'")']
            for f in fs:py+=check(f['shape'],'value.'+pn(f),4)
            known=tuple(sorted(f['name'] for f in fs if not f.get('flatten')))
            for f in fs:
                if f.get('flatten'):
                    if f['shape']['kind']!='string_map' or f['shape']['value']['kind']!='json':
                        raise ValueError('Only finite extension maps may be flattened')
                    py+=['    if set(value.'+pn(f)+').intersection('+repr(known)+'):',
                         '        raise ValueError("Extension keys overlap declared fields")']
            py.append('')
        py+=['def encode_'+n+'(value: '+n+') -> object:']
        if unit(s):py+=['    return value.value','']
        elif enum(s):
            for i,v in enumerate(variants(s)):
                py.append('    '+('if' if i==0 else 'elif')+' isinstance(value, '+vn(s,v)+'):')
                content='{'+', '.join(repr(f['name'])+': '+encode(f['shape'],'value.'+pn(f)) for f in fields(v))+'}'
                if v['form']=='newtype':content=encode(fields(v)[0]['shape'],'value.'+fields(v)[0]['name'])
                if v['form']=='tuple':content='['+', '.join(encode(f['shape'],'value.'+pn(f)) for f in fields(v))+']'
                tag=s.get('tag');ct=s.get('content')
                if tag and ct:out='{'+repr(tag)+': '+repr(v['name'])+(', '+repr(ct)+': '+content if v['form']!='unit' else '')+'}'
                elif tag:out='{'+repr(tag)+': '+repr(v['name'])+', **cast(Mapping[str, object], '+content+')}'
                elif v['form']=='unit':out=repr(v['name'])
                else:out='{'+repr(v['name'])+': '+content+'}'
                py.append('        return '+out)
            py+=['    raise TypeError("Expected '+n+'")','']
        else:
            out='{'+', '.join(('**'+encode(f['shape'],'value.'+pn(f)) if f.get('flatten') else repr(f['name'])+': '+encode(f['shape'],'value.'+pn(f))) for f in fs)+'}'
            if s.get('transparent'):out=encode(fs[0]['shape'],'value.'+fs[0]['name'])
            py+=['    return '+out,'']
        py+=['def read_'+n+'(value: object) -> '+n+':']
        if enum(s) and not unit(s):py+=['    if isinstance(value, ('+', '.join(vn(s,v) for v in variants(s))+',)): return value']
        else:py+=['    if isinstance(value, '+n+'): return value']
        if unit(s):py+=['    return '+n+'(string(value))','']
        elif enum(s):
            tag=s.get('tag');ct=s.get('content')
            if tag:py+=['    r = record(value)','    tag = string(r['+repr(tag)+'])']
            else:py+=['    r: Mapping[str, object] = {} if isinstance(value, str) else record(value)', '    tag = string(value) if isinstance(value, str) else next(iter(r))']
            for i,v in enumerate(variants(s)):
                py.append('    '+('if' if i==0 else 'elif')+' tag == '+repr(v['name'])+':')
                container='r['+repr(ct)+']' if ct else ('r' if tag else 'r['+repr(v['name'])+']')
                if v['form']=='record':py.append('        f = record('+container+')')
                args=[]
                for j,f in enumerate(fields(v)):
                    e=('f.get('+repr(f['name'])+', '+repr(f['default'])+')' if 'default' in f else 'f['+repr(f['name'])+']') if v['form']=='record' else (container if v['form']=='newtype' else 'sequence('+container+')['+str(j)+']')
                    args.append(pn(f)+'='+decode(f['shape'],e))
                py.append('        return '+vn(s,v)+'('+', '.join(args)+')')
            py+=['    raise ValueError("Unknown '+n+' variant")','']
        else:
            if not s.get('transparent'):py.append('    r = record(value)')
            known=tuple(f['name'] for f in fs if not f.get('flatten'))
            def field_input(f):
                if f.get('flatten'):return '{key: item for key, item in r.items() if key not in '+repr(known)+'}'
                return 'value' if s.get('transparent') else ('r.get('+repr(f['name'])+', '+repr(f['default'])+')' if 'default' in f and s['kind']=='serde_record' else 'r['+repr(f['name'])+']')
            py+=['    return '+n+'('+', '.join(pn(f)+'='+decode(f['shape'],field_input(f)) for f in fs)+')','']
    root=contract['roots'][0]
    py+=['OPERATION_TYPES = ('+', '.join(vn(root,v) for v in variants(root))+',)','']
    def rn(s):return 'read_'+nm(s).lower()
    def wn(s):return 'write_'+nm(s).lower()
    def nr(s,e,d=0):
        k=s['kind']
        if k=='sequence' and s['item']['kind']=='u8':return e+'.extract::<Vec<u8>>()?'
        if k=='shared':return 'std::sync::Arc::new('+nr(s['item'],e,d)+')'
        if k=='optional':return 'if '+e+'.is_none() { None } else { Some('+nr(s['item'],e,d)+') }'
        if k in {'sequence','array'}:
            v='item'+str(d)
            out=e+'.try_iter()?.map(|item| -> PyResult<_> { let '+v+' = item?; Ok('+nr(s['item'],v,d+1)+') }).collect::<PyResult<Vec<_>>>()?'
            return out+'.try_into().map_err(|_| pyo3::exceptions::PyValueError::new_err("Invalid fixed array length"))?' if k=='array' else out+'.into()'
        if k in {'string_map','index_map'}:
            v='item'+str(d);return e+'.call_method0("items")?.try_iter()?.map(|item| -> PyResult<_> { let '+v+' = item?; Ok(('+v+'.get_item(0)?.extract()?, '+nr(s['value'],v+'.get_item(1)?',d+1)+')) }).collect::<PyResult<_>>()?'
        if k=='json':return 'read_extension(&'+e+')?'
        if k=='bytes':return e+'.extract::<Vec<u8>>()?.into()'
        if k in {'string','bool','u8','u16','u32','u64','usize','i32','i64','f32','f64'}:return e+'.extract()?'
        return rn(s)+'(&'+e+')?'
    def nw(s,e,d=0):
        k=s['kind'];e='('+e+')'
        if k=='sequence' and s['item']['kind']=='u8':return 'PyBytes::new(py, '+e+').into_any().unbind()'
        if k=='shared':return nw(s['item'],e+'.as_ref()',d)
        if k=='optional':return '{ match '+e+'.as_ref() { Some(item)=>'+nw(s['item'],'item',d+1)+', None=>py.None() } }'
        if k in {'array','sequence','bytes'}:
            v='item'+str(d);item=s.get('item',{'kind':'u8'});return '{ let out=PyList::empty(py); for '+v+' in '+e+' { out.append('+nw(item,v,d+1)+')?; } out.into_any().unbind() }'
        if k in {'string_map','index_map'}:
            v='item'+str(d);return '{ let out=PyDict::new(py); for (key, '+v+') in '+e+' { out.set_item(key.to_string(), '+nw(s['value'],v,d+1)+')?; } out.into_any().unbind() }'
        if k=='json':return 'write_extension(py,'+e+')?'
        if k in {'string','bool','u8','u16','u32','u64','usize','i32','i64','f32','f64'}:return '('+e+').into_pyobject(py)?.into_bound().into_any().unbind()'
        return wn(s)+('' if s['kind']=='opaque' else '_in')+'(py, '+e+('' if s['kind']=='opaque' else ', classes')+')?'
    rust=[rust_header.rstrip(),'#![allow(dead_code, unused_variables, unused_parens)]','use pyo3::BoundObject;','use crate::*;','use crate::extension_bridge::{read_extension,write_extension};','']
    for s in shapes.values():
        n=path(s);fs=fields(s);read_start=len(rust)
        if s['kind']=='opaque' and nm(s)=='SourceRef':
            rust += ["pub fn read_sourceref(value: &Bound<'_, PyAny>) -> PyResult<SourceRef> { crate::modeling_bridge::read_source(value) }", "pub fn write_sourceref(py: Python<'_>, value: &SourceRef) -> PyResult<Py<PyAny>> { Ok(PyModule::import(py, \"zipzi._values\")?.getattr(\"Source\")?.call_method1(\"_read\", (source(py, *value)?,))?.unbind()) }"]
            continue
        if s['kind']=='opaque':
            rust += ['pub fn read_version(value: &Bound<\'_ , PyAny>) -> PyResult<zipzi_sdk::Version> { Ok(zipzi_sdk::Version {session_id:value.getattr(\"session_id\")?.extract()?,generation:value.getattr(\"generation\")?.extract()?,revision:value.getattr(\"revision\")?.extract()?}) }', 'pub fn write_version(py:Python<\'_>, value:&zipzi_sdk::Version)->PyResult<Py<PyAny>> {Ok(PyModule::import(py,"zipzi._values")?.getattr("Version")?.call1((value.session_id,value.generation,value.revision))?.unbind())}']
            continue
        rust+=['pub fn '+rn(s)+"(value: &Bound<'_, PyAny>) -> PyResult<"+n+'> {']
        if unit(s):
            rust+=['    match value.getattr("value")?.extract::<String>()?.as_str() {']
            for v in variants(s):rust+=['        '+repr(v['name']).replace("'",'"')+' => Ok('+n+'::'+v['rust_name']+'),']
            rust+=['        _=>Err(pyo3::exceptions::PyValueError::new_err("Unknown enum value")),','    }']
        elif enum(s):
            rust+=['    match value.getattr("_variant")?.extract::<String>()?.as_str() {']
            for v in variants(s):
                vs=fields(v);args=[nr(f['shape'],'value.getattr("'+('intent' if nm(s)=='ModelingIntent' else pn(f))+'")?') for f in vs]
                val=n+'::'+v['rust_name']
                if v['form']=='record':val+=' { '+', '.join(f.get('rust_name',f['name'])+': '+arg for f,arg in zip(vs,args))+' }'
                elif v['form'] in {'tuple','newtype'}:val+='('+', '.join(args)+')'
                rust+=['        "'+v['name']+'" => Ok('+val+'),']
            rust+=['        _=>Err(pyo3::exceptions::PyTypeError::new_err("Unexpected modeling variant")),','    }']
        elif s.get('transparent'):
            rust+=['    Ok(value.getattr("'+fs[0]['name']+'")?.extract::<String>()?.into())']
        else:
            entries=[f.get('rust_name',f['name'])+': '+('Default::default()' if f.get('skip') else nr(f['shape'],access(s,f))) for f in s['fields']]
            rust+=['    Ok('+n.replace('<', '::<', 1)+' { '+', '.join(entries)+' })']
        rust+=['}', '']
        if s['name'] not in input_names:del rust[read_start:]
        if nm(s)=='ModelingIntent' or s['name'] not in public_shapes:continue
        rust+=['pub fn '+wn(s)+"(py: Python<'_>, value: &"+n+') -> PyResult<Py<PyAny>> {', '    let classes=PyModule::import(py, \"zipzi._generated.modeling\")?;', '    '+wn(s)+'_in(py,value,&classes)', '}', 'fn '+wn(s)+"_in(py: Python<'_>, value: &"+n+", classes:&Bound<'_,PyModule>) -> PyResult<Py<PyAny>> {"]
        if unit(s):
            rust+=['    let name=match value {']
            for v in variants(s):rust+=['        '+n+'::'+v['rust_name']+' => "'+v['name']+'",']
            rust+=['    };','    Ok(classes.getattr(\"'+nm(s)+'\")?.call1((name,))?.unbind())']
        elif enum(s):
            rust+=['    match value {']
            for v in variants(s):
                vs=fields(v);ids=['field'+str(i) for i in range(len(vs))];pat=n+'::'+v['rust_name']
                if v['form']=='record':pat+=' { '+', '.join(f.get('rust_name',f['name'])+': '+i for f,i in zip(vs,ids))+' }'
                elif v['form'] in {'newtype','tuple'}:pat+='('+', '.join(ids)+')'
                rust+=['        '+pat+' => {']
                if v['form']=='record':
                    rust+=['            let content=PyDict::new(py);']
                    rust+=['            content.set_item("'+pn(f)+'", '+nw(f['shape'],i)+')?;' for f,i in zip(vs,ids)]
                    content='content.into_any().unbind()'
                elif v['form']=='newtype':content=nw(vs[0]['shape'],ids[0])
                elif v['form']=='tuple':
                    rust+=['            let content=PyList::empty(py);']+['            content.append('+nw(f['shape'],i)+')?;' for f,i in zip(vs,ids)];content='content.into_any().unbind()'
                else:content='py.None()'
                cls=nm(s)+v['rust_name']
                if v['form']=='unit':rust+=['            Ok(classes.getattr("'+cls+'")?.call0()?.unbind())']
                else:
                    if v['form']=='record':kwargs='content'
                    else:
                        kwargs='kwargs';rust+=['            let kwargs=PyDict::new(py);']
                        rust+=['            kwargs.set_item("'+pn(f)+'", '+nw(f['shape'],i)+')?;' for f,i in zip(vs,ids)]
                    rust+=['            Ok(classes.getattr("'+cls+'")?.call((),Some(&'+kwargs+'))?.unbind())']
                rust+=['        },']
            rust+=['    }']
        elif nm(s) in {'Vec2','PreciseVec2'}:rust+=['    Ok(classes.getattr("'+nm(s)+'")?.call1((value.x,value.y))?.unbind())']
        elif s.get('transparent'):rust+=['    Ok(classes.getattr(\"'+nm(s)+'\")?.call((),Some(&{let kwargs=PyDict::new(py);kwargs.set_item(\"value\",value.as_str())?;kwargs}))?.unbind())']
        else:
            rust+=['    let out=PyDict::new(py);']
            rust+=['    out.set_item(\"'+pn(f)+'\", '+nw(f['shape'],'&value.'+f.get('rust_name',f['name']))+')?;' for f in fs]
            rust+=['    Ok(classes.getattr(\"'+nm(s)+'\")?.call((),Some(&out))?.unbind())']
        rust+=['}','']
    def measure(s,e,d=0):
        k=s['kind'];e='('+e+')'
        if k=='shared':return measure(s['item'],e,d)
        if k=='optional':return 'if '+e+'.is_none() {0usize} else {'+measure(s['item'],e,d)+'}'
        if k=='string':return e+'.len()?.saturating_mul(4).saturating_add(32)'
        if k=='bytes' or (k=='sequence' and s['item']['kind']=='u8'):return e+'.len()?.saturating_mul(2).saturating_add(64)'
        if k in {'bool','u8','u16','u32','u64','usize','i32','i64','f32','f64'}:return '16usize'
        if k in {'array','sequence'}:
            v='v'+str(d)
            return '{let mut bytes=64usize;for value in '+e+'.try_iter()? {let '+v+'=value?;bytes=bytes.saturating_add('+measure(s['item'],v,d+1)+');}bytes}'
        if k in {'string_map','index_map'}:
            v='v'+str(d)
            return '{let mut bytes=64usize;for value in '+e+'.call_method0("items")?.try_iter()? {let '+v+'=value?;bytes=bytes.saturating_add(128).saturating_add('+measure(({'kind':'string'} if k=='string_map' else s['key']),v+'.get_item(0)?',d+1)+').saturating_add('+measure(s['value'],v+'.get_item(1)?',d+1)+');}bytes}'
        if k=='json':return 'crate::extension_bridge::measure_extension(&'+e+')?'
        if s['kind']=='opaque':return '128usize'
        return 'measure_'+nm(s).lower()+'(&'+e+')?'
    for s in shapes.values():
        if s['name'] not in input_names:continue
        rust += ['pub fn measure_'+nm(s).lower()+"(value:&Bound<'_,PyAny>)->PyResult<usize> {"]
        if unit(s) or s['kind']=='opaque':rust += ['    Ok(128usize)']
        elif enum(s):
            rust += ['    match value.getattr("_variant")?.extract::<String>()?.as_str() {']
            for v in variants(s):
                expr='256usize'
                for f in fields(v):expr+='.saturating_add('+measure(f['shape'],access(s,f))+')'
                rust += ['        "'+v['name']+'"=>Ok('+expr+'),']
            rust += ['        _=>Err(pyo3::exceptions::PyTypeError::new_err("Unexpected modeling variant")),','    }']
        else:
            expr='256usize'
            for f in fields(s):expr+='.saturating_add('+measure(f['shape'],access(s,f))+')'
            rust += ['    Ok('+expr+')']
        rust += ['}','']
    object_kind=next((s for s in shapes.values() if nm(s)=='ObjectKind'),None)
    if object_kind:
        rust += ['pub fn read_objectkind_name(value:&str)->PyResult<zipzi_sdk::ObjectKind> {match value {']
        rust += ['    "'+v['name']+'"=>Ok(zipzi_sdk::ObjectKind::'+v['rust_name']+'),' for v in variants(object_kind)]
        rust += ['    _=>Err(pyo3::exceptions::PyValueError::new_err("Unknown object kind")),','}}']
    # Class annotations resolve through the public module after __module__ is
    # assigned. Keep implementation typing names private in both namespaces.
    py.insert(2, 'from typing import ClassVar as _ClassVar')
    py.insert(3, 'from collections.abc import Mapping as _Mapping')
    py.insert(4, 'from .._values import Version as _Version')
    py.insert(5, 'from builtins import bytes as _Bytes')
    py.insert(6, 'from .._values import Source as _Source')
    for index,line in enumerate(py):
        if line.startswith('from typing import ClassVar,'):py[index]=line.replace('ClassVar, ','')
        if re.match(r'    [A-Za-z_]\w*:',line):
            for name in ('ClassVar','Mapping','Version'):
                line=re.sub(r'\b'+name+r'\b','_'+name,line)
            field,colon,annotation=line.partition(':')
            line=field+colon+re.sub(r'\bbytes\b','_Bytes',annotation)
            py[index]=line
    return {'client/python/zipzi/_generated/modeling.py' :'\n'.join(py),'src/generated/modeling.rs':'\n'.join(rust)}
