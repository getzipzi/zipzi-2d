"""Mechanical emitters for the selected resource/Plan Rust shapes.

Ownership, allocation admission, recovery and transaction semantics are handwritten.
"""
TARGETS = frozenset({
    'client/python/zipzi/_generated/resource_values.py',
    'client/python/zipzi/_generated/plan_values.py',
    'client/python/zipzi/_generated/plan_intents.py',
    'client/python/zipzi/_generated/resource_ops.py',
    'client/python/zipzi/_generated/resource_backends.py',
    'src/generated/resources.rs',
    'src/generated/resource_queries.rs',
})

def render_resources(contract, Model, header, rust_header, resolve=lambda shape: shape):
    m = Model(contract, resources=True, resolve=resolve)
    selected = {b['name']: m.types[b['rust']] for b in m.bindings.values() if b['rust'] in m.types}
    resource_union = selected['ResourceOperation']
    intent_union = selected['Intent']
    if [(o['name'], o['backend'], o['category']) for o in contract['operations']] != [
            ('image_snapshot', 'image', 'query'), ('prepared_pixels', 'prepared_pixels', 'query'),
            ('plan_authoring', 'plan', 'planning')]:
        raise ValueError('Unreviewed resource registration')
    if [v['rust_name'] for v in resource_union['variants']] != ['CreateImage', 'ReplaceImage', 'RenameImage', 'ApplyPlan']:
        raise ValueError('New resource variant requires a convenience policy')
    if [v['rust_name'] for v in intent_union['variants']] != ['MeshRename', 'ImageRename']:
        raise ValueError('New plan intent requires public naming review')

    def typ(s):
        s=resolve(s)
        k=s['kind']
        if k in {'string','u32','u64','usize','f32','bool','bytes'}:
            return dict(string='str',u32='int',u64='int',usize='int',f32='float',bool='bool',bytes='bytes')[k]
        if k=='optional': return typ(s['item'])+' | None'
        if k=='sequence': return 'tuple['+typ(s['item'])+', ...]'
        if k=='string_map': return 'tuple[tuple[str, str], ...]'
        b=m.binding(s)
        if b['form']=='external': return 'modeling.'+b['name']
        if b['form']=='handle': return b['module']+'.'+b['name']
        return b['name']

    def read(s,e):
        s=resolve(s)
        k=s['kind']
        if k in {'string','u32','u64','usize','f32','bool','bytes'}:
            return dict(string='string',u32='integer',u64='integer',usize='integer',f32='number',bool='_boolean',bytes='_bytes')[k]+'('+e+')'
        if k=='optional': return 'None if '+e+' is None else '+read(s['item'],e)
        if k=='sequence': return 'tuple('+read(s['item'],'item')+' for item in sequence('+e+'))'
        if k=='string_map': return 'tuple((key, string(item)) for key, item in sorted(record('+e+').items()))'
        if m.binding(s)['form']=='external':return 'modeling_codec.read_'+m.binding(s)['name']+'('+e+')'
        return m.binding(s)['name']+'.read('+e+')'

    output={}
    common=[header.rstrip(),'from dataclasses import dataclass','from .._values import Source, Version, integer, record, sequence, string','']
    for module,path in [('resources','resource_values'),('plans','plan_values')]:
        lines=common.copy()
        if module=='resources':
            lines += ['def _bytes(value: object) -> bytes:', '    return value if isinstance(value, bytes) else bytes(integer(v) for v in sequence(value))','']
        else:
            lines += ['from . import values as modeling, codecs as modeling_codec', '', 'def _boolean(value: object) -> bool:', '    if type(value) is not bool:', '        raise TypeError("Expected a boolean")','    return value','']
        for s in m.records:
            b=m.binding(s)
            if b['module']!=module: continue
            # ImageSpec is internal construction metadata; no new public stub.
            if b['name']=='ImageSpec': continue
            fields=[f for f in s['fields'] if b['form']!='snapshot' or f['name']!='source'];context=b['name']=='PlanReport'
            lines+=['@dataclass(frozen=True)',f'class {b["name"]}:','    __module__ = "zipzi'+('.resources' if module=='resources' else '')+'"']
            if context:lines+=['    source: Source']
            lines += ['    '+f['name']+': '+typ(f['shape']) for f in fields]
            lines += ['', '    @classmethod', f'    def read(cls, {"source: object, " if context else ""}value: object) -> "{b["name"]}":']
            if b['form']=='snapshot': lines += ['        if isinstance(value, cls): return value']
            lines += ['        r = record(value)']
            args=['Source.read(source)'] if context else []
            args += [read(f['shape'],f'r["{f["name"]}"]') for f in fields]
            lines+=['        return cls('+', '.join(args)+')','']
        # Avoid unused imports in each strictly typed generated module.
        if module=='resources':lines[2]='from .._values import integer, record, sequence, string'
        output['client/python/zipzi/_generated/'+path+'.py']='\n'.join(lines)

    intents=[header.rstrip(),'from dataclasses import dataclass, field','from typing import Literal','']
    for v in intent_union['variants']:
        intents += ['@dataclass(frozen=True)',f'class {v["rust_name"]}:', '    __module__ = "zipzi._generated.plan_intents"']
        intents += ['    '+f['name']+': '+typ(f['shape']) for f in v['fields']]
        intents += [f'    kind: Literal["{v["wire_name"]}"] = field(default="{v["wire_name"]}", init=False)','']
    intents += ['']
    intents+=['Intent = '+' | '.join(v['rust_name'] for v in intent_union['variants']),'','def validate(intent: Intent) -> None:','    match intent:']
    for v in intent_union['variants']:
        intents += [f'        case {v["rust_name"]}():']
        for f in v['fields']:intents+=m.checks(f['shape'],'intent.'+f['name'],12,[0])
    intents+=['        case _:','            raise TypeError("Expected a typed Plan intent")','','def wire(intent: Intent) -> dict[str, object]:','    match intent:']
    for v in intent_union['variants']:
        intents += [f'        case {v["rust_name"]}():', '            return dict(kind="'+v['wire_name']+'", '+', '.join(f['name']+'=intent.'+f['name'] for f in v['fields'])+')']
    intents+=['    raise TypeError("Expected a typed Plan intent")','']
    output['client/python/zipzi/_generated/plan_intents.py']='\n'.join(intents)

    # Flatten only the reviewed ImageCreateSpec, sorting defaults last. This
    # preserves CreateImage(name, image, id=None) without duplicating fields.
    def projected_fields(v):
        flat=[]
        for f in v['fields']:
            if f['name']=='spec':flat+=resolve(f['shape'])['fields']
            else:flat.append(f)
        return sorted(flat,key=lambda f:f['default'] is not None)
    def default_suffix(field):
        if field['name']=='expected_name': return ' = field(default=None, kw_only=True)'
        default = field['default']
        if default is None:
            return ''
        return ' = ' + {'null': 'None', 'empty_string': '""'}[default['kind']]
    ops=[header.rstrip(),'from dataclasses import dataclass, field','from typing import Literal',
         'from .. import resources', 'from .. import _plans as plans','']
    for v in resource_union['variants']:
        ops+=['@dataclass(frozen=True)',f'class {v["rust_name"]}:','    __module__ = "zipzi.'+('_generated.resource_ops' if v['rust_name']=='ApplyPlan' else 'resources')+'"']
        ops+=['    '+f['name']+': '+typ(f['shape'])+default_suffix(f) for f in projected_fields(v)]
        ops+=[f'    kind: Literal["{v["wire_name"]}"] = field(default="{v["wire_name"]}", init=False)','']
    ops+=['ResourceOperation = '+' | '.join(v['rust_name'] for v in resource_union['variants']),'',
          'def validate(op: ResourceOperation) -> None:', '    match op:']
    for v in resource_union['variants']:
        ops += [f'        case {v["rust_name"]}():']
        for f in projected_fields(v):
            s=resolve(f['shape']);e='op.'+f['name']
            if s['kind']=='opaque':
                b=m.binding(s);ops += [f'            if not isinstance({e}, {b["module"]}.{b["name"]}):',f'                raise TypeError("Expected a {b["name"]}")']
            else:ops+=m.checks(s,e,12,[0])
    ops+=['        case _:','            raise TypeError("Unknown resource operation")','','def wire(op: ResourceOperation) -> dict[str, object]:','    match op:']
    def wire_field(f):
        s=resolve(f['shape']);e='op.'+f['name']
        if f['name']=='spec':return 'dict('+', '.join(x['name']+'=op.'+x['name'] for x in s['fields'])+')'
        if s['kind']=='opaque':return e+'._token'
        return e
    for v in resource_union['variants']:
        ops += [f'        case {v["rust_name"]}():','            return dict(domain="'+v['wire_name']+'", '+', '.join(f['name']+'='+wire_field(f) for f in v['fields'])+')']
    ops+=['    raise TypeError("Unknown resource operation")','']
    output['client/python/zipzi/_generated/resource_ops.py']='\n'.join(ops)

    def rust_name(s):return resolve(s)['name'].split('::')[-1].lower()
    def native_read(s,e):
        s=resolve(s)
        if s['kind']=='opaque':
            b=m.binding(s)
            if b['form']=='handle':return e+'.getattr("_token")?.extract::<PyRef<\'_, '+('ImageToken' if b['name']=='PreparedImage' else 'PlanToken')+'>>()?.id'
        if s['kind'] in {'string','u32','u64','usize','bool','f32','optional'}:return e+'.extract()?'
        if s['kind']=='union':return 'read_'+rust_name(s)+'(&'+e+')?'
        raise ValueError('Unreviewed native input')
    def native_write(s,e):
        s=resolve(s)
        k=s['kind']
        if k in {'string','u32','u64','usize','bool','f32','sequence','string_map'}:return e
        if k=='optional':return e+'.map(|value| -> PyResult<_> { Ok('+native_write(s['item'],'value')+') }).transpose()?'
        if k=='bytes':return 'owned_pixels(py, &'+e+')'
        b=m.binding(s)
        if b['form']=='external':return 'super::modeling::write_'+rust_name(s)+'(py, &'+e+')?'
        if b['form'] in {'source','version'}:return b['form']+'(py, '+e+')?'
        if b['form']=='handle':return 'Py::new(py, '+('ImageToken' if b['name']=='PreparedImage' else 'PlanToken')+' { id: '+e+' })?'
        return 'write_'+rust_name(s)+'(py, '+e+')?'
    rust=[rust_header.rstrip(),'use crate::*;','',
          "fn owned_pixels<'py>(py: Python<'py>, bytes: &[u8]) -> Bound<'py, PyBytes> {",
          '    zipzi_runtime::metrics::copy_payload(bytes.len());','    PyBytes::new(py, bytes)','}','']
    for s in m.records:
        b=m.binding(s)
        if b['name']=='ImageSpec':continue
        rust += [f'pub(crate) fn write_{rust_name(s)}(py: Python<\'_>, value: {b["path"]}) -> PyResult<Bound<\'_, PyDict>> {{','    let d = PyDict::new(py);']
        for f in s['fields']:rust += [f'    d.set_item("{f["name"]}", {native_write(f["shape"], "value."+f["name"])})?;']
        rust += ['    Ok(d)','}','']
    def native_measure(s,e):
        s=resolve(s)
        k=s['kind']
        if k=='string':return 'input_budget::text(&'+e+')?'
        if k=='optional':return '{ let v='+e+'; if v.is_none() { 0 } else { '+native_measure(s['item'],'v')+' } }'
        if k=='union':return 'measure_'+rust_name(s)+'(&'+e+')?'
        if k=='opaque' or k in {'u32','u64','usize','bool','f32'}:return '64usize'
        if k=='record':return '256usize'+''.join('.saturating_add('+native_measure(f['shape'],e+'.getattr("'+f['name']+'")?')+')' for f in s['fields'])
        raise ValueError('Unreviewed native input admission')
    for union in [resource_union,intent_union]:
        b=m.binding(union)
        rust += [f'pub(crate) fn measure_{rust_name(union)}(op: &Bound<\'_, PyAny>) -> PyResult<usize> {{','    let kind=op.getattr("kind")?;','    match kind.extract::<&str>()? {']
        for v in union['variants']:
            cost='256usize'
            for f in v['fields']:
                if f['name']=='spec':
                    cost+=''.join('.saturating_add('+native_measure(x['shape'],'op.getattr("'+x['name']+'")?')+')' for x in resolve(f['shape'])['fields'])
                else:cost+='.saturating_add('+native_measure(f['shape'],'op.getattr("'+f['name']+'")?')+')'
            rust += ['        "'+v['wire_name']+'" => Ok('+cost+'),']
        rust += ['        _ => Err(pyo3::exceptions::PyTypeError::new_err("Unknown typed input")),','    }','}','']
        rust += [f'pub(crate) fn read_{rust_name(union)}(op: &Bound<\'_, PyAny>) -> PyResult<{b["path"]}> {{', '    match op.getattr("kind")?.extract::<String>()?.as_str() {']
        for v in union['variants']:
            rust += [f'        "{v["wire_name"]}" => Ok({b["path"]}::{v["rust_name"]} {{']
            for f in v['fields']:
                if f['name']=='spec':
                    expression=m.binding(f['shape'])['path']+' { '+', '.join(x['name']+': '+native_read(x['shape'],f'op.getattr("{x["name"]}")?') for x in resolve(f['shape'])['fields'])+' }'
                else:expression=native_read(f['shape'],f'op.getattr("{f["name"]}")?')
                rust += ['            '+f['name']+': '+expression+',']
            rust+=['        }),']
        rust+=['        _ => Err(pyo3::exceptions::PyTypeError::new_err("Unknown typed intent")),','    }','}','']
    output['src/generated/resources.rs']='\n'.join(rust)

    def param(s,native=False):
        s=resolve(s)
        if s['kind']=='optional':return ('Option<'+param(s['item'],True)+'>' if native else param(s['item'])+' | None')
        if s['kind']=='string':return '&str' if native else 'str'
        if s['kind']=='opaque':
            b=m.binding(s);token={'PreparedImage':'ImageToken','Plan':'PlanToken'}.get(b['name'],b['name'])
            return '&'+token if native else 'object'
        if s['kind']=='union':return "&Bound<'_, PyAny>" if native else 'Intent'
        raise ValueError('Unreviewed query parameter')
    back=[header.rstrip(),'from typing import Protocol','from .plan_intents import Intent','']
    for cls,owner in [('ResourceBackend',True),('NativeResources',False)]:
        back+=['class '+cls+'(Protocol):']
        for o in contract['operations']:
            params=', '.join(f['name']+': '+param(f['shape']) for f in resolve(o['input'])['fields'])
            back += [f'    def {o["backend"]}(self, {params}{", owner: object" if owner else ""}) -> object: ...']
        back+=['']
    labels={'image':'resources.get','prepared_pixels':'resources.pixels','plan':'plan.create'}
    queries=[rust_header.rstrip(),'use crate::*;','use super::resources::*;','#[pymethods]','impl NativeRuntime {']
    for o in contract['operations']:
        fields=resolve(o['input'])['fields'];params=', '.join(f['name']+': '+param(f['shape']) for f in fields)
        native_params=', '.join(f['name']+': '+param(f['shape'],True) for f in fields)
        if o['backend']=='image':queries += ['    #[pyo3(signature = (session, id, edit=None))]']
        queries += [f'    fn {o["backend"]}(&self, py: Python<\'_>, {native_params}) -> PyResult<Py<PyDict>> {{']
        costs=[]
        for f in fields:
            if resolve(f['shape'])['kind']=='string':costs.append(f['name']+'.len().saturating_mul(4).saturating_add(128)')
            elif resolve(f['shape'])['kind']=='union':costs.append('measure_'+rust_name(f['shape'])+'('+f['name']+')?')
        if costs:
            cost='256usize'+''.join('.saturating_add('+e+')' for e in costs)
            queries += [f'        let _input=input_lease!(self,py,{cost},"{labels[o["backend"]]}",None);']
        queries += ['        let client = self.client;',f'        let input = {m.binding(o["input"])["path"]} {{']
        for f in fields:
            s=resolve(f['shape']);n=f['name'];expr=n
            if s['kind']=='opaque':expr=n+'.id'
            elif s['kind']=='optional':expr=n+'.map(|v| v.id)'
            elif s['kind']=='union':expr='read_'+rust_name(s)+'('+n+')?'
            elif s['kind']=='string':expr=n+'.to_owned()'
            queries+=['            '+n+': '+expr+',']
        if o['backend'] in {'image', 'prepared_pixels'}:
            count = 'v.pixels.rgba.len()' if o['backend']=='image' else 'v.rgba.len()'
            queries+=['        };',f'        let value = py.detach(|| self.host.call(move |rt| {{ let v = {o["handler"]}.call(rt, client, input)?; let lease = rt.reserve_transfer({count})?; Ok((v, lease)) }}));',
                      f'        reply(py, value, "{labels[o["backend"]]}", |(v, _lease)| Ok(write_{rust_name(o["output"])}(py, v)?.into_any()))','    }']
        else:
            queries+=['        };',f'        let value = py.detach(|| self.host.call(move |rt| {o["handler"]}.call(rt, client, input)));',
                      f'        reply(py, value, "{labels[o["backend"]]}", |v| Ok(write_{rust_name(o["output"])}(py, v)?.into_any()))','    }']
    queries+=['}',''];back+=['']
    output['client/python/zipzi/_generated/resource_backends.py']='\n'.join(back).rstrip()+'\n'
    output['src/generated/resource_queries.rs']='\n'.join(queries)
    return output
