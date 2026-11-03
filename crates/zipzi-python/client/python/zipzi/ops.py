"""Static operation unions; construct operations in their owning domain."""
from typing import TypeAlias as _TypeAlias
from . import meshes as _meshes, parameters as _parameters, bindings as _bindings, resources as _resources
from ._generated import scene as _scene, values as _modeling
from ._generated.planning import ModelingPlanOperation as _ModelingPlanOperation

Operation: _TypeAlias = (_meshes.Create | _meshes.Rename | _meshes.SetPositions |
    _parameters.Create | _parameters.Patch | _bindings.Create | _bindings.SetKeyform |
    _resources.CreateImage | _resources.ReplaceImage | _resources.RenameImage |
    _scene.Operation | _modeling.ModelingCommand)
"""Candidate write input: select a domain operation or a canonical modeling command; execution and publication are separate."""
PlannableOperation: _TypeAlias = _meshes.Rename | _resources.RenameImage | _scene.Operation | _ModelingPlanOperation
"""Operations accepted for planning; inspect the source-bound plan before applying it."""

__all__ = ['Operation', 'PlannableOperation']
