"""Canonical modeling values and source-bearing selected record snapshots.

Operations use the same SDK authoring/plan engines in both backends. Collection
and parameter sweeps are explicit on Session.modeling and Edit.modeling.
"""
from dataclasses import dataclass
from ._values import Source, record
from ._generated.modeling import *
from ._generated.modeling import OwnedModelingRecords, read_OwnedModelingRecords, Capabilities, read_Capabilities

@dataclass(frozen=True)
class Records:
    source: Source
    values: OwnedModelingRecords

    @classmethod
    def read(cls, value: object) -> 'Records':
        r = record(value)
        return cls(Source.read(r['source']), read_OwnedModelingRecords(r['records']))

@dataclass(frozen=True)
class CapabilitySnapshot:
    source: Source
    values: Capabilities

    @classmethod
    def read(cls, value: object) -> 'CapabilitySnapshot':
        r = record(value)
        return cls(Source.read(r['source']), read_Capabilities(r['capabilities']))

@dataclass(frozen=True)
class EditorSnapshot:
    source:Source
    context:EditorContext | None
    @classmethod
    def read(cls,raw:object)->'EditorSnapshot':
        value=record(raw)
        return cls(Source.read(value["source"]),None if value["context"] is None else read_EditorContext(value["context"]))
