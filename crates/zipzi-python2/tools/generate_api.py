"""Finite Python binding declarations; lifecycle/transport hooks remain handwritten."""
import ast
import json
from pathlib import Path

PUBLIC_EXPORTS = {'meshes': ('Mesh values are generated; selected Plan intents are generated.',
            ['from ._generated.meshes import Geometry as Geometry, MeshSpec as '
             'MeshSpec, Point as Point, Mesh as Mesh, Meshing as Meshing, Constraint as Constraint, ConstraintKind as ConstraintKind',
             'from ._generated.mesh_ops import Create as Create, Rename as '
             'Rename, SetPositions as SetPositions',
             'from ._generated.discovery import ObjectSummary as Summary'],
            ['Geometry',
             'MeshSpec',
             'Point',
             'Mesh',
             'Meshing',
             'Constraint',
             'ConstraintKind',
             'Create',
             'Rename',
             'SetPositions',
             'Summary']),
 'parameters': ('Parameter values and operations projected from the shared '
                'Rust declarations.',
                ['from ._generated.parameter_values import Kind as Kind, '
                 'ParameterSpec as ParameterSpec, ParameterPatch as '
                 'ParameterPatch, Parameter as Parameter, Create as Create, '
                 'Patch as Patch',
                 'from ._generated.discovery import ObjectSummary as Summary'],
                ['Kind',
                 'ParameterSpec',
                 'ParameterPatch',
                 'Parameter',
                 'Create',
                 'Patch',
                 'Summary']),
 'bindings': ('Numeric binding inputs and canonical values from one Rust '
              'declaration.',
              ['from ._generated.binding_values import Axis as Axis, Keyform '
               'as Keyform, MeshBindingSpec as MeshBindingSpec, Sample as '
               'Sample, AxisSnapshot as AxisSnapshot, Appearance as '
               'Appearance, KeyformSnapshot as KeyformSnapshot, MeshBinding as '
               'MeshBinding, Create as Create, SetKeyform as SetKeyform',
               'from ._generated.discovery import ObjectSummary as Summary'],
              ['Axis',
               'Keyform',
               'MeshBindingSpec',
               'Sample',
               'AxisSnapshot',
               'Appearance',
               'KeyformSnapshot',
               'MeshBinding',
               'Create',
               'SetKeyform',
               'Summary']),
 'scene': ('Typed scene/organization intentions and immutable metadata '
           'projections.',
           ['from ._generated.scene import PartSpec as PartSpec, '
            'ParameterGroupSpec as ParameterGroupSpec, Placement as Placement, '
            'Summary as Summary, Operation as Operation, CreatePart as '
            'CreatePart, CreateParameterGroup as CreateParameterGroup, '
            'CreateWarp as CreateWarp, CreateRotation as CreateRotation, '
            'CreateGlue as CreateGlue, CreateOffscreen as CreateOffscreen, '
            'MoveToPart as MoveToPart, MoveDeformation as MoveDeformation, '
            'MoveParameters as MoveParameters, MoveParameterGroups as '
            'MoveParameterGroups, DuplicateScene as DuplicateScene, '
            'DeleteScene as DeleteScene, DeleteOrganization as '
            'DeleteOrganization, SetSceneLabel as SetSceneLabel, '
            'SetSceneEnabled as SetSceneEnabled, ReparentLocal as '
            'ReparentLocal, ReplaceTriangles as ReplaceTriangles, '
            'ReorderMeshVertices as ReorderMeshVertices, InsertMeshVertex as '
            'InsertMeshVertex, DeleteMeshVertices as DeleteMeshVertices, '
            'CreateBlendTable as CreateBlendTable, CreateMeshBlend as '
            'CreateMeshBlend, SetMeshBlendVertices as SetMeshBlendVertices, '
            'InsertGluePair as InsertGluePair, DeleteGluePairs as '
            'DeleteGluePairs, PatchOffscreenKeyform as PatchOffscreenKeyform, '
            'VertexWeight as VertexWeight, InsertOffscreenKeyform as '
            'InsertOffscreenKeyform, SetOffscreenStaticKeyform as '
            'SetOffscreenStaticKeyform, VertexPatch as VertexPatch, Sample as '
            'Sample, GluePair as GluePair, OffscreenForm as OffscreenForm'],
           ['PartSpec',
            'ParameterGroupSpec',
            'Placement',
            'Summary',
            'Operation',
            'CreatePart',
            'CreateParameterGroup',
            'CreateWarp',
            'CreateRotation',
            'CreateGlue',
            'CreateOffscreen',
            'MoveToPart',
            'MoveDeformation',
            'MoveParameters',
            'MoveParameterGroups',
            'DuplicateScene',
            'DeleteScene',
            'DeleteOrganization',
            'SetSceneLabel',
            'SetSceneEnabled',
            'ReparentLocal',
            'ReplaceTriangles',
            'ReorderMeshVertices',
            'InsertMeshVertex',
            'DeleteMeshVertices',
            'CreateBlendTable',
            'CreateMeshBlend',
            'SetMeshBlendVertices',
            'InsertGluePair',
            'DeleteGluePairs',
            'PatchOffscreenKeyform',
            'VertexWeight',
            'InsertOffscreenKeyform',
            'SetOffscreenStaticKeyform',
            'VertexPatch',
            'Sample',
            'GluePair',
            'OffscreenForm'])}

