"""Mesh values are generated; selected Plan intents remain handwritten."""
from dataclasses import dataclass, field
from typing import Literal
from ._generated.meshes import Geometry as Geometry, MeshSpec as MeshSpec, Point as Point, Snapshot as Snapshot

@dataclass(frozen=True)
class RenameIntent:
    id: str
    expected_name: str
    name: str
    kind: Literal["mesh_rename"] = field(default="mesh_rename", init=False)
