"""Finite Python binding declarations; lifecycle/transport hooks remain handwritten."""
import ast
import json
from pathlib import Path

PUBLIC_EXPORTS = {'meshes': ('Mesh values are generated; selected Plan intents are generated.',
            ['from ._generated.meshes import Geometry as Geometry, MeshSpec as '
             'MeshSpec, Point as Point, Mesh as Mesh',
             'from ._generated.mesh_ops import Create as Create, Rename as '
             'Rename, SetPositions as SetPositions',
             'from ._generated.discovery import ObjectSummary as Summary'],
            ['Geometry',
             'MeshSpec',
             'Point',
             'Mesh',
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
 'Session':'edit plan plan_async info editor_context capabilities address reference show_window subscribe undo redo detach close',
 'Project':'save run diagnose export replace open', 'Edit':'apply apply_many plan plan_async commit cancel', 'Job':'wait close state cancel',
 'Modeling':'records sample sample_async',
}
OWNED_REQUESTS={'resources.Resources':'prepare_image prepare_buffer load_png prepare_png usage','resources.PreparedImage':'pixels','Buffer':'read','Owned':'close','references.ObjectRef':'resolve label capabilities scene_capabilities','references.LabelAddress':'plan','references.FieldAddress':'plan','events.SessionSubscription':'poll resync','events.RegistrySubscription':'poll resync'}

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
         '        """Read one owned value at an exact source. Missing IDs fail the whole read; recover the original call after transport loss."""',
         '        return project(lambda: self._read((id,),expected), self._single, operation='+repr(domain+'.get')+', inputs=(id,expected))',
         '    def get_many(self, ids: Sequence[str], *, expected: Source | None = None) -> Batch['+domain+'.'+value+']:',
         '        """Read all IDs in request order, including duplicates. Empty batches keep Source; missing IDs or budgets reject the entire read."""',
         '        return self._read(tuple(ids),expected)',
         '    def list(self, *, name: str | None = None, limit: int = 128, cursor: PageCursor | None = None) -> Page['+summary+']:',
         '        """Read bounded metadata in SDK storage order. Names match exactly and preserve duplicates. Source changes raise STALE_SOURCE; restart explicitly."""',
         '        return self._page(name,limit,cursor,'+summary+'.read)','']
        if cls=='Images':facade+=['    def snapshot(self, id: str) -> resources.ImageLease:','        """Pin immutable pixels with an explicit lease; close the lease when finished."""','        return self._image_lease(id)','']
        if cls not in writes:continue
        facade+=['class Editable'+cls+'('+cls+'):']
        for name,args,result,expr,inputs in writes[cls]:
            facade+=['    def '+name+'(self, '+args+') -> '+result+':','        """Apply a typed '+domain+' operation to this candidate. Commit publishes separately; ordinary write failures require a new Edit."""']
            if result=='str':facade+=['        return project(lambda: self._write(lambda: '+expr+'),single_id,operation='+repr(domain+'.'+name)+',inputs='+inputs+')']
            else:facade+=['        return self._write(lambda: '+expr+')']
        facade+=['']
    facade+=['class Modeling:', '    def __init__(self, session: "_lifecycle.Session", edit: "_lifecycle.Edit | None" = None):','        self._session,self._edit=session,edit',
     '    def records(self, query: modeling.ModelingQuery) -> Snapshot[modeling.OwnedModelingRecords]:','        """Read selected canonical records from one exact source; the complete result remains owned after later writes."""','        return records(self,query)',
     '    def sample(self, poses: Sequence[Mapping[str,float]], *, checks: sampling.Checks = sampling.Checks(), expected: Source | None = None) -> Snapshot[sampling.Result]:',
     '        """Evaluate CPU poses without editing. Values are copied before execution; expected rejects a changed source."""','        return sample(self,poses,checks,expected,False)',
     '    def sample_async(self, poses: Sequence[Mapping[str,float]], *, checks: sampling.Checks = sampling.Checks(), expected: Source | None = None) -> "_lifecycle.Job[Snapshot[sampling.Result]]":',
     '        """Submit CPU evaluation at the captured source; wait and close the accepted Job to release its result slot."""','        return sample(self,poses,checks,expected,True)','']
    directory=[header.rstrip(),'from .._calls import register_request','', 'def register() -> None:','    from .. import _lifecycle as lifecycle, resources, references, events','    from ..buffers import Buffer','    from .._owned import Owned']
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
