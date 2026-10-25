"""Explicit CPU poses and complete immutable frame results; no Preview owner."""
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from enum import Enum
import math
from . import bindings
from ._generated import modeling as m
from ._values import Source, Version, integer, number, record, sequence, string
from .meshes import Point

Inputs = tuple[dict[str, float], ...]

@dataclass(frozen=True)
class Checks:
    min_triangle_area: float = 0.
    model_bounds: tuple[Point, Point] | None = None

    def __post_init__(self) -> None:
        if self.model_bounds is not None:
            object.__setattr__(self, "model_bounds", tuple(tuple(p) for p in self.model_bounds))

    def wire(self) -> dict[str, object]:
        if isinstance(self.min_triangle_area, bool) or not isinstance(self.min_triangle_area, (int, float)) or not math.isfinite(self.min_triangle_area):
            raise ValueError("Triangle area must encode as finite f64")
        bounds = None
        if self.model_bounds is not None:
            if len(self.model_bounds) != 2 or any(len(p) != 2 for p in self.model_bounds):
                raise ValueError("Expected min/max point pairs")
            for p in self.model_bounds:
                for value in p:
                    bindings.scalar(value)
            bounds = dict(min=dict(x=self.model_bounds[0][0], y=self.model_bounds[0][1]),
                max=dict(x=self.model_bounds[1][0], y=self.model_bounds[1][1]))
        return dict(min_triangle_area=self.min_triangle_area, model_bounds=bounds)

def inputs(poses: Sequence[Mapping[str, float]]) -> Inputs:
    result: list[dict[str, float]] = []
    for pose in poses:
        owned = dict(pose)
        for id, value in owned.items():
            if not isinstance(id, str):
                raise TypeError("Expected parameter ID text")
            bindings.scalar(value)
        result.append(owned)
    return tuple(result)

def flag(value: object) -> bool:
    if type(value) is not bool:
        raise TypeError("Expected a flag")
    return value

def rgba(value: object) -> tuple[float, float, float, float]:
    values = sequence(value)
    if len(values) != 4:
        raise ValueError("Expected four color channels")
    return number(values[0]), number(values[1]), number(values[2]), number(values[3])

class BlendMode(str, Enum):
    NORMAL = "normal"
    ADDITIVE = "additive"
    MULTIPLICATIVE = "multiplicative"

class DiagnosticKind(str, Enum):
    SMALL_TRIANGLE = "SmallTriangle"
    INCONSISTENT_WINDING = "InconsistentWinding"
    OUTSIDE_MODEL_BOUNDS = "OutsideModelBounds"

class RenderKind(str, Enum):
    BEGIN_OFFSCREEN = "BeginOffscreen"
    DRAW_MESH = "DrawMesh"
    END_OFFSCREEN = "EndOffscreen"

@dataclass(frozen=True)
class Diagnostic:
    kind: DiagnosticKind
    mesh_id: str
    triangle_index: int | None

@dataclass(frozen=True)
class Parameter:
    id: str
    requested: float
    value: float
    clamped: bool

@dataclass(frozen=True)
class Pose:
    values: tuple[tuple[str, float], ...]

@dataclass(frozen=True)
class Drawable:
    id: str
    runtime_id: str
    part_id: str
    raw_blend_mode: int | None
    texture_asset_id: str
    texture_slot: int
    renderable: bool
    positions: tuple[Point, ...]
    uvs: tuple[Point, ...]
    indices: tuple[int, ...]
    draw_order: int
    render_order: int
    opacity: float
    multiply_color: tuple[float, float, float, float]
    screen_color: tuple[float, float, float, float]
    blend_mode: BlendMode
    enabled: bool
    visible: bool
    double_sided: bool
    inverted_mask: bool
    masks: tuple[str, ...]

    @classmethod
    def read(cls, raw: object) -> "Drawable":
        v = record(raw)
        return cls(string(v["id"]), string(v["runtime_id"]), string(v["part_id"]),
            None if v["raw_blend_mode"] is None else integer(v["raw_blend_mode"]), string(v["texture_asset_id"]),
            integer(v["texture_slot"]), flag(v["renderable"]), bindings.points(v["positions"]), bindings.points(v["uvs"]),
            tuple(integer(i) for i in sequence(v["indices"])), integer(v["draw_order"]), integer(v["render_order"]),
            number(v["opacity"]), rgba(v["multiply_color"]), rgba(v["screen_color"]), BlendMode(string(v["blend_mode"])),
            flag(v["enabled"]), flag(v["visible"]), flag(v["double_sided"]), flag(v["inverted_mask"]),
            tuple(string(s) for s in sequence(v["masks"])))

@dataclass(frozen=True)
class Deformer:
    id: str
    enabled: bool
    points: tuple[Point, ...]
    rotation_axis: Point | None

@dataclass(frozen=True)
class Offscreen:
    renderable: bool
    id: str
    runtime_id: str
    owner_part_id: str
    parent_offscreen_id: str | None
    render_order: int
    opacity: float
    enabled: bool
    blend_mode: int
    flags: int
    masks: tuple[str, ...]
    multiply_color: tuple[float, float, float, float]
    screen_color: tuple[float, float, float, float]

