"""Immutable query results; reading a result never contacts its Runtime."""
from ._presentation import CompactValue
from dataclasses import dataclass, field
from typing import Generic, TypeVar
from ._values import Source

T = TypeVar("T", covariant=True)


@dataclass(frozen=True, repr=False)
class Snapshot(CompactValue, Generic[T]):
    'One owned domain value at an exact formal or candidate source; field access performs no IO.\n\nFields:\nsource: Exact source captured atomically with the payload.\nvalue: Owned immutable domain payload; retain source separately when planning or checking freshness.'
    __module__ = "zipzi"
    source: Source
    value: T


@dataclass(frozen=True, repr=False)
class Batch(CompactValue, Generic[T]):
    'Ordered owned values sharing one exact source; duplicates and empty selections retain their semantics.\n\nFields:\nsource: Exact source shared by the whole selection, including an empty batch.\nitems: Owned payloads in requested-ID order, preserving duplicates; a missing item rejects a selected read as a whole.'
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
    'Bounded metadata page from one exact source; an empty page may still have a continuation.\n\nFields:\nsource: Exact formal version or candidate sequence captured for this page.\nitems: Metadata records in native storage order; excludes geometry, keyforms and pixel payloads.\nnext_cursor: Opaque continuation for the same domain/filter/source; None ends the scan. Stale cursors require restarting discovery.'
    __module__ = "zipzi"
    source: Source
    items: tuple[T, ...]
    next_cursor: PageCursor | None = None

    def __post_init__(self) -> None:
        object.__setattr__(self, "items", tuple(self.items))
