"""Translate the public plannable operation subset into existing Runtime calls."""
from __future__ import annotations
from ._generated import scene as _scene_codec
from ._generated.codecs import commands as _modeling_codec
from typing import TYPE_CHECKING
from . import meshes, resources, ops
from ._generated import plan_intents, planning
from ._generated.mesh_ops import validate as validate_mesh
from ._generated.resource_ops import validate as validate_resource
if TYPE_CHECKING:
    from .backends import Backend


def request(backend: Backend, session: object, edit: object | None,
            operation: ops.PlannableOperation, owner: object) -> object:
    if isinstance(operation, meshes.Rename):
        validate_mesh(operation)
        return backend.plan(session, edit, plan_intents.MeshRename(
            id=operation.id, name=operation.name, expected_name=operation.expected_name), owner)
    if isinstance(operation, resources.RenameImage):
        validate_resource(operation)
        return backend.plan(session, edit, plan_intents.ImageRename(
            id=operation.id, name=operation.name, expected_name=operation.expected_name), owner)
    if isinstance(operation, _scene_codec.OPERATION_TYPES):
        _scene_codec.validate(operation)
        return backend.scene_plan(session, edit, operation, owner)
    if isinstance(operation, planning.MODELING_PLAN_TYPES):
        _modeling_codec.validate_ModelingCommand(operation)
        return backend.modeling_plan(session, edit, operation, owner)
    raise TypeError('This operation is not registered for planning')
