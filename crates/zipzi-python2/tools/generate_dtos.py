"""Finite public DTO projections; every field/type comes from compiled Rust."""
import re
from copy import deepcopy

TARGETS = frozenset('client/python/zipzi/_generated/'+name+'_values.py' for name in
                    ('parameter', 'binding', 'sampling', 'reference', 'event')) | frozenset({'client/python/zipzi/_generated/planning.py'})

PRELUDE = '''from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from enum import Enum
from typing import ClassVar, Literal, TypeAlias, cast
from .._dto_support import get as _get, text as _text, numeric as _numeric, external as _external, whole as _whole, flag as _flag, point as _point, mapping as _mapping, cursor_guard as _cursor_guard
from .._values import Version, SessionId, Receipt, SaveReceipt, SessionInfo
'''

NAMES = {
 'parameters': {'ParameterCreateSpec':'ParameterSpec','ParameterPatch':'ParameterPatch','ParameterKind':'Kind','Parameter':'Parameter','ParameterOperation':'Operation'},
 'bindings': {'MeshBindingAxisSpec':'Axis','MeshKeyformSpec':'Keyform','MeshBindingCreateSpec':'MeshBindingSpec','MeshBinding':'MeshBinding','BindingAxis':'AxisSnapshot','BindingSample':'Sample','MeshKeyform':'KeyformSnapshot','Appearance':'Appearance','MeshBindingOperation':'Operation','Vec2':'Point'},
 'sampling': {'GeometryChecks':'Checks','GeometryBounds':'Bounds','ModelingSamples':'Result','DrawableFrame':'Frame','EvaluatedParameter':'Parameter','Drawable':'Drawable','OffscreenFrame':'Offscreen','EvaluatedDeformer':'Deformer','RenderCommand':'RenderCommand','GeometryDiagnostic':'Diagnostic','GeometryDiagnosticKind':'DiagnosticKind','BlendMode':'BlendMode','Vec2':'Point','Version':'Version'},
 'references': {'SceneCapabilities':'SceneCapabilities','CapabilityState':'Capability','ObjectKind':'ObjectKind'},
 'events': {'EventCursor':'Cursor','ClientId':'Origin'},
}


def short(name):
    return name.rsplit('::', 1)[-1]


def opaque(shape):
    if shape['kind'] != 'opaque': return shape
    name = shape['name']
    for wrapper, kind in [('Option', 'optional'), ('Arc', 'shared')]:
        match = re.fullmatch(r'.*\b'+wrapper+r'<(.+)>', name)
        if match:return {'kind':kind,'item':opaque({'kind':'opaque','name':match[1]})}
    base = short(name)
    primitive = {'&str':'string','RuntimeInstanceId':'string', **{x:x for x in ['u64','u32','i32','usize','f32','f64','bool']}}
    return {'kind':primitive[base]} if base in primitive else shape


