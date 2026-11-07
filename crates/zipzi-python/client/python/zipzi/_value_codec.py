"""Serde-compatible codecs for generated owned values; no authoring algorithms."""

from __future__ import annotations
from collections.abc import Mapping
from enum import Enum
from importlib.resources import files
import json
import keyword
import math
import reprlib
from types import MappingProxyType
from typing import Any, Self, cast

_SCHEMAS: dict[str, Any] = {}
_TYPES: dict[str, Any] = {}
_VARIANTS: dict[str, Any] = {}
_PREVIEW = reprlib.Repr()
_PREVIEW.maxstring = 96
_PREVIEW.maxother = 160


def register(namespace: dict[str, Any]) -> None:
    schemas = json.loads(
        files(__package__).joinpath("_schema.json").read_text(encoding="utf-8")
    )["definitions"]
    _SCHEMAS.update(schemas)
    for name, s in schemas.items():
        _TYPES[name] = namespace[name]
        if s["kind"] in ("serde_enum", "union") and not issubclass(
            namespace[name], Enum
        ):
            _VARIANTS[name] = {
                v.get("name", v.get("wire_name")): namespace[name + v["rust_name"]]
                for v in s["variants"]
            }


def _name(f: dict[str, Any]) -> str:
    return f["name"] + ("_" if keyword.iskeyword(f["name"]) else "")


def _fields(s: dict[str, Any]) -> list[dict[str, Any]]:
    return [f for f in s.get("fields", ()) if not f.get("skip")]


def default(cls: type[Value], field_name: str) -> Any:
    """Decode a generated constructor default from the installed schema."""
    s = _SCHEMAS[cls._type]
    if cls._variant is not None:
        s = next(
            v
            for v in s["variants"]
            if v.get("name", v.get("wire_name")) == cls._variant
        )
    field = next(f for f in _fields(s) if _name(f) == field_name)
    value = field["default"]
    if value == {"kind": "null"}:
        value = None
    elif value == {"kind": "empty_string"}:
        value = ""
    return decode(field["shape"], value)


def _freeze(v: Any) -> Any:
    if isinstance(v, Mapping):
        return MappingProxyType(
            {k: _freeze(x) for k, x in cast(Mapping[Any, Any], v).items()}
        )
    if isinstance(v, (tuple, list)):
        return tuple(_freeze(x) for x in cast(list[Any], v))
    return v


def encode(value: Any) -> Any:
    if isinstance(value, Value):
        return value.to_wire()
    if isinstance(value, Enum):
        return value.value
    if isinstance(value, Mapping):
        return {str(k): encode(v) for k, v in cast(Mapping[Any, Any], value).items()}
    if isinstance(value, (tuple, list)):
        return [encode(v) for v in cast(list[Any], value)]
    if isinstance(value, float) and not math.isfinite(value):
        raise ValueError("Non-finite wire number")
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    raise TypeError("Unsupported wire value " + type(value).__name__)


def _record(cls: Any, s: dict[str, Any], value: Any) -> Any:
    if s.get("transparent") or (s.get("tuple") and len(_fields(s)) == 1):
        f = _fields(s)[0]
        return cls(**{_name(f): decode(f["shape"], value)})
    if not isinstance(value, Mapping):
        raise TypeError("Expected object for " + cls.__name__)
    args = {}
    for f in _fields(s):
        key = f["name"]
        if f.get("flatten"):
            args[_name(f)] = decode(f["shape"], value)
        elif key in value:
            args[_name(f)] = decode(f["shape"], value[key])
    return cls(**args)


