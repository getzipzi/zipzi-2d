"""Internal typed wire projection; public constructors live in domain modules."""
from ._generated import scene as _scene_codec
from ._generated.codecs import commands as _modeling_codec
from ._generated.values.commands import OPERATION_TYPES as _MODELING_OPERATION_TYPES
from typing import TypeAlias
from . import meshes, parameters, bindings, resources
from ._generated.mesh_ops import validate as validate_mesh, wire as mesh_wire
from ._generated.resource_ops import ApplyPlan as ApplyPlan, validate as validate_resource, wire as resource_wire
from ._generated.parameter_values import validate_Operation as validate_parameter, encode_Operation as parameter_wire
from ._generated.binding_values import validate_Operation as validate_binding, encode_Operation as binding_wire
from .ops import Operation as Operation

Create = meshes.Create
Rename = meshes.Rename
SetPositions = meshes.SetPositions
CreateParameter = parameters.Create
PatchParameter = parameters.Patch
CreateMeshBinding = bindings.Create
SetMeshKeyform = bindings.SetKeyform
CreateImage = resources.CreateImage
ReplaceImage = resources.ReplaceImage
RenameImage = resources.RenameImage
WireOperation: TypeAlias = Operation | ApplyPlan
OPERATION_TYPES = (Create, Rename, SetPositions, CreateParameter, PatchParameter,
    CreateMeshBinding, SetMeshKeyform, CreateImage, ReplaceImage, RenameImage,
    *_scene_codec.OPERATION_TYPES, *_MODELING_OPERATION_TYPES)

def validate(op: WireOperation) -> None:
    # Ordinary mesh batches should not scan every canonical modeling/scene type.
    if isinstance(op, (Create, Rename, SetPositions)): validate_mesh(op)
    elif isinstance(op, _MODELING_OPERATION_TYPES): _modeling_codec.validate_ModelingCommand(op)
    elif isinstance(op, _scene_codec.OPERATION_TYPES): _scene_codec.validate(op)
    elif isinstance(op, (CreateParameter, PatchParameter)): validate_parameter(op)
    elif isinstance(op, (CreateMeshBinding, SetMeshKeyform)): validate_binding(op)
    elif isinstance(op, (CreateImage, ReplaceImage, RenameImage, ApplyPlan)): validate_resource(op)
    else: raise TypeError('Expected domain operation')

def wire(op: WireOperation) -> dict[str, object]:
    if isinstance(op, (Create, Rename, SetPositions)): return dict(domain='mesh', operation=mesh_wire(op))
    if isinstance(op, _MODELING_OPERATION_TYPES): return dict(domain='modeling', operation=_modeling_codec.encode_ModelingCommand(op))
    if isinstance(op, _scene_codec.OPERATION_TYPES): return _scene_codec.wire(op)
    if isinstance(op, (CreateParameter, PatchParameter)): return dict(domain='parameter', operation=parameter_wire(op))
    if isinstance(op, (CreateMeshBinding, SetMeshKeyform)): return dict(domain='mesh_binding', operation=binding_wire(op))
    if isinstance(op, (CreateImage, ReplaceImage, RenameImage, ApplyPlan)): return resource_wire(op)
    raise TypeError('Expected domain operation')
