"""Selected mesh values. Coordinates/UVs use f32; vertex IDs use u32 in Rust.

Arrays have owned tuple storage in snapshots. Rust validates finite values,
geometry and domain references. Reading a snapshot field performs no requests.
"""
from dataclasses import dataclass, field
from typing import Literal
from ._values import Source, number, integer, record, sequence, string

Point = tuple[float, float]

@dataclass(frozen=True)
class Geometry:
    vertex_ids: tuple[int, ...]
    positions: tuple[Point, ...]
    uvs: tuple[Point, ...]
    triangles: tuple[tuple[int, int, int], ...]

    def __post_init__(self) -> None:
        # Shape/ownership checks only. Invalid geometry still reaches Rust.
        for values in (self.vertex_ids, self.positions, self.uvs, self.triangles):
            if not isinstance(values, tuple):
                raise TypeError("Geometry arrays must be tuples")
        if any(not isinstance(p, tuple) or len(p) != 2 for p in (*self.positions, *self.uvs)):
            raise TypeError("Points must be pairs")
        if any(not isinstance(t, tuple) or len(t) != 3 for t in self.triangles):
            raise TypeError("Triangles must contain three vertex IDs")

    @classmethod
    def read(cls, value: object) -> "Geometry":
        r = record(value)
        def points(value: object) -> tuple[Point, ...]:
            return tuple((number(record(p)["x"]), number(record(p)["y"])) for p in sequence(value))
        triangles: list[tuple[int, int, int]] = []
        for raw in sequence(r["triangles"]):
            t = sequence(raw)
            if len(t) != 3:
                raise ValueError("Invalid triangle")
            triangles.append((integer(t[0]), integer(t[1]), integer(t[2])))
        return cls(tuple(integer(v) for v in sequence(r["vertex_ids"])), points(r["positions"]),
            points(r["uvs"]), tuple(triangles))

@dataclass(frozen=True)
class MeshSpec:
    name: str
    geometry: Geometry
    texture_asset_id: str = ""
    id: str | None = None

@dataclass(frozen=True)
class Snapshot:
    source: Source
    id: str
    name: str
    geometry: Geometry

    @classmethod
    def read(cls, value: object) -> "Snapshot":
        r = record(value)
        return cls(Source.read(r["source"]), string(r["id"]), string(r["name"]), Geometry.read(r["geometry"]))

@dataclass(frozen=True)
class RenameIntent:
    id: str
    expected_name: str
    name: str
    kind: Literal["mesh_rename"] = field(default="mesh_rename", init=False)