def decode(s: dict[str, Any], value: Any) -> Any:
    kind = s["kind"]
    if kind == "shared":
        return decode(s["item"], value)
    if kind == "optional":
        return None if value is None else decode(s["item"], value)
    if kind == "ref":
        name = s["name"]
        cls = _TYPES[name]
        spec = _SCHEMAS[name]
        if isinstance(value, cls):
            return value
        if issubclass(cls, Enum):
            return cls(value)
        if spec["kind"] not in ("serde_enum", "union"):
            return _record(cls, spec, value)
        tag = spec.get("tag")
        content = spec.get("content")
        if tag:
            variant = value[tag]
            payload = (
                value.get(content)
                if content
                else {
                    k: v for k, v in cast(Mapping[Any, Any], value).items() if k != tag
                }
            )
        elif isinstance(value, str):
            variant = value
            payload = None
        else:
            if len(value) != 1:
                raise ValueError("Expected one externally tagged variant")
            variant, payload = next(iter(value.items()))
        v = next(
            v for v in spec["variants"] if v.get("name", v.get("wire_name")) == variant
        )
        sub = _VARIANTS[name][variant]
        if not _fields(v):
            return sub()
        if v.get("form") == "newtype":
            f = _fields(v)[0]
            return sub(**{_name(f): decode(f["shape"], payload)})
        return _record(sub, v, payload)
    if kind in ("sequence", "array"):
        if isinstance(value, (str, bytes)) or not isinstance(value, (list, tuple)):
            raise TypeError("Expected sequence")
        if kind == "array" and len(cast(list[Any], value)) != s["length"]:
            raise ValueError("Array length mismatch")
        return tuple(decode(s["item"], v) for v in cast(list[Any], value))
    if kind == "tuple":
        if len(value) != len(s["items"]):
            raise ValueError("Tuple length mismatch")
        return tuple(decode(t, v) for t, v in zip(s["items"], value))
    if kind in ("string_map", "index_map"):
        if not isinstance(value, Mapping):
            raise TypeError("Expected map")
        return MappingProxyType(
            {
                str(k) if kind == "string_map" else decode(s["key"], int(k)): decode(
                    s["value"], v
                )
                for k, v in cast(Mapping[Any, Any], value).items()
            }
        )
    if kind == "json":
        return _freeze(value)
    if kind == "string":
        if not isinstance(value, str):
            raise TypeError("Expected string")
        return value
    if kind == "bool":
        if type(value) is not bool:
            raise TypeError("Expected bool")
        return value
    if kind in ("f32", "f64"):
        if (
            isinstance(value, bool)
            or not isinstance(value, (float, int))
            or not math.isfinite(value)
        ):
            raise ValueError("Expected finite number")
        if kind == "f32" and abs(value) > 3.4028234663852886e38:
            raise ValueError("Number outside f32")
        return float(value)
    if kind in ("u8", "u16", "u32", "u64", "usize", "i32", "i64"):
        if type(value) is not int:
            raise TypeError("Expected integer")
        bits = 64 if kind == "usize" else int(kind[1:])
        signed = kind.startswith("i")
        low = -(1 << (bits - 1)) if signed else 0
        high = (1 << (bits - 1 if signed else bits)) - 1
        if not low <= value <= high:
            raise ValueError("Integer outside " + kind)
        return value
    raise TypeError("Unknown shape " + kind)


class Value:
    """Immutable contract value with schema-checked construction and wire conversion."""

    _type: str
    _variant: str | None = None

    def __repr__(self) -> str:
        s = _SCHEMAS[self._type]
        if self._variant is not None:
            s = next(
                v
                for v in s["variants"]
                if v.get("name", v.get("wire_name")) == self._variant
            )
        visible = _fields(s)
        parts = [
            f"{_name(f)}={_PREVIEW.repr(getattr(self, _name(f)))}" for f in visible[:6]
        ]
        if len(visible) > 6:
            parts.append("...")
        return type(self).__name__ + "(" + ", ".join(parts) + ")"

    def __post_init__(self) -> None:
        s = _SCHEMAS[self._type]
        if self._variant is not None:
            s = next(
                v
                for v in s["variants"]
                if v.get("name", v.get("wire_name")) == self._variant
            )
        for f in _fields(s):
            object.__setattr__(
                self, _name(f), decode(f["shape"], getattr(self, _name(f)))
            )

    @classmethod
    def from_wire(cls, value: Any) -> Self:
        """Decode an owned value using this type's schema; invalid fields raise errors."""
        return cast(Self, decode({"kind": "ref", "name": cls._type}, value))

    def to_wire(self) -> Any:
        """Return the complete serializable value, including fields omitted by repr()."""
        s = _SCHEMAS[self._type]
        variant = self._variant
        fields = (
            s
            if variant is None
            else next(
                v for v in s["variants"] if v.get("name", v.get("wire_name")) == variant
            )
        )
        data: dict[str, Any] = {}
        for f in _fields(fields):
            value = encode(getattr(self, _name(f)))
            if f.get("flatten"):
                data.update(value)
            else:
                data[f["name"]] = value
        if variant is None:
            if s.get("transparent") or (s.get("tuple") and len(data) == 1):
                return next(iter(data.values()))
            return data
        payload = next(iter(data.values())) if fields.get("form") == "newtype" else data
        tag = s.get("tag")
        content = s.get("content")
        if tag:
            if not _fields(fields):
                return {tag: variant}
            if content:
                return {tag: variant, content: payload}
            return {tag: variant, **payload}
        return variant if not _fields(fields) else {variant: payload}
