"""Finite P3 records/unions; names and fields come from compiled Rust shapes."""
TARGETS = frozenset({'client/python/zipzi/_generated/scene.py', 'src/generated/scene.rs'})

def render_scene(contract, header, rust_header, identifier, rust_path):
    if contract['version'] != 1:
        raise ValueError('Unsupported scene contract')
    bindings = {}
    for b in contract['bindings']:
        identifier(b['name']); rust_path(b['path']); rust_path(b['rust'])
        if b['rust'] in bindings or b['module'] != 'scene' or b['form'] not in {'record', 'union'}:
            raise ValueError('Invalid scene binding')
        bindings[b['rust']] = b
    types = {}
    def visit(s):
        k = s['kind']
        if k in {'string', 'bool', 'f32', 'u32', 'i32'}: return
        if k in {'optional', 'sequence', 'array'}:
            if k == 'array' and not ((s['length'] in {2, 3, 4} and s['item']['kind'] == 'f32') or (s['length'] == 3 and s['item']['kind'] == 'u32')):
                raise ValueError('Unreviewed scene fixed array')
            visit(s['item']); return
        if k not in {'record', 'union'} or s['name'] not in bindings:
            raise ValueError('Unselected scene shape')
        if s['name'] in types:
            if types[s['name']] != s: raise ValueError('Conflicting scene shape')
            return
        types[s['name']] = s
        fields = s.get('fields', [])
        if k == 'union':
            if s['tag'] != 'op': raise ValueError('Unreviewed scene discriminator')
            if len({v['wire_name'] for v in s['variants']}) != len(s['variants']):
                raise ValueError('Duplicate scene tag')
            fields = [f for v in s['variants'] for f in v['fields']]
        for f in fields:
            identifier(f['name'])
            if f['name'] in {'kind', 'read', 'wire'}: raise ValueError('Reserved scene field')
            if f['default'] is not None and f['default'] != {'kind': 'null'}:
                raise ValueError('Unreviewed scene default')
            visit(f['shape'])
    for s in contract['roots']: visit(s)
    if set(types) != set(bindings): raise ValueError('Unused scene binding')
    def name(s): return bindings[s['name']]['name']
    def typ(s):
        k = s['kind']
        if k in {'string', 'bool', 'f32', 'u32', 'i32'}: return dict(string='str', bool='bool', f32='float', u32='int', i32='int')[k]
        if k == 'optional': return typ(s['item']) + ' | None'
        if k == 'sequence': return 'tuple[' + typ(s['item']) + ', ...]'
        if k == 'array': return 'tuple[' + ', '.join([typ(s['item'])] * s['length']) + ']'
        return name(s)
    def wire(s, e):
        k = s['kind']
        if k in {'string', 'bool', 'f32', 'u32', 'i32'}: return e
        if k == 'optional': return f'None if {e} is None else {wire(s["item"], e)}'
        if k in {'array', 'sequence'}: return '[' + wire(s['item'], 'item') + f' for item in {e}]'
        if k == 'record': return 'dict(' + ', '.join(f['name'] + '=' + wire(f['shape'], e + '.' + f['name']) for f in s['fields']) + ')'
        raise ValueError('Unreviewed nested scene union')
    def read(s, e):
        k=s['kind']
        if k in {'string','f32','u32','i32','bool'}: return {'string':'string','f32':'number','u32':'integer','i32':'integer','bool':'_boolean'}[k]+'('+e+')'
        if k=='optional': return 'None if '+e+' is None else '+read(s['item'],e)
        if k in {'sequence','array'}: return 'tuple('+read(s['item'],'item')+' for item in sequence('+e+'))'
        return name(s)+'.read('+e+')'
    def check(s, e, indent, depth=0):
        p=' '*indent; k=s['kind']
        if k == 'string': return [p+f'if not isinstance({e}, str):', p+'    raise TypeError("Expected text")']
        if k == 'bool': return [p+f'if type({e}) is not bool:', p+'    raise TypeError("Expected boolean")']
        if k == 'f32': return [p+f'if isinstance({e}, bool) or not isinstance({e}, (int, float)):', p+'    raise TypeError("Expected a number")', p+f'if not math.isfinite({e}) or abs({e}) > 3.4028234663852886e38:', p+'    raise ValueError("Expected finite f32")']
        if k == 'i32': return [p+f'if type({e}) is not int or not -(1 << 31) <= {e} < (1 << 31):', p+'    raise ValueError("Expected i32")']
        if k == 'u32': return [p+f'if type({e}) is not int or not 0 <= {e} <= 0xffffffff:', p+'    raise ValueError("Expected u32")']
        if k == 'optional': return [p+f'if {e} is not None:']+check(s['item'], e, indent+4, depth)
        if k in {'sequence', 'array'}:
            lines=[p+f'if not isinstance({e}, tuple):',p+'    raise TypeError("Expected owned tuple")']
            if k == 'array': lines += [p+f'if len({e}) != {s["length"]}:',p+'    raise ValueError("Invalid fixed array length")']
            v='item'+str(depth)
            return lines+[p+f'for {v} in {e}:']+check(s['item'], v, indent+4, depth+1)
        if k == 'record':
            lines=[p+f'if not isinstance({e}, {name(s)}):',p+'    raise TypeError("Expected '+name(s)+'")']
            for f in s['fields']: lines += check(f['shape'], e+'.'+f['name'], indent, depth)
            return lines
        raise ValueError('Unreviewed scene check')
    def fields(s): return sorted(s, key=lambda f: f['shape']['kind']=='optional')
    def freeze(s, e, depth=0):
        if s['kind']=='optional': return 'None if '+e+' is None else '+freeze(s['item'], e, depth)
        if s['kind'] in {'sequence','array'}:
            v='item'+str(depth)
            return 'tuple('+freeze(s['item'], v, depth+1)+' for '+v+' in '+e+')'
        return e
    py=[header.rstrip(), 'from dataclasses import dataclass, field', 'from typing import Literal', 'import math', 'from .._values import record, string', '', 'def _boolean(value: object) -> bool:', '    if type(value) is not bool:', '        raise TypeError("Expected boolean")', '    return value', '']
    def record(n, fs, tag=None):
        py.extend(['@dataclass(frozen=True)', 'class '+n+':', '    __module__ = "zipzi.scene"'])
        for f in fields(fs): py.append('    '+f['name']+': '+typ(f['shape'])+(' = None' if f['shape']['kind']=='optional' else ''))
        if tag is not None: py.append('    kind: Literal["scene_'+tag+'"] = field(default="scene_'+tag+'", init=False)')
        arrays=[f for f in fs if f['shape']['kind'] in {'sequence', 'array'}]
        optional_arrays=[f for f in fs if f['shape']['kind']=='optional' and f['shape']['item']['kind'] in {'sequence','array'}]
        if arrays or optional_arrays:
            py.extend(['', '    def __post_init__(self) -> None:'])
            for f in arrays: py.append('        object.__setattr__(self, "'+f['name']+'", '+freeze(f['shape'], 'self.'+f['name'])+')')
            for f in optional_arrays:
                py.extend(['        if self.'+f['name']+' is not None:', '            object.__setattr__(self, "'+f['name']+'", '+freeze(f['shape']['item'], 'self.'+f['name'])+')'])
        py.append('')
    for s in reversed(list(types.values())):
        if s['kind']=='record':
            record(name(s), s['fields'])
            if name(s)=='Summary':
                py += ['    @classmethod', '    def read(cls, value: object) -> "Summary":', '        r = record(value)', '        return cls('+', '.join(f['name']+'='+read(f['shape'], 'r["'+f['name']+'"]') for f in s['fields'])+')', '']
    union=contract['roots'][0]
    if union['kind'] != 'union': raise ValueError('Expected scene operation union')
    for v in union['variants']: record(v['rust_name'], v['fields'], v['wire_name'])
    py += ['Operation = '+' | '.join(v['rust_name'] for v in union['variants']), '', 'OPERATION_TYPES = ('+', '.join(v['rust_name'] for v in union['variants'])+',)', '', 'def validate(op: Operation) -> None:', '    match op:']
    for v in union['variants']:
        py += ['        case '+v['rust_name']+'():']
        for f in v['fields']: py += check(f['shape'], 'op.'+f['name'], 12)
    py += ['        case _:', '            raise TypeError("Expected scene operation")', '', 'def wire(op: Operation) -> dict[str, object]:', '    match op:']
    for v in union['variants']:
        py += ['        case '+v['rust_name']+'():', '            return dict(domain="scene", operation=dict(op="'+v['wire_name']+'", '+', '.join(f['name']+'='+wire(f['shape'], 'op.'+f['name']) for f in v['fields'])+'))']
    py += ['    raise TypeError("Expected scene operation")', '']
    def native(s, e):
        k=s['kind']
        if k == 'sequence' and s['item']['kind']=='record':
            return e+'.try_iter()?.map(|item| -> PyResult<_> { Ok('+native(s['item'], 'item?')+') }).collect::<PyResult<_>>()?'
        if k in {'string', 'bool', 'f32', 'u32', 'i32', 'array', 'sequence'}: return e+'.extract()?'
        if k=='optional' and s['item']['kind'] in {'string','bool','f32','u32','i32','array','sequence'}: return e+'.extract()?'
        if k=='record': return 'read_'+name(s).lower()+'(&'+e+')?'
        raise ValueError('Unreviewed scene native shape')
    rust=[rust_header.rstrip(), 'use crate::*;', '']
    for s in types.values():
        if s['kind']!='record': continue
        rust += ['fn read_'+name(s).lower()+"(value: &Bound<'_, PyAny>) -> PyResult<"+bindings[s['name']]['path']+'> {', '    Ok('+bindings[s['name']]['path']+' {']
        for f in s['fields']: rust.append('        '+f['name']+': '+native(f['shape'], 'value.getattr("'+f['name']+'")?')+',')
        rust += ['    })', '}', '']
    rust += ["pub fn read_operation(value: &Bound<'_, PyAny>) -> PyResult<zipzi_sdk::SceneOperation> {", '    match value.getattr("kind")?.extract::<String>()?.as_str() {']
    for v in union['variants']:
        rust += ['        "scene_'+v['wire_name']+'" => Ok(zipzi_sdk::SceneOperation::'+v['rust_name']+' {']
        for f in v['fields']: rust.append('            '+f['name']+': '+native(f['shape'], 'value.getattr("'+f['name']+'")?')+',')
        rust += ['        }),']
    rust += ['        _ => Err(pyo3::exceptions::PyTypeError::new_err("Expected scene operation")),', '    }', '}', '']
    summary = next(s for s in types.values() if name(s)=='Summary')
    rust += ["pub fn write_summary(py: Python<'_>, value: zipzi_sdk::SceneSummary) -> PyResult<Bound<'_, PyDict>> {", '    let result = PyDict::new(py);']
    rust += ['    result.set_item("'+f['name']+'", value.'+f['name']+')?;' for f in summary['fields']]
    rust += ['    Ok(result)', '}', '']
    def measure(s,e,d=0):
        k=s['kind'];e='('+e+')'
        if k=='optional':return 'if '+e+'.is_none() {0usize} else {'+measure(s['item'],e,d)+'}'
        if k=='string':return e+'.len()?.saturating_mul(4).saturating_add(32)'
        if k in {'bool','f32','u32','i32'}:return '16usize'
        if k in {'array','sequence'}:
            v='value'+str(d)
            return '{let mut bytes=64usize;for value in '+e+'.try_iter()? {let '+v+'=value?;bytes=bytes.saturating_add('+measure(s['item'],v,d+1)+');}bytes}'
        return 'measure_'+name(s).lower()+'(&'+e+')?'
    for s in types.values():
        if s['kind']!='record':continue
        expr='256usize'
        for f in s['fields']:expr+='.saturating_add('+measure(f['shape'],'value.getattr("'+f['name']+'")?')+')'
        rust += ['fn measure_'+name(s).lower()+"(value:&Bound<'_,PyAny>)->PyResult<usize> {Ok("+expr+')}','']
    rust += ["pub fn measure_operation(value:&Bound<'_,PyAny>)->PyResult<usize> {",'    match value.getattr("kind")?.extract::<String>()?.as_str() {']
    for v in union['variants']:
        expr='256usize'
        for f in v['fields']:expr+='.saturating_add('+measure(f['shape'],'value.getattr("'+f['name']+'")?')+')'
        rust += ['        "scene_'+v['wire_name']+'"=>Ok('+expr+'),']
    rust += ['        _=>Err(pyo3::exceptions::PyTypeError::new_err("Expected scene operation")),','    }','}','']
    return {'client/python/zipzi/_generated/scene.py':'\n'.join(py), 'src/generated/scene.rs':'\n'.join(rust)}
