"""Numeric mesh binding inputs and immutable canonical binding snapshots."""
from dataclasses import dataclass
import math
from ._values import Source, number, record, sequence, string
from .meshes import Point
from ._generated import modeling as m

def scalar(value: float) -> None:
    if isinstance(value, bool) or not isinstance(value, (float, int)):
        raise TypeError("Expected a number")
    if not math.isfinite(value) or abs(value) > 3.4028234663852886e38:
        raise ValueError("Expected finite f32")

def points(value: object) -> tuple[Point, ...]:
    return tuple((number(record(p)["x"]), number(record(p)["y"])) for p in sequence(value))

def color(value: object) -> tuple[float, float, float]:
    items = sequence(value)
    if len(items) != 3:
        raise ValueError("Expected three color channels")
    return number(items[0]), number(items[1]), number(items[2])

@dataclass(frozen=True)
class Axis:
    parameter_id: str
    keys: tuple[float, ...]

    def __post_init__(self) -> None:
        object.__setattr__(self, "keys", tuple(self.keys))

@dataclass(frozen=True)
class Keyform:
    keys: tuple[float, ...]
    positions: tuple[Point, ...]
    opacity: float | None = None
    multiply: tuple[float, float, float] | None = None
    screen: tuple[float, float, float] | None = None
    draw_order: float | None = None

    def __post_init__(self) -> None:
        object.__setattr__(self, "keys", tuple(self.keys))
        object.__setattr__(self, "positions", tuple(tuple(p) for p in self.positions))
        for name in ("multiply", "screen"):
            value = getattr(self, name)
            if value is not None:
                object.__setattr__(self, name, tuple(value))

@dataclass(frozen=True)
class MeshBindingSpec:
    mesh_id: str
    axes: tuple[Axis, ...]
    keyforms: tuple[Keyform, ...]
    id: str | None = None

    def __post_init__(self) -> None:
        object.__setattr__(self, "axes", tuple(self.axes))
        object.__setattr__(self, "keyforms", tuple(self.keyforms))

def validate_keyform(value: Keyform) -> None:
    if not isinstance(value, Keyform):
        raise TypeError("Expected a Keyform")
    for key in value.keys:
        scalar(key)
    for p in value.positions:
        if len(p) != 2:
            raise ValueError("Expected point pairs")
        for component in p:
            scalar(component)
    for item in (value.opacity, value.draw_order):
        if item is not None:
            scalar(item)
    for channels in (value.multiply, value.screen):
        if channels is not None:
            if len(channels) != 3:
                raise ValueError("Expected three color channels")
            for item in channels:
                scalar(item)

def validate(value: MeshBindingSpec) -> None:
    if not isinstance(value, MeshBindingSpec) or not isinstance(value.mesh_id, str) or (value.id is not None and not isinstance(value.id, str)):
        raise TypeError("Expected MeshBindingSpec and text identities")
    for axis in value.axes:
        if not isinstance(axis, Axis) or not isinstance(axis.parameter_id, str):
            raise TypeError("Expected an Axis and parameter ID text")
        for key in axis.keys:
            scalar(key)
    for keyform in value.keyforms:
        validate_keyform(keyform)

def keyform_wire(value: Keyform) -> dict[str, object]:
    return dict(keys=value.keys, positions=[dict(x=x, y=y) for x, y in value.positions],
        opacity=value.opacity, multiply=value.multiply, screen=value.screen, draw_order=value.draw_order)

def wire(value: MeshBindingSpec) -> dict[str, object]:
    return dict(mesh_id=value.mesh_id, id=value.id, axes=[dict(parameter_id=a.parameter_id, keys=a.keys) for a in value.axes],
        keyforms=[keyform_wire(k) for k in value.keyforms])

@dataclass(frozen=True)
class Sample:
    id: str
    value: float

@dataclass(frozen=True)
class AxisSnapshot:
    id: str
    parameter_id: str
    samples: tuple[Sample, ...]

@dataclass(frozen=True)
class Appearance:
    opacity: float
    multiply: tuple[float, float, float]
    screen: tuple[float, float, float]

@dataclass(frozen=True)
class KeyformSnapshot:
    samples: tuple[str, ...]
    positions: tuple[Point, ...]
    appearance: Appearance
    draw_order: float | None

@dataclass(frozen=True)
class Snapshot:
    source: Source
    id: str
    mesh_id: str
    axes: tuple[AxisSnapshot, ...]
    keyforms: tuple[KeyformSnapshot, ...]

    @classmethod
    def read(cls, raw: object, source: Source) -> "Snapshot":
        if isinstance(raw,m.MeshBinding):
            return cls(source,raw.id,raw.mesh_id,
                tuple(AxisSnapshot(a.id,a.parameter_id,tuple(Sample(s.id,s.value) for s in a.samples)) for a in raw.axes),
                tuple(KeyformSnapshot(k.samples,k.positions,Appearance(k.appearance.opacity,k.appearance.multiply,k.appearance.screen),k.draw_order) for k in raw.keyforms))
        value = record(raw)
        axes: list[AxisSnapshot] = []
        for item in sequence(value["axes"]):
            a = record(item)
            axes.append(AxisSnapshot(string(a["id"]), string(a["parameter_id"]),
                tuple(Sample(string(record(s)["id"]), number(record(s)["value"])) for s in sequence(a["samples"]))))
        keyforms: list[KeyformSnapshot] = []
        for item in sequence(value["keyforms"]):
            k = record(item)
            a = record(k["appearance"])
            keyforms.append(KeyformSnapshot(tuple(string(s) for s in sequence(k["samples"])), points(k["positions"]),
                Appearance(number(a["opacity"]), color(a["multiply"]), color(a["screen"])),
                None if k["draw_order"] is None else number(k["draw_order"])))
        return cls(source, string(value["id"]), string(value["mesh_id"]), tuple(axes), tuple(keyforms))

@dataclass(frozen=True)
class Batch:
    source: Source
    items: tuple[Snapshot, ...]

    @classmethod
    def read(cls, raw: object) -> "Batch":
        value = record(raw)
        source = Source.read(value["source"])
        return cls(source, tuple(Snapshot.read(item, source) for item in sequence(value["bindings"])))
