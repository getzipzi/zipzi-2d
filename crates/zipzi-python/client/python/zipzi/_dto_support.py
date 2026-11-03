"""Primitive ownership/shape conversions shared by generated DTO projections."""
from collections.abc import Callable, Mapping, Sequence
from typing import TypeVar, cast
from uuid import UUID
import math
from ._values import SessionId

T = TypeVar('T')

def get(value: object, key: str) -> object:
    if isinstance(value, Mapping): return cast(Mapping[str, object], value)[key]
    return cast(object, getattr(value, key))

def text(value: object) -> str:
    if not isinstance(value, str): raise TypeError('Expected text')
    return value

def numeric(value: object, kind: str = 'f32') -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)): raise TypeError('Expected a real number')
    if not math.isfinite(value) or kind == 'f32' and abs(value) > 3.4028234663852886e38: raise ValueError('Expected finite '+kind)
    return float(value)

def whole(value: object, kind: str) -> int:
    if type(value) is not int: raise ValueError('Expected '+kind)
    bits = {'u8':8, 'u16':16, 'u32':32, 'u64':64, 'i32':32, 'i64':64, 'usize':64}[kind]
    low, high = (-(1 << (bits-1)), 1 << (bits-1)) if kind.startswith('i') else (0, 1 << bits)
    if not low <= value < high: raise ValueError('Expected '+kind)
    return value

def flag(value: object) -> bool:
    if type(value) is not bool: raise TypeError('Expected bool')
    return value

def point(value: object) -> tuple[float, float]:
    if isinstance(value, (tuple, list)):
        values=cast(Sequence[object], value)
        if len(values)!=2: raise ValueError('Expected point pair')
        return numeric(values[0]), numeric(values[1])
    return numeric(get(value,'x')), numeric(get(value,'y'))

def items(value: object) -> Sequence[object]:
    if not isinstance(value, (list, tuple)): raise TypeError('Expected immutable array input')
    return cast(Sequence[object], value)

def array(value: Sequence[T], length: int) -> tuple[T, ...]:
    if len(value)!=length: raise ValueError('Invalid fixed array length')
    return tuple(value)

def mapping(value: object) -> Mapping[str, object]:
    if not isinstance(value, Mapping): raise TypeError('Expected mapping')
    result=cast(Mapping[object, object], value)
    if any(not isinstance(key,str) for key in result): raise TypeError('Expected text keys')
    return cast(Mapping[str, object], result)

def cursor_guard(runtime: str, session: SessionId | None, sequence: int) -> None:
    if str(UUID(runtime))!=runtime or type(sequence) is not int or not 0<=sequence<1<<64: raise ValueError('Invalid event cursor')
    if session is not None and session.runtime!=runtime: raise ValueError('Cursor mixes Runtime identities')


def external(value: object, cls: type[T], decode: Callable[[object], T]) -> T:
    return value if isinstance(value, cls) else decode(value)
