"""P1 operation values; convenience methods construct these same operations."""
from dataclasses import dataclass, field
from typing import Literal
import math
from .meshes import MeshSpec, Point
from .plans import Plan
from .resources import PreparedImage

@dataclass(frozen=True)
class Create:
    spec: MeshSpec
    kind: Literal["create"] = field(default="create", init=False)

@dataclass(frozen=True)
class Rename:
    id: str
    name: str
    kind: Literal["rename"] = field(default="rename", init=False)

@dataclass(frozen=True)
class SetPositions:
    id: str
    vertex_ids: tuple[int, ...]
    positions: tuple[Point, ...]
    kind: Literal["set_positions"] = field(default="set_positions", init=False)

@dataclass(frozen=True)
class CreateImage:
    name: str
    image: PreparedImage
    id: str | None = None
    kind: Literal["create_image"] = field(default="create_image", init=False)

@dataclass(frozen=True)
class ReplaceImage:
    id: str
    image: PreparedImage
    kind: Literal["replace_image"] = field(default="replace_image", init=False)

@dataclass(frozen=True)
class ApplyPlan:
    plan: Plan
    kind: Literal["apply_plan"] = field(default="apply_plan", init=False)

Operation = Create | Rename | SetPositions | CreateImage | ReplaceImage | ApplyPlan

def validate(op: Operation) -> None:
    """Common expression/encoding checks before either backend sees a write."""
    def text(value: object) -> None:
        if not isinstance(value, str):
            raise TypeError("Expected text")
    def ids(values: tuple[int, ...]) -> None:
        if not isinstance(values, tuple):
            raise TypeError("Vertex IDs must be an owned tuple")
        if any(type(v) is not int or not 0 <= v <= 0xffffffff for v in values):
            raise ValueError("Vertex IDs must be u32 integers")
    def points(values: tuple[Point, ...]) -> None:
        if not isinstance(values, tuple):
            raise TypeError("Positions must be an owned tuple")
        for p in values:
            if not isinstance(p, tuple) or len(p) != 2:
                raise TypeError("Points must be pairs")
            for n in p:
                if not isinstance(n, (float, int)) or isinstance(n, bool):
                    raise TypeError("Coordinates must be real numbers")
                if not math.isfinite(n) or abs(n) > 3.4028234663852886e38:
                    raise ValueError("Coordinates must be finite and representable as f32")
    match op:
        case CreateImage(name, image, id):
            text(name)
            if id is not None:
                text(id)
            if not isinstance(image, PreparedImage):
                raise TypeError("Expected a PreparedImage")
        case ReplaceImage(id, image):
            text(id)
            if not isinstance(image, PreparedImage):
                raise TypeError("Expected a PreparedImage")
        case ApplyPlan(plan):
            if not isinstance(plan, Plan):
                raise TypeError("Expected a Plan")
        case Create(spec):
            text(spec.name)
            text(spec.texture_asset_id)
            if spec.id is not None:
                text(spec.id)
            g = spec.geometry
            ids(g.vertex_ids)
            points(g.positions)
            points(g.uvs)
            for triangle in g.triangles:
                ids(triangle)
        case Rename(id, name):
            text(id)
            text(name)
        case SetPositions(id, vertex_ids, positions):
            text(id)
            ids(vertex_ids)
            points(positions)

def mesh_wire(op: Create | Rename | SetPositions) -> dict[str, object]:
    def points(values: tuple[Point, ...]) -> list[dict[str, float]]:
        return [dict(x=x, y=y) for x, y in values]
    match op:
        case Create(spec):
            g = spec.geometry
            return dict(op="create", spec=dict(id=spec.id, name=spec.name, texture_asset_id=spec.texture_asset_id,
                geometry=dict(vertex_ids=g.vertex_ids, positions=points(g.positions), uvs=points(g.uvs), triangles=g.triangles)))
        case Rename(id, name):
            return dict(op="rename", id=id, name=name)
        case SetPositions(id, vertex_ids, positions):
            return dict(op="set_positions", id=id, vertex_ids=vertex_ids, positions=points(positions))
    raise TypeError("Unknown mesh operation")

def wire(op: Operation) -> dict[str, object]:
    match op:
        case CreateImage(name, image, id):
            return dict(domain="create_image", spec=dict(id=id, name=name), image=image._token)
        case ReplaceImage(id, image):
            return dict(domain="replace_image", id=id, image=image._token)
        case ApplyPlan(plan):
            return dict(domain="apply_plan", plan=plan._token)
        case _:
            return dict(domain="mesh", operation=mesh_wire(op))