PUBLIC_EXPORTS['animation'] = ('Independent frozen animation controls and CPU playback results.',
    ['from ._preview import Preview as Preview',
     'from ._generated.modeling import PreviewCommand as Command, PreviewCommandSetParameter as SetParameter, PreviewCommandScheduleMotion as ScheduleMotion, PreviewCommandScheduleRegistration as ScheduleRegistration, PreviewCommandScheduleExpression as ScheduleExpression, PreviewCommandScheduleParameter as ScheduleParameter, PreviewCommandSetPhysics as SetPhysics, PreviewCommandReset as Reset, PreviewState as State, PreviewFrame as Frame, PreviewBatch as Batch'],
    ['Preview','Command','SetParameter','ScheduleMotion','ScheduleRegistration','ScheduleExpression','ScheduleParameter','SetPhysics','Reset','State','Frame','Batch'])

TARGETS=frozenset('client/python/zipzi/'+n+'.py' for n in PUBLIC_EXPORTS) | frozenset({'client/python/zipzi/_generated/facades.py','client/python/zipzi/_generated/discovery.py','src/generated/discovery.rs','client/python/zipzi/_generated/request_directory.py','client/python/zipzi/_generated/backend_bindings.py'})

DOMAINS=(('Meshes','meshes','Mesh','items'),('Parameters','parameters','Parameter','parameters'),('Bindings','bindings','MeshBinding','bindings'),('Images','resources','Image','items'),('Scene','scene','Summary','items'))
WRITES = {'Meshes': [('create',
             'spec: meshes.MeshSpec',
             'str',
             'meshes.Create(spec)',
             '(spec,)'),
            ('rename',
             'id: str, name: str, *, expected_name: str | None = None',
             'WriteResult',
             'meshes.Rename(id,name,expected_name=expected_name)',
             '(id,name,expected_name)'),
            ('set_positions',
             'id: str, *, vertex_ids: Sequence[int], positions: '
             'Sequence[meshes.Point]',
             'WriteResult',
             'meshes.SetPositions(id,vertex_ids,positions)',
             '(id,vertex_ids,positions)')],
 'Parameters': [('create',
                 'spec: parameters.ParameterSpec',
                 'str',
                 'parameters.Create(spec)',
                 '(spec,)'),
                ('patch',
                 'id: str, patch: parameters.ParameterPatch',
                 'WriteResult',
                 'parameters.Patch(id,patch)',
                 '(id,patch)')],
 'Bindings': [('create',
               'spec: bindings.MeshBindingSpec',
               'str',
               'bindings.Create(spec)',
               '(spec,)'),
              ('set_keyform',
               'id: str, keyform: bindings.Keyform',
               'WriteResult',
               'bindings.SetKeyform(id,keyform)',
               '(id,keyform)')]}
