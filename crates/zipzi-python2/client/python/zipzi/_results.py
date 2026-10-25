"""Immutable query results; reading a result never contacts its Runtime."""
from dataclasses import dataclass, field
from typing import Generic, TypeVar
from ._values import Source

T = TypeVar("T", covariant=True)


@dataclass(frozen=True)
class Snapshot(Generic[T]):
    """One owned domain value at an exact formal or candidate source."""
    __module__ = "zipzi"
    source: Source
    value: T


@dataclass(frozen=True)
class Batch(Generic[T]):
    """Ordered values, including duplicates, sharing one exact source."""
    __module__ = "zipzi"
    source: Source
    items: tuple[T, ...]

    def __post_init__(self) -> None:
        object.__setattr__(self, "items", tuple(self.items))


@dataclass(frozen=True)
class PageCursor:
    """Opaque continuation issued by a Runtime collection query."""
    __module__ = "zipzi"
    _token: bytes = field(repr=False)


@dataclass(frozen=True)
class Page(Generic[T]):
    """A bounded collection page; Runtime pagination is delivered in P3A-4."""
    __module__ = "zipzi"
    source: Source
    items: tuple[T, ...]
    next_cursor: PageCursor | None = None

    def __post_init__(self) -> None:
        object.__setattr__(self, "items", tuple(self.items))
