"""Typed parameter inputs and immutable source-bound reads."""
from dataclasses import dataclass
from enum import Enum
import math
from ._values import Source, integer, number, record, sequence, string

class Kind(str, Enum):
    NORMAL = "normal"
    BLEND_SHAPE = "blend_shape"

@dataclass(frozen=True)
class ParameterSpec:
    name: str
    minimum: float
    maximum: float
    default_value: float
    id: str | None = None
    runtime_id: str | None = None
    kind: Kind | None = None
    decimal_places: int | None = None
    repeat: bool | None = None

@dataclass(frozen=True)
class Patch:
    name: str | None = None
    minimum: float | None = None
    maximum: float | None = None
    default_value: float | None = None
    runtime_id: str | None = None
    kind: Kind | None = None
    decimal_places: int | None = None
    repeat: bool | None = None

def validate(value: ParameterSpec | Patch) -> None:
    if not isinstance(value, (ParameterSpec, Patch)):
        raise TypeError("Expected a ParameterSpec or Patch")
    if isinstance(value, ParameterSpec):
        if not isinstance(value.name, str):
            raise TypeError("Expected parameter name text")
        if value.id is not None and not isinstance(value.id, str):
            raise TypeError("Expected parameter ID text")
    elif value.name is not None and not isinstance(value.name, str):
        raise TypeError("Expected parameter name text")
    for scalar in (value.minimum, value.maximum, value.default_value):
        if scalar is None and isinstance(value, Patch):
            continue
        if not isinstance(scalar, (int, float)) or isinstance(scalar, bool):
            raise TypeError("Parameter numbers must be real")
        if not math.isfinite(scalar) or abs(scalar) > 3.4028234663852886e38:
            raise ValueError("Parameter numbers must be finite and representable as f32")
    if value.runtime_id is not None and not isinstance(value.runtime_id, str):
        raise TypeError("Expected runtime ID text")
    if value.kind is not None and not isinstance(value.kind, Kind):
        raise TypeError("Expected a parameter Kind")
    if value.decimal_places is not None and (type(value.decimal_places) is not int or not -(1 << 31) <= value.decimal_places < (1 << 31)):
        raise ValueError("Precision must encode as i32")
    if value.repeat is not None and type(value.repeat) is not bool:
        raise TypeError("Expected a repeat flag")

def wire(value: ParameterSpec | Patch) -> dict[str, object]:
    result: dict[str, object] = dict(name=value.name, minimum=value.minimum, maximum=value.maximum,
        default_value=value.default_value, runtime_id=value.runtime_id,
        kind=None if value.kind is None else value.kind.value, decimal_places=value.decimal_places, repeat=value.repeat)
    if isinstance(value, ParameterSpec):
        result["id"] = value.id
    return result

@dataclass(frozen=True)
class Snapshot:
    source: Source
    id: str
    runtime_id: str
    name: str
    minimum: float
    maximum: float
    default_value: float
    kind: Kind
    decimal_places: int
    repeat: bool

    @classmethod
    def read(cls, raw: object, source: Source) -> "Snapshot":
        value = record(raw)
        repeat = value["repeat"]
        if type(repeat) is not bool:
            raise TypeError("Expected a repeat flag")
        return cls(source, string(value["id"]), string(value["runtime_id"]), string(value["name"]),
            number(value["minimum"]), number(value["maximum"]), number(value["default_value"]),
            Kind(string(value["kind"])), integer(value["decimal_places"]), repeat)

@dataclass(frozen=True)
class Batch:
    source: Source
    items: tuple[Snapshot, ...]

    @classmethod
    def read(cls, raw: object) -> "Batch":
        value = record(raw)
        source = Source.read(value["source"])
        return cls(source, tuple(Snapshot.read(item, source) for item in sequence(value["parameters"])))