def render_dtos(contract, header):
    if contract['version'] != 1:raise ValueError('Unknown DTO contract')
    output = {}
    for domain, filename in [('parameters','parameter'),('bindings','binding'),('sampling','sampling'),('references','reference'),('events','event')]:
        bindings = NAMES[domain]
        shapes = {}
        roots = list(contract[domain]) if domain != 'events' else [contract['events']['cursor'],contract['events']['origin']]
        def visit(s):
            s=opaque(s);k=s['kind']
            if k in {'serde_record','record','serde_enum','union'}:
                name=short(s['name'])
                if name not in bindings:raise ValueError('Unselected '+domain+' DTO: '+name)
                if name in shapes:
                    if shapes[name]!=s:raise ValueError('Conflicting DTO shape')
                    return
                for f in s.get('fields',[]):
                    if not f.get('skip'):visit(f['shape'])
                for v in s.get('variants',[]):
                    for f in v['fields']:
                        if not f.get('skip'):visit(f['shape'])
                shapes[name]=s
            elif k in {'optional','sequence','array','shared'}:visit(s['item'])
            elif k=='string_map':visit(s['value'])
            elif k=='opaque':
                if short(s['name']) not in {'Version','SessionId','EditReceipt','SaveReceipt','SessionInfo','ClientId','JobId','OperationId'}:raise ValueError('Unbound DTO leaf '+s['name'])
            elif k not in {'string','bool','f32','f64','u8','u16','u32','u64','i32','i64','usize'}:raise ValueError('Unsupported DTO shape '+k)
        for s in roots:visit(s)
        def name(s):return bindings[short(s['name'])]
        def fields(s):return [f for f in s.get('fields',[]) if not f.get('skip')]
        def variants(s):return [dict(v,name=v.get('name',v.get('wire_name'))) for v in s['variants']]
        def unit(s):return s['kind'] in {'serde_enum','union'} and all(not v['fields'] for v in variants(s))
        external={'Version':'Version','SessionId':'SessionId','EditReceipt':'Receipt','SaveReceipt':'SaveReceipt','SessionInfo':'SessionInfo','ClientId':'Origin','JobId':'Origin','OperationId':'Origin','EventCursor':'Cursor'}
        def typ(s, inputs=False):
            s=opaque(s);k=s['kind']
            if k in {'f32','f64'}:return 'float'
            if k in {'u8','u16','u32','u64','i32','i64','usize'}:return 'int'
            if k in {'string','bool'}:return {'string':'str','bool':'bool'}[k]
            if k=='shared':return typ(s['item'],inputs)
            if k=='optional':return typ(s['item'],inputs)+' | None'
            if k in {'sequence','array'}:
                if inputs:return 'Sequence['+typ(s['item'],True)+']'
                return 'tuple['+(typ(s['item'])+', ...' if k=='sequence' else ', '.join([typ(s['item'])]*s['length']))+']'
            if k=='string_map':return 'Mapping[str, '+typ(s['value'])+']'
            if k=='opaque':return external[short(s['name'])]
            n=name(s)
            return 'Point' if n=='Point' else 'tuple[Point, Point]' if n=='Bounds' else n
        def own(s,e):
            s=opaque(s);k=s['kind']
            if k=='optional':return 'None if '+e+' is None else '+own(s['item'],e)
            if k=='shared':return own(s['item'],e)
            if k in {'sequence','array'}:return 'tuple('+own(s['item'],'item')+' for item in '+e+')'
            if k in {'record','serde_record'} and name(s) in {'Point','Bounds'}:
                return 'tuple('+e+')' if name(s)=='Point' else 'tuple(tuple(item) for item in '+e+')'
            return e
        def read(s,e):
            s=opaque(s);k=s['kind']
            if k=='string':return '_text('+e+')'
            if k=='bool':return '_flag('+e+')'
            if k in {'u8','u16','u32','u64','i32','i64','usize'}:return '_whole('+e+', '+repr(k)+')'
            if k in {'f32','f64'}:return '_numeric('+e+', '+repr(k)+')'
            if k=='optional':return 'None if '+e+' is None else '+read(s['item'],e)
            if k=='shared':return read(s['item'],e)
            if k in {'sequence','array'}:
                value='tuple('+read(s['item'],'item')+' for item in _items('+e+'))'
                return 'cast('+repr(typ(s))+', _array('+value+', '+str(s['length'])+'))' if k=='array' else value
            if k=='opaque':return '_external('+e+', '+external[short(s['name'])]+', '+external[short(s['name'])]+'.read)'
            n=name(s)
            if n=='Point':return '_point('+e+')'
            if n=='Bounds':return '('+', '.join(read(f['shape'],'_get('+e+', '+repr(f['name'])+')') for f in fields(s))+')'
            if unit(s):return n+'(_text('+e+'))'
            if n=='Capability':return 'Capability.read('+e+')'
            return n+'.read('+e+')' if s['kind'] in {'record','serde_record'} else 'read_'+n+'('+e+')'
        def encode(s,e):
            s=opaque(s);k=s['kind']
            if k in {'string','bool','f32','f64','u8','u16','u32','u64','i32','i64','usize'}:return e
            if k=='shared':return encode(s['item'],e)
            if k=='optional':return 'None if '+e+' is None else '+encode(s['item'],e)
            if k in {'sequence','array'}:return '['+encode(s['item'],'item')+' for item in '+e+']'
            if k=='opaque':return e+'.wire()'
            n=name(s)
            if n=='Point':return '{"x": '+e+'[0], "y": '+e+'[1]}'
            if n=='Bounds':return '{'+', '.join(repr(f['name'])+': '+encode(f['shape'],e+'['+str(i)+']') for i,f in enumerate(fields(s)))+'}'
            if unit(s):return e+'.value'
            return 'encode_'+n+'('+e+')'
        def checks(s,e,indent=4):
            s=opaque(s);k=s['kind'];pad=' '*indent
            if k=='shared':return checks(s['item'],e,indent)
            if k=='optional':return [pad+'if '+e+' is not None:']+checks(s['item'],e,indent+4)
            if k in {'sequence','array'}:
                out=[]
                if k=='array':out+=[pad+'_array('+e+', '+str(s['length'])+')']
                return out+[pad+'for item in '+e+':']+checks(s['item'],'item',indent+4)
            if k=='opaque':return [pad+'if not isinstance('+e+', '+typ(s)+'):',pad+'    raise TypeError("Expected '+typ(s)+'")']
            if k in {'record','serde_record','union','serde_enum'}:
                n=name(s)
                if n=='Point':return [pad+'_point('+e+')']
                if n=='Bounds':return [pad+'_array('+e+', 2)']+sum((checks(f['shape'],e+'['+str(i)+']',indent) for i,f in enumerate(fields(s))),[])
                if unit(s):return [pad+'if not isinstance('+e+', '+n+'):',pad+'    raise TypeError("Expected '+n+'")']
                return [pad+'validate_'+n+'('+e+')']
            return [pad+read(s,e)]
        lines=[header.rstrip(),PRELUDE.rstrip(), 'from .._dto_support import items as _items, array as _array', '', 'Point = tuple[float, float]','']
        if domain=='sampling':
            sample=next(s for n,s in shapes.items() if n=='ModelingSamples')
            params=next(f for f in fields(sample) if f['name']=='parameters')['shape']['item']
            if params['kind']!='string_map':raise ValueError('Sample poses must remain typed maps')
            lines += ['@dataclass(frozen=True)', 'class Pose:', '    __module__ = "zipzi.sampling"',
                      '    values: tuple[tuple[str, '+typ(params['value'])+'], ...]', '', '    def __post_init__(self) -> None:', '        object.__setattr__(self, "values", tuple(tuple(item) for item in self.values))', '',
                      '    @classmethod', '    def read(cls, value: object) -> "Pose":', '        return cls(tuple(sorted((key, '+read(params['value'],'item')+') for key, item in _mapping(value).items())))','']
            lines.insert(lines.index('class Pose:')+1,'    '+repr('Owned effective parameter pose; reading it performs no IO.\n\nFields:\nvalues: Immutable parameter-ID/value pairs sorted by parameter ID; IDs are document parameter IDs and values use each parameter domain.'))
        def record_class(n,fs,kind=None,variant=None):
            lines.extend(['@dataclass(frozen=True, init=False)', 'class '+n+':','    __module__ = "zipzi.'+domain+'"'])
            missing=[f['name'] for f in fs if not f.get('doc','').strip()]
            if missing:raise ValueError('Missing semantic field documentation: '+n+'.'+', '.join(missing))
            text='Owned '+domain+' '+n+' value; reading fields, repr and summary performs no IO.\n\nFields:\n'+'\n'.join(f['name']+': '+f['doc'] for f in fs)
            lines.insert(len(lines)-1,'    '+repr(text))
            if kind is not None:lines.extend(['    kind: ClassVar[Literal['+repr(kind)+']] = '+repr(kind)])
            if variant is not None:lines.append('    _variant: ClassVar[str] = '+repr(variant))
            for f in fs:lines.append('    '+f['name']+': '+f.get('_type',typ(f['shape'])))
            input_names={'parameters':{'ParameterSpec','ParameterPatch','Create','Patch'},'bindings':{'Axis','Keyform','MeshBindingSpec','Create','SetKeyform'},'sampling':{'Checks'},'events':{'Cursor','Origin'},'references':set()}
            is_input=n in input_names[domain]
            for f in fs:
                if is_input and domain!='events' and opaque(f['shape'])['kind']=='optional':f['_default_present']=True;f['default']=None
                elif n=='Checks' and f.get('default') is not None:f['_default_present']=True
            defaults=lambda f: f.get('_default_present',False)
            ordered=sorted(fs,key=defaults)
            args=[]
            for f in ordered:
                default=' = '+repr(f['default']) if defaults(f) else ''
                args.append(f['name']+': '+f.get('_type',typ(f['shape'],True))+default)
            lines.extend(['', '    def __init__(self, '+', '.join(args)+') -> None:'])
            if not fs:lines.append('        pass')
            for f in fs:lines.append('        object.__setattr__(self, '+repr(f['name'])+', '+own(f['shape'],f['name'])+')')
            if domain=='events' and n=='Cursor':lines.append('        _cursor_guard(self.runtime, self.session, self.sequence)')
            lines.extend(['','    @classmethod', '    def read(cls, value: object) -> "'+n+'":','        if isinstance(value, cls): return value'])
            values=[]
            for f in fs:
                e='_get(value, '+repr(f['name'])+')'
                if domain=='sampling' and n=='Result' and f['name']!='state':e='_get(_get(value, "samples"), '+repr(f['name'])+')'
                if domain=='sampling' and n=='Result' and f['name']=='parameters':expr='tuple(Pose.read(item) for item in _items('+e+'))'
                else:expr=read(f['shape'],e)
                values.append(f['name']+'='+expr)
            lines.extend(['        return cls('+', '.join(values)+')', ''])
            if n=='Checks' or domain=='events' and n in {'Cursor','Origin'}:lines.extend(['    def wire(self) -> dict[str, object]:','        return encode_'+n+'(self)',''])
            if not is_input:return
            lines.extend(['def validate_'+n+'(value: '+n+') -> None:', '    if not isinstance(value, '+n+'):', '        raise TypeError("Expected '+n+'")'])
            for f in fs:
                if f.get('_type'):continue
                lines.extend(checks(f['shape'],'value.'+f['name']))
            lines.extend(['','def encode_'+n+'(value: '+n+') -> dict[str, object]:', '    return {'+', '.join(repr(f['name'])+': '+encode(f['shape'],'value.'+f['name']) for f in fs if not f.get('_type'))+'}',''])
        for rust,s in shapes.items():
            n=name(s)
            if n in {'Point','Bounds'}:continue
            if n=='ObjectKind':
                tags=[v['name'] for v in variants(s)]
                lines+=['from .modeling import ObjectKind as _ObjectKind','ObjectKind: TypeAlias = Literal['+', '.join(repr(tag) for tag in tags)+'] | _ObjectKind','OBJECT_KINDS = frozenset('+repr(tags)+')',''];continue
            if unit(s):
                lines+=['class '+n+'(str, Enum):','    __module__ = "zipzi.'+domain+'"']
                for v in variants(s):lines+=['    '+re.sub(r'(?<!^)(?=[A-Z])','_',v['rust_name']).upper()+' = '+repr(v['name'])]
                lines+=[''];continue
            if n=='Capability':
                vs=variants(s)
                if {v['rust_name'] for v in vs}!={'Available','Unavailable','Unsupported'}:raise ValueError('Capability states changed')
                lines+=['@dataclass(frozen=True)','class Capability:','    '+repr('Current native capability state; inspect it before invoking the operation.\n\nFields:\nstate: Available, temporarily unavailable, or unsupported for this object/domain.\ncode: Stable refusal reason; None for the available state.'),'    __module__ = "zipzi.references"','    state: Literal["available", "unavailable", "unsupported"]','    code: str | None = None','','    @classmethod','    def read(cls, value: object) -> "Capability":', '        tag, content = _variant(value)','        if tag == "Available": return cls("available")','        if tag not in {"Unavailable", "Unsupported"}: raise ValueError("Unknown capability")','        return cls("unavailable" if tag == "Unavailable" else "unsupported", _text(_get(content, "code")))',''];continue
            if n=='RenderCommand':
                vs=variants(s)
                if any(len(v['fields'])!=1 or v['fields'][0]['shape']['kind']!='string' for v in vs):raise ValueError('Unreviewed render command projection')
                lines+=['class RenderKind(str, Enum):','    __module__ = "zipzi.sampling"']
                for v in vs:lines+=['    '+re.sub(r'(?<!^)(?=[A-Z])','_',v['rust_name']).upper()+' = '+repr(v['name'])]
                lines+=['','@dataclass(frozen=True)','class RenderCommand:','    '+repr('One ordered native render-plan command.\n\nFields:\nkind: BeginOffscreen, DrawMesh or EndOffscreen; begin/end commands delimit compositing scopes.\nid: Stable mesh ID for DrawMesh, or Offscreen ID for begin/end commands.'),'    __module__ = "zipzi.sampling"','    kind: RenderKind','    id: str','','def read_RenderCommand(value: object) -> RenderCommand:', '    if isinstance(value, RenderCommand): return value','    tag, content = _variant(value)']
                for v in vs:lines+=['    if tag == '+repr(v['name'])+': return RenderCommand(RenderKind(tag), _text(_get(content, '+repr(v['fields'][0]['name'])+')))']
                lines+=['    raise ValueError("Unknown render command")',''];continue
            if s['kind'] in {'union','serde_enum'}:
                vs=variants(s);classes=[]
                for v in vs:
                    cls=v['rust_name'];classes.append(cls)
                    kind={'parameters':{'Create':'create_parameter','Patch':'patch_parameter'},'bindings':{'Create':'create_mesh_binding','SetKeyform':'set_mesh_keyform'}}[domain][cls]
                    record_class(cls,deepcopy(v['fields']),kind,v['name'])
                lines+=['Operation: TypeAlias = '+' | '.join(classes),'OPERATION_TYPES = ('+', '.join(classes)+',)','', 'def validate_Operation(value: Operation) -> None:']
                for cls in classes:lines+=['    if isinstance(value, '+cls+'): return validate_'+cls+'(value)']
                lines+=['    raise TypeError("Expected domain operation")','','def encode_Operation(value: Operation) -> dict[str, object]:']
                for v in vs:lines+=['    if isinstance(value, '+v['rust_name']+'): return dict(op='+repr(v['name'])+', **encode_'+v['rust_name']+'(value))']
                lines+=['    raise TypeError("Expected domain operation")',''];continue
            fs=deepcopy(fields(s))
            if domain=='sampling' and n=='Result':
                state=next(f for f in contract['sampling_outer']['fields'] if f['name']=='state')
                fs=[state]+fs
                next(f for f in fs if f['name']=='parameters')['_type']='tuple[Pose, ...]'
            if domain=='sampling' and n=='Checks':
                for f in fs:
                    if f.get('default')=={'kind':'null'}:f['default']=None
            # Normalize concrete serde defaults. Optional field metadata None means required.
            for f in fs:
                default=f.get('default')
                if default=={'kind':'null'}:f['default']=None
                elif default=={'kind':'empty_string'}:f['default']=''
            record_class(n,fs)
        if domain in {'sampling','references'}:
            lines+=['def _variant(value: object) -> tuple[str, object]:','    if isinstance(value, str): return value, {}','    tag = getattr(value, "_variant", None)','    if isinstance(tag, str): return tag, value','    raw = _mapping(value)','    if len(raw) != 1: raise ValueError("Expected a tagged value")','    tag = next(iter(raw))','    return tag, raw[tag]','']
        if domain=='sampling':
            lines+=['def inputs(poses: Sequence[Mapping[str, float]]) -> tuple[dict[str, float], ...]:','    result: list[dict[str, float]] = []','    for pose in poses:','        owned = dict(pose)','        for key, value in owned.items():','            _text(key)','            _numeric(value, '+repr(params['value']['kind'])+')','        result.append(owned)','    return tuple(result)','']
        if domain=='events':
            event=contract['events']['event'];common=[f for f in event['fields'] if f['name']!='kind'];classes=[]
            for variant in contract['events']['kind']['variants']:
                n=variant['rust_name'];classes.append(n)
                payload=opaque(variant['fields'][0]['shape'])
                payload_name=typ(payload)
                public='receipt' if payload_name in {'Receipt','SaveReceipt'} else 'info' if payload_name=='SessionInfo' else None
                if public is None:raise ValueError('Unselected event payload')
                fs=deepcopy(common)+[dict(variant['fields'][0],name=public)]
                kind=re.sub(r'(?<!^)(?=[A-Z])','_',n).lower()
                record_class(n,fs,kind)
            lines+=['Event: TypeAlias = '+' | '.join(classes),'','def read_event(value: object) -> Event:','    kind = _text(_get(value, "kind"))','    content = {name: _get(value, name) for name in '+repr(tuple(f['name'] for f in common))+'}']
            for variant in contract['events']['kind']['variants']:
                n=variant['rust_name'];payload=typ(variant['fields'][0]['shape']);field='receipt' if payload in {'Receipt','SaveReceipt'} else 'info';kind=re.sub(r'(?<!^)(?=[A-Z])','_',n).lower()
                lines+=['    if kind == '+repr(kind)+': return '+n+'.read(dict(content, '+field+'=_get(value, "value")))']
            lines+=['    raise ValueError("Unknown event kind")','']
        # Emit only imports actually used by this finite projection.
        text='\n'.join(lines)
        imports=[('from collections.abc import ',['Mapping','Sequence']),('from typing import ',['ClassVar','Literal','TypeAlias','cast']),('from .._values import ',['Version','SessionId','Receipt','SaveReceipt','SessionInfo']),('from .._dto_support import ',['get as _get','text as _text','numeric as _numeric','external as _external','whole as _whole','flag as _flag','point as _point','mapping as _mapping','cursor_guard as _cursor_guard'])]
        for prefix,names in imports:
            original=next(line for line in text.splitlines() if line.startswith(prefix))
            body=text.replace(original,'')
            used=[item for item in names if re.search(r'\b'+re.escape(item.split(' as ')[-1])+r'\b',body)]
            text=text.replace(original,prefix+', '.join(used) if used else '')
        if 'Enum)' not in text:text=text.replace('from enum import Enum\n','')
        if '_array(' not in text:text=text.replace('items as _items, array as _array','items as _items')
        if '_items(' not in text:text=text.replace('from .._dto_support import items as _items\n','')
        output['client/python/zipzi/_generated/'+filename+'_values.py']=text+'\n'
    # Public planning is a subset of the same Runtime commands, derived from the SDK intent registration.
    vs=contract['planning']['variants'];classes=['m.ModelingCommand'+v['rust_name'] for v in vs]
    tag,content=contract['planning']['tag'],contract['planning']['content']
    planning=[header.rstrip(),'from . import modeling as m','from .._values import record','', 'ModelingPlanOperation = '+' | '.join(classes),'MODELING_PLAN_TYPES = ('+', '.join(classes)+',)','', 'def encode_modeling(value: ModelingPlanOperation) -> dict[str, object]:','    raw = record(m.encode_ModelingCommand(value))','    return {'+repr(tag)+': raw["op"], '+repr(content)+': raw["intent"]}','']
    output['client/python/zipzi/_generated/planning.py']='\n'.join(planning)
    return output