# Special methods retain exactly one lifecycle implementation; this registration
# is also used by recover_call and the installed API catalog.
LIFECYCLE_REQUESTS={
 'Runtime':'close events_since capabilities limits', 'Sessions':'create list attach open import_ subscribe',
 'Session':'preview edit plan plan_async info editor_context capabilities address reference show_window subscribe undo redo detach close',
 'Project':'save run diagnose export replace open', 'Edit':'preview apply apply_many plan plan_async commit cancel', 'Job':'wait close state cancel',
 'Modeling':'records sample sample_async',
}
OWNED_REQUESTS={'resources.Resources':'prepare_image prepare_buffer load_png prepare_png usage','resources.PreparedImage':'pixels','Buffer':'read','Owned':'close','animation.Preview':'info apply sample seek advance_to','references.ObjectRef':'resolve label capabilities scene_capabilities','references.LabelAddress':'plan','references.FieldAddress':'plan','events.SessionSubscription':'poll resync','events.RegistrySubscription':'poll resync'}

def render_api(contract,header,rust_header):
    runtime=contract['runtime']
    writes={key:list(value) for key,value in WRITES.items()}
    # Public resource constructors flatten the selected SDK spec and replace
    # only the runtime ImageId leaf with its ownership wrapper.
    roots=contract['resources']['roots']
    union=next(shape for shape in roots if shape.get('name','').endswith('::ResourceOperation'))
    writes['Images']=[]
    for variant in union['variants']:
        if variant['rust_name']=='ApplyPlan':continue
        policy={'CreateImage':('create_image','str'),'ReplaceImage':('replace_image','WriteResult'),'RenameImage':('rename_image','WriteResult')}
        if variant['rust_name'] not in policy:raise ValueError('Unregistered public resource operation')
        projected=[]
        for field in variant['fields']:
            if field['name']=='spec':projected.extend(field['shape']['fields'])
            else:projected.append(field)
        def default(field):
            if field['shape']['kind']=='optional':return 'None'
            value=field.get('default')
            if value is None:return None
            if value=={'kind':'empty_string'}:return '""'
            raise ValueError('Unsupported resource convenience default')
        def annotation(shape):
            if shape['kind']=='string':return 'str'
            if shape['kind']=='optional':return annotation(shape['item'])+' | None'
            if shape['kind']=='opaque' and shape['name'].endswith('::ImageId'):return 'resources.PreparedImage'
            raise ValueError('Unsupported resource convenience field')
        projected=sorted(projected,key=lambda field:default(field) is not None)
        args=[];keyword=False
        for field in projected:
            value=default(field)
            if value is not None and not keyword:args.append('*');keyword=True
            args.append(field['name']+': '+annotation(field['shape'])+(' = '+value if value is not None else ''))
        names=[field['name'] for field in projected]
        expr='resources.'+variant['rust_name']+'('+','.join(name+'='+name for name in names)+')'
        name,result=policy[variant['rust_name']]
        writes['Images'].append((name,', '.join(args),result,expr,'('+','.join(names)+',)'))

    import runpy
    BINDINGS=runpy.run_path(str(Path(__file__).with_name('backend_bindings.py')))['BINDINGS']
    py=[header.rstrip(),'"""Owned Runtime metadata generated from compiled Rust declarations."""','from dataclasses import dataclass','from .._values import record, sequence, integer, string','']
    rs=[rust_header.rstrip(),'use crate::*;']
    def typ(s):
        k=s['kind']
        if k in {'usize','u32','u64'}:return 'int'
        if k in {'bool','string'}:return {'bool':'bool','string':'str'}[k]
        if k=='optional':return typ(s['item'])+' | None'
        if k=='sequence':return 'tuple['+typ(s['item'])+', ...]'
        raise ValueError('Unsupported metadata type '+k)
    def read(s,e):
        k=s['kind']
        if k in {'usize','u32','u64'}:return 'integer('+e+')'
        if k=='string':return 'string('+e+')'
        if k=='bool':return '_flag('+e+')'
        if k=='optional':return 'None if '+e+' is None else '+read(s['item'],e)
        if k=='sequence':return 'tuple('+read(s['item'],'item')+' for item in sequence('+e+'))'
        raise ValueError('Unsupported metadata read')
    py+=['def _flag(value: object) -> bool:','    if type(value) is not bool: raise TypeError("Expected bool")','    return value','']
    for name,s in runtime.items():
        fs=s['fields'];py+=['@dataclass(frozen=True)','class '+name+':','    __module__ = "zipzi"']
        for f in fs:py+=['    '+f['name']+': '+typ(f['shape'])]
        py+=['','    @classmethod','    def read(cls, value: object) -> "'+name+'":','        if isinstance(value,cls): return value','        r=record(value)','        return cls('+', '.join(read(f['shape'],'r['+repr(f['name'])+']') for f in fs)+')','']
        rs+=['pub(crate) fn write_'+name.lower()+"<'py>(py:Python<'py>,v:&"+'zipzi_runtime::'+name+")->PyResult<Bound<'py,PyDict>> {",'let d=PyDict::new(py);']
        for f in fs:rs+=['d.set_item("'+f['name']+'", &v.'+f['name']+')?;']
        rs+=['Ok(d)','}']
    facade=[header.rstrip(),'"""Uniform, typed domain calls. Only registered hooks own execution policy."""','from collections.abc import Sequence, Mapping','from .. import meshes, parameters, bindings, resources, scene, sampling, modeling','from .. import _lifecycle','from .._results import Snapshot, Batch, Page, PageCursor','from .._values import Source, WriteResult','from .._calls import project, single_id','from .._facade_support import Collection, records, sample','']
    for cls,domain,value,key in DOMAINS:
        summary='scene.Summary' if domain=='scene' else domain+'.Summary'
        facade+=['class '+cls+'(Collection['+domain+'.'+value+']):','    def __init__(self, session: "_lifecycle.Session", edit: "_lifecycle.Edit | None" = None):','        super().__init__(session,edit,'+repr(domain)+','+domain+'.'+value+'.read,'+repr(key)+')',
         '    def get(self, id: str, *, expected: Source | None = None) -> Snapshot['+domain+'.'+value+']:',
         '        '+repr('Read one owned value at an exact formal or candidate Source.\n\n'
             'id is the stable document object ID, not a name. expected=None reads current content; '
             'an explicit Source is checked atomically and stale sources raise STALE_SOURCE. '
             'Returns Snapshot[value] with the captured Source; reading its fields performs no IO. '
             'This synchronous call uses one bounded selected read and may reject missing IDs or response budgets. '
             'Query failures do not abort an Edit. After OutcomeUnknown recover the original call to get the '
             'original captured result; do not replace it with a fresh read. The result owns no host lease. '
             'Example: snapshot = session.'+domain+'.get(id); value = snapshot.value.'),
         '        return project(lambda: self._read((id,),expected), self._single, operation='+repr(domain+'.get')+', inputs=(id,expected))',
         '    def get_many(self, ids: Sequence[str], *, expected: Source | None = None) -> Batch['+domain+'.'+value+']:',
         '        '+repr('Read all selected IDs atomically from one formal or candidate Source.\n\n'
             'ids is copied before execution; requested order and duplicates are preserved. '
             'expected=None accepts current content; an explicit Source rejects changes with STALE_SOURCE. '
             'Returns Batch[value], including a valid Source for empty ids. Missing objects or total-byte '
             'budgets reject the whole read without partial values or an aborted Edit. This call is synchronous '
             'and has no host-owned result lease. Recover a lost response through its original Call to retain '
             'the captured source. Example: batch = session.'+domain+'.get_many((id, id)).'),
         '        return self._read(tuple(ids),expected)',
         '    def list(self, *, name: str | None = None, limit: int = 128, cursor: PageCursor | None = None) -> Page['+summary+']:',
         '        '+repr('Read a bounded metadata Page in SDK storage order, without geometry/keyform/pixel payloads.\n\n'
             'name=None disables filtering; a string matches exactly and case-sensitively, preserving duplicate '
             'names. limit defaults to 128 and must fit the host collection-record limit; count/byte/scan '
             'budgets can produce shorter or empty pages. cursor=None starts a scan; otherwise use the '
             'previous Page.next_cursor with the same collection and filter. None as next_cursor ends the scan. '
             'A changed formal/candidate Source or Undo/Redo raises STALE_SOURCE: restart rather than merging '
             'versions. A lost response recovers the original page, even after a peer publication. This is a '
             'synchronous query; read failure does not abort an Edit and the Page needs no close. '
             'Example: page = session.'+domain+'.list(name="Face", limit=128).'),
         '        return self._page(name,limit,cursor,'+summary+'.read)','']
        if cls=='Images':facade+=['    def snapshot(self, id: str) -> resources.ImageLease:',
            '        '+repr('Capture pixels of image id at this exact formal/candidate source into an independent ImageLease. '
                'This synchronous request can reject a missing image or lease/byte budget; queries do not abort an Edit. '
                'The lease pins pixels across replacement and history eviction: close it or use with. Its source is '
                'cached, and pixels() returns owned ImagePixels. Recover an unknown capture through its original Call. '
                'Example: with session.resources.snapshot(id) as lease: pixels = lease.pixels().'),
            '        return self._image_lease(id)','']
        if cls not in writes:continue
        facade+=['class Editable'+cls+'('+cls+'):']
        for name,args,result,expr,inputs in writes[cls]:
            operation=expr.split('(',1)[0]
            returns='the stable object ID as str' if result=='str' else 'WriteResult with the new candidate Source and ordered result IDs'
            detail='Arguments have the semantics of '+operation+'; inspect that operation and its input DTO fields with zipzi.describe. '
            if cls=='Images' and name in {'create_image','replace_image'}:detail+='image must be a live PreparedImage from this Runtime; close that independent lease after acceptance, while native candidate/history owners retain the pixels. '
            facade+=['    def '+name+'(self, '+args+') -> '+result+':',
                '        '+repr('Apply '+operation+' synchronously to this Edit and return '+returns+'.\n\n'+detail+
                    'This changes candidate state only; commit publishes separately. Ordinary write failures '
                    'abort the candidate even when caught, so repair inputs and start a new Edit. Confirmed Busy '
                    'or pre-execution admission refusal preserves it; act on recovery_action. OutcomeUnknown '
                    'retains the original Call and exact return type: recover it instead of replaying the write. '
                    'Example: result = edit.'+domain+'.'+name+'(...), using the arguments shown in its signature.')]
            if result=='str':facade+=['        return project(lambda: self._write(lambda: '+expr+'),single_id,operation='+repr(domain+'.'+name)+',inputs='+inputs+')']
            else:facade+=['        return self._write(lambda: '+expr+')']
        facade+=['']
    facade+=['class Modeling:', '    def __init__(self, session: "_lifecycle.Session", edit: "_lifecycle.Edit | None" = None):','        self._session,self._edit=session,edit',
     '    def records(self, query: modeling.ModelingQuery) -> Snapshot[modeling.OwnedModelingRecords]:',
     '        '+repr('Read selected canonical records from one exact source without editing. query lists stable IDs per domain; empty lists request no payloads, while the ids directory still exposes collection identities. Returns an owned Snapshot; payload slots follow query order and None marks a missing requested ID. This synchronous query may reject count/byte budgets but does not abort an Edit. Recover a lost response through its original Call; results need no close. Example: result = session.modeling.records(modeling.ModelingQuery(meshes=(id,))).'),
     '        return records(self,query)',
     '    def sample(self, poses: Sequence[Mapping[str,float]], *, checks: sampling.Checks = sampling.Checks(), expected: Source | None = None) -> Snapshot[sampling.Result]:',
     '        '+repr('Synchronously evaluate CPU poses without editing or GPU work. poses is an ordered sequence of parameter-ID/value maps in each parameter domain; inputs are owned before execution. checks supplies optional native geometry diagnostics, and expected=None accepts current content while an explicit Source rejects changes atomically. Returns Snapshot[sampling.Result] with frames in pose order. Native count/byte/work budgets apply; query/diagnostic failure does not abort an Edit. A lost result recovers through its original multi-stage Call, never by silently resampling current content. Owned results need no close. Example: sampled = session.modeling.sample(({parameter_id: 0.5},)).'),
     '        return sample(self,poses,checks,expected,False)',
     '    def sample_async(self, poses: Sequence[Mapping[str,float]], *, checks: sampling.Checks = sampling.Checks(), expected: Source | None = None) -> "_lifecycle.Job[Snapshot[sampling.Result]]":',
     '        '+repr('Submit CPU evaluation of poses at the captured exact Source and return Job[Snapshot[sampling.Result]]. poses, checks and expected have the same semantics as sample; this named API always returns a Job rather than switching by input size. Native admission can reject work/count/byte budgets before acceptance. After an unknown submission recover the original Job instead of submitting another. Wait for the actual owned result and close/wait the Job to release its bounded result slot; JobTimeout keeps it pending. Example: with session.modeling.sample_async(({parameter_id: 0.5},)) as job: sampled = job.wait().'),
     '        return sample(self,poses,checks,expected,True)','']
    directory=[header.rstrip(),'from .._calls import register_request','', 'def register() -> None:','    from .. import _lifecycle as lifecycle, resources, references, events, animation','    from ..buffers import Buffer','    from .._owned import Owned']
    registrations={**{'lifecycle.'+k:v for k,v in LIFECYCLE_REQUESTS.items()},**OWNED_REQUESTS}
    for cls,domain,_,_ in DOMAINS:
        registrations['lifecycle.'+cls]='get get_many list'+(' snapshot' if cls=='Images' else '')
        if cls in writes:registrations['lifecycle.Editable'+cls]=' '.join(m[0] for m in writes[cls])
    for cls,methods in registrations.items():
        for method in methods.split():directory+=['    register_request('+cls+('[object]' if cls=='lifecycle.Job' else '')+'.'+method+')']
    hooks={}
    for cls,methods in BINDINGS.items():
        names=[name for name,_,_ in methods]
        if len(names)!=len(set(names)):raise ValueError('Duplicate backend registration')
        tree=ast.parse((Path(__file__).parents[1]/'client/python/zipzi/backends'/ (cls.lower()+'.py')).read_text())
        implementation=next(n for n in tree.body if isinstance(n,ast.ClassDef))
        actual={n.name for n in implementation.body if isinstance(n,ast.FunctionDef)}
        if actual & set(names):raise ValueError('Handwritten method shadows generated backend binding')
        for name,_,hook in methods:
            if hook:
                if hook not in actual:raise ValueError('Missing unique transport hook '+hook)
                hooks[cls+'.'+name]={'hook':hook,'reason':'Own transport/GIL, request identity, binary stages or cleanup; public projection stays in Call','contracts':['C01','C07','C08']}
    backend=[header.rstrip(), 'from __future__ import annotations','from abc import ABC, abstractmethod','from pathlib import Path','from typing import TYPE_CHECKING, Literal','from .. import _operations as ops, sampling','from . import modeling, scene','from .plan_intents import Intent, wire as intent_wire','from .planning import ModelingPlanOperation, encode_modeling','from .._errors import unwrap','from .._values import Source, SessionId, record','from ..events import Cursor','from ..backends._transfers import Transfer, TransferBackend','from ..backends import JobKind','if TYPE_CHECKING:','    from ..backends._native_types import NativeRuntime','']
    for cls,methods in BINDINGS.items():
        backend+=['class '+cls+'Bindings('+('TransferBackend, ' if cls=='Connected' else '')+'ABC):']
        if cls=='Local':backend+=['    _native: NativeRuntime']
        else:backend+=['    @abstractmethod','    def _request(self,value:dict[str,object],owner:object,operation:str)->object: ...','    @abstractmethod','    def _drive_transfer(self,transfer:Transfer)->object: ...']
        for name,source,hook in methods:
            if hook:
                signature=source.split('\n',1)[0].replace('def '+name+'(','def '+hook+'(')
                backend+=['    @abstractmethod','    '+signature+' ...']
            backend+=['    '+line if line else '' for line in source.splitlines()]
        backend+=['']
    backend += ['TRANSPORT_HOOKS = '+repr(hooks), '']
    reexports={'client/python/zipzi/'+name+'.py':header+repr(doc)+'\n'+'\n'.join(imports)+'\n__all__ = '+repr(names)+'\n' for name,(doc,imports,names) in PUBLIC_EXPORTS.items()}
    return {**reexports, 'client/python/zipzi/_generated/backend_bindings.py':'\n'.join(backend),'client/python/zipzi/_generated/facades.py':'\n'.join(facade), 'client/python/zipzi/_generated/discovery.py':'\n'.join(py), 'src/generated/discovery.rs':'\n'.join(rs), 'client/python/zipzi/_generated/request_directory.py':'\n'.join(directory)+'\n'}