@dataclass(frozen=True)
class RenderCommand:
    kind: RenderKind
    id: str

@dataclass(frozen=True)
class Frame:
    source_revision: int
    parameters: tuple[Parameter, ...]
    drawables: tuple[Drawable, ...]
    offscreens: tuple[Offscreen, ...]
    render_plan: tuple[RenderCommand, ...]
    deformers: tuple[Deformer, ...]

    @classmethod
    def read(cls, raw: object) -> "Frame":
        v = record(raw)
        params = tuple(Parameter(string(record(p)["id"]), number(record(p)["requested"]),
            number(record(p)["value"]), flag(record(p)["clamped"])) for p in sequence(v["parameters"]))
        deformers: list[Deformer] = []
        for item in sequence(v["deformers"]):
            d = record(item)
            axis = None if d.get("rotation_axis") is None else bindings.points((d["rotation_axis"],))[0]
            deformers.append(Deformer(string(d["id"]), flag(d["enabled"]), bindings.points(d["points"]), axis))
        offscreens: list[Offscreen] = []
        for item in sequence(v["offscreens"]):
            d = record(item)
            offscreens.append(Offscreen(flag(d["renderable"]), string(d["id"]), string(d["runtime_id"]), string(d["owner_part_id"]),
                None if d["parent_offscreen_id"] is None else string(d["parent_offscreen_id"]), integer(d["render_order"]),
                number(d["opacity"]), flag(d["enabled"]), integer(d["blend_mode"]), integer(d["flags"]),
                tuple(string(s) for s in sequence(d["masks"])), rgba(d["multiply_color"]), rgba(d["screen_color"])))
        commands: list[RenderCommand] = []
        for item in sequence(v["render_plan"]):
            command = record(item)
            if len(command) != 1:
                raise ValueError("Expected a render command")
            kind = RenderKind(next(iter(command)))
            commands.append(RenderCommand(kind, string(record(command[kind.value])["mesh_id" if kind == RenderKind.DRAW_MESH else "offscreen_id"])))
        return cls(integer(v["source_revision"]), params, tuple(Drawable.read(d) for d in sequence(v["drawables"])),
            tuple(offscreens), tuple(commands), tuple(deformers))

@dataclass(frozen=True)
class Batch:
    source: Source
    state: str
    version: Version
    parameters: tuple[Pose, ...]
    frames: tuple[Frame, ...]
    diagnostics: tuple[Diagnostic, ...]

    @classmethod
    def read(cls, raw: object) -> "Batch":
        v = record(raw)
        raw_samples=v["samples"]
        if isinstance(raw_samples,m.ModelingSamples):
            source=Source.read(v["source"])
            frames=tuple(Frame(f.source_revision,
                tuple(Parameter(p.id,p.requested,p.value,p.clamped) for p in f.parameters),
                tuple(Drawable(d.id,d.runtime_id,d.part_id,d.raw_blend_mode,d.texture_asset_id,d.texture_slot,d.renderable,d.positions,d.uvs,d.indices,d.draw_order,d.render_order,d.opacity,d.multiply_color,d.screen_color,BlendMode(d.blend_mode.value),d.enabled,d.visible,d.double_sided,d.inverted_mask,d.masks) for d in f.drawables),
                tuple(Offscreen(d.renderable,d.id,d.runtime_id,d.owner_part_id,d.parent_offscreen_id,d.render_order,d.opacity,d.enabled,d.blend_mode,d.flags,d.masks,d.multiply_color,d.screen_color) for d in f.offscreens),
                tuple(RenderCommand(RenderKind(c._variant), c.mesh_id if isinstance(c,m.RenderCommandDrawMesh) else c.offscreen_id) for c in f.render_plan),
                tuple(Deformer(d.id,d.enabled,d.points,d.rotation_axis) for d in f.deformers)) for f in raw_samples.frames)
            poses=tuple(Pose(tuple(sorted(p.items()))) for p in raw_samples.parameters)
            if source.version!=raw_samples.version or len(poses)!=len(frames): raise ValueError("Inconsistent sampled source or batch")
            return cls(source,string(v["state"]),raw_samples.version,poses,frames,tuple(Diagnostic(DiagnosticKind(d.kind.value),d.mesh_id,d.triangle_index) for d in raw_samples.diagnostics))
        s = record(raw_samples)
        diagnostics: list[Diagnostic] = []
        for item in sequence(s["diagnostics"]):
            d = record(item)
            diagnostics.append(Diagnostic(DiagnosticKind(string(d["kind"])), string(d["mesh_id"]),
                None if d["triangle_index"] is None else integer(d["triangle_index"])))
        source, version = Source.read(v["source"]), Version.read(s["version"])
        poses = tuple(Pose(tuple(sorted((k, number(value)) for k, value in record(p).items()))) for p in sequence(s["parameters"]))
        frames = tuple(Frame.read(f) for f in sequence(s["frames"]))
        if version != source.version or len(poses) != len(frames):
            raise ValueError("Inconsistent sampled source or batch")
        return cls(source, string(v["state"]), version, poses, frames, tuple(diagnostics))
