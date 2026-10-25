"""Typed scene/organization intentions and immutable metadata projections."""
from dataclasses import dataclass
from ._values import Source, record, sequence
from ._generated.scene import (
    PartSpec as PartSpec, ParameterGroupSpec as ParameterGroupSpec, Placement as Placement,
    Summary as Summary, Operation as Operation, OPERATION_TYPES as OPERATION_TYPES,
    CreatePart as CreatePart, CreateParameterGroup as CreateParameterGroup,
    CreateWarp as CreateWarp, CreateRotation as CreateRotation, CreateGlue as CreateGlue,
    CreateOffscreen as CreateOffscreen, MoveToPart as MoveToPart,
    MoveDeformation as MoveDeformation, MoveParameters as MoveParameters,
    MoveParameterGroups as MoveParameterGroups, DuplicateScene as DuplicateScene,
    DeleteScene as DeleteScene, DeleteOrganization as DeleteOrganization,
    SetSceneLabel as SetSceneLabel, SetSceneEnabled as SetSceneEnabled,
    ReparentLocal as ReparentLocal, ReplaceTriangles as ReplaceTriangles,
    ReorderMeshVertices as ReorderMeshVertices, InsertMeshVertex as InsertMeshVertex,
    DeleteMeshVertices as DeleteMeshVertices, CreateBlendTable as CreateBlendTable,
    CreateMeshBlend as CreateMeshBlend, SetMeshBlendVertices as SetMeshBlendVertices,
    InsertGluePair as InsertGluePair, DeleteGluePairs as DeleteGluePairs,
    PatchOffscreenKeyform as PatchOffscreenKeyform, VertexWeight as VertexWeight,
    InsertOffscreenKeyform as InsertOffscreenKeyform,
    SetOffscreenStaticKeyform as SetOffscreenStaticKeyform,
    VertexPatch as VertexPatch, Sample as Sample, GluePair as GluePair, OffscreenForm as OffscreenForm,
    validate as validate, wire as wire,
)

@dataclass(frozen=True)
class Batch:
    source: Source
    items: tuple[Summary, ...]

    @classmethod
    def read(cls, raw: object) -> "Batch":
        value = record(raw)
        return cls(Source.read(value["source"]), tuple(Summary.read(item) for item in sequence(value["items"])))

@dataclass(frozen=True)
class Snapshot:
    source: Source
    value: Summary
