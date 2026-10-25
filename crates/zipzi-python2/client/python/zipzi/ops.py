"""Typed operations; resource/Plan ownership remains handwritten."""
from dataclasses import dataclass, field
from typing import Literal
from . import bindings, parameters
from ._generated.mesh_ops import Create as Create, Rename as Rename, SetPositions as SetPositions, validate as validate_mesh, wire as mesh_wire

from ._generated.resource_ops import CreateImage as CreateImage, ReplaceImage as ReplaceImage, ApplyPlan as ApplyPlan, validate as validate_resource, wire as resource_wire

@dataclass(frozen=True)
class CreateParameter:
    spec: parameters.ParameterSpec
    kind: Literal["create_parameter"] = field(default="create_parameter", init=False)

@dataclass(frozen=True)
class PatchParameter:
    id: str
    patch: parameters.Patch
    kind: Literal["patch_parameter"] = field(default="patch_parameter", init=False)

@dataclass(frozen=True)
class CreateMeshBinding:
    spec: bindings.MeshBindingSpec
    kind: Literal["create_mesh_binding"] = field(default="create_mesh_binding", init=False)

@dataclass(frozen=True)
class SetMeshKeyform:
    id: str
    keyform: bindings.Keyform
    kind: Literal["set_mesh_keyform"] = field(default="set_mesh_keyform", init=False)

Operation = Create | Rename | SetPositions | CreateImage | ReplaceImage | ApplyPlan | CreateParameter | PatchParameter | CreateMeshBinding | SetMeshKeyform

def validate(op: Operation) -> None:
    match op:
        case CreateMeshBinding(spec):
            bindings.validate(spec)
        case SetMeshKeyform(id, keyform):
            if not isinstance(id, str):
                raise TypeError("Expected binding ID text")
            bindings.validate_keyform(keyform)
        case CreateParameter(spec):
            if not isinstance(spec, parameters.ParameterSpec):
                raise TypeError("Expected a ParameterSpec")
            parameters.validate(spec)
        case PatchParameter(id, patch):
            if not isinstance(id, str) or not isinstance(patch, parameters.Patch):
                raise TypeError("Expected parameter ID and Patch")
            parameters.validate(patch)
        case CreateImage() | ReplaceImage() | ApplyPlan():
            validate_resource(op)
        case _:
            validate_mesh(op)

def wire(op: Operation) -> dict[str, object]:
    match op:
        case CreateMeshBinding(spec):
            return dict(domain="mesh_binding", operation=dict(op="create", spec=bindings.wire(spec)))
        case SetMeshKeyform(id, keyform):
            return dict(domain="mesh_binding", operation=dict(op="set_keyform", id=id, keyform=bindings.keyform_wire(keyform)))
        case CreateParameter(spec):
            return dict(domain="parameter", operation=dict(op="create", spec=parameters.wire(spec)))
        case PatchParameter(id, patch):
            return dict(domain="parameter", operation=dict(op="patch", id=id, patch=parameters.wire(patch)))
        case CreateImage() | ReplaceImage() | ApplyPlan():
            return resource_wire(op)
        case _:
            return dict(domain="mesh", operation=mesh_wire(op))
