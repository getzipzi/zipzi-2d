"""Common complete-result projections for native, JSON and binary replies."""
from collections.abc import Callable
from typing import TypeVar, cast
from ._results import Batch, Snapshot
from ._values import Source, record, sequence
from ._generated import values as modeling, codecs as _modeling_codec

T = TypeVar("T")


def snapshot(raw: object, read: Callable[[object], T]) -> Snapshot[T]:
    if isinstance(raw, Snapshot):
        value = cast(Snapshot[object], raw)
        return Snapshot(value.source, read(value.value))
    value = record(raw)
    return Snapshot(Source._read(value["source"]), read(value))


def batch(raw: object, key: str, read: Callable[[object], T]) -> Batch[T]:
    if isinstance(raw, Batch):
        value = cast(Batch[object], raw)
        return Batch(value.source, tuple(read(item) for item in value.items))
    value = record(raw)
    return Batch(Source._read(value["source"]), tuple(read(item) for item in sequence(value[key])))


def single(value: Batch[T]) -> Snapshot[T]:
    if len(value.items) != 1:
        raise ValueError("Expected exactly one query value")
    return Snapshot(value.source, value.items[0])


def records(value: object) -> Snapshot[modeling.OwnedModelingRecords]:
    raw = record(value)
    return Snapshot(Source._read(raw["source"]), _modeling_codec.read_OwnedModelingRecords(raw["records"]))


def capabilities(value: object) -> Snapshot[modeling.Capabilities]:
    raw = record(value)
    return Snapshot(Source._read(raw["source"]), _modeling_codec.read_Capabilities(raw["capabilities"]))


def editor_context(value: object) -> Snapshot[modeling.EditorContext | None]:
    raw = record(value)
    return Snapshot(Source._read(raw["source"]), None if raw["context"] is None else _modeling_codec.read_EditorContext(raw["context"]))
