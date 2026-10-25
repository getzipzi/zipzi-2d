"""Immutable query results; reading a result never contacts its Runtime."""
from ._presentation import CompactValue
from dataclasses import dataclass, field
from typing import Generic, TypeVar
from ._values import Source

T = TypeVar("T", covariant=True)


@dataclass(frozen=True, repr=False)
class Snapshot(CompactValue, Generic[T]):
    """One owned domain value at an exact formal or candidate source."""
    __module__ = "zipzi"
    source: Source
    value: T


@dataclass(frozen=True, repr=False)
class Batch(CompactValue, Generic[T]):
    """Ordered values, including duplicates, sharing one exact source."""
    __module__ = "zipzi"
    source: Source
    items: tuple[T, ...]

    def __post_init__(self) -> None:
        object.__setattr__(self, "items", tuple(self.items))


@dataclass(frozen=True, repr=False)
class PageCursor(CompactValue):
    """Opaque continuation issued by a Runtime collection query."""
    __module__ = "zipzi"
    _token: bytes = field(repr=False)


@dataclass(frozen=True, repr=False)
class Page(CompactValue, Generic[T]):
    """A bounded metadata page at one exact source; continue with next_cursor."""
    __module__ = "zipzi"
    source: Source
    items: tuple[T, ...]
    next_cursor: PageCursor | None = None

    def __post_init__(self) -> None:
        object.__setattr__(self, "items", tuple(self.items))
