"""Immutable JSON extensions in canonical document fields only."""
from collections.abc import Mapping
from types import MappingProxyType
from typing import TypeAlias, cast
import math
ExtensionValue: TypeAlias = None | bool | int | float | str | tuple['ExtensionValue', ...] | Mapping[str, 'ExtensionValue']
def freeze(value: ExtensionValue) -> ExtensionValue:
    if isinstance(value, Mapping):
        return MappingProxyType({k: freeze(v) for k, v in value.items()})
    if isinstance(value, (tuple, list)):
        return tuple(freeze(v) for v in cast(tuple[ExtensionValue, ...], value))
    return value
def validate(value: ExtensionValue) -> None:
    if value is None or isinstance(value, (bool, str)): return
    if isinstance(value, int):
        if not -(1 << 63) <= value < (1 << 64): raise ValueError('Extension integer exceeds JSON range')
        return
    if isinstance(value, float):
        if not math.isfinite(value): raise ValueError('Extension number must be finite')
        return
    if isinstance(value, Mapping):
        for k,v in value.items():
            if not isinstance(k,str): raise TypeError('Extension key must be text')
            validate(v)
        return
    if isinstance(value, tuple):
        for v in value: validate(v)
        return
    raise TypeError('Invalid extension value')
def encode(value: ExtensionValue) -> object:
    if isinstance(value, Mapping): return {k: encode(v) for k,v in value.items()}
    if isinstance(value, tuple): return [encode(v) for v in value]
    return value
def read(value: object) -> ExtensionValue:
    if value is None or isinstance(value, (bool,int,float,str)): return value
    if isinstance(value, dict): return MappingProxyType({str(k): read(v) for k,v in cast(dict[str,object],value).items()})
    if isinstance(value, (list,tuple)): return tuple(read(v) for v in cast(tuple[object,...],value))
    raise TypeError('Invalid JSON extension response')
