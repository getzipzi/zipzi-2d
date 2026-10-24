"""Typed plans cache immutable reports independently of their execution lease."""
from __future__ import annotations
from dataclasses import dataclass
from typing import TYPE_CHECKING
from . import meshes, resources
from ._owned import Owned
from ._values import Source, Version, integer, record, sequence, string
if TYPE_CHECKING:
    from .backends import Backend
Intent = meshes.RenameIntent | resources.RenameIntent

@dataclass(frozen=True)
class Cost:
    sample_combinations: int
    payload_bytes: int
    temporary_bytes: int
    history_bytes: int

    @classmethod
    def read(cls, value: object) -> Cost:
        r = record(value)
        return cls(*(integer(r[k]) for k in ("sample_combinations", "payload_bytes", "temporary_bytes", "history_bytes")))

@dataclass(frozen=True)
class Report:
    source: Source
    version: Version
    affected_ids: tuple[str, ...]
    copied_ids: tuple[tuple[str, str], ...]
    cost: Cost | None
    losses: tuple[str, ...]
    no_op: bool | None

    @classmethod
    def read(cls, source: object, value: object) -> Report:
        r = record(value)
        no_op = r["no_op"]
        if no_op is not None and type(no_op) is not bool:
            raise TypeError("Expected an optional boolean")
        return cls(Source.read(source), Version.read(r["version"]),
            tuple(string(v) for v in sequence(r["affected_ids"])),
            tuple((k, string(v)) for k, v in sorted(record(r["copied_ids"]).items())),
            None if r["cost"] is None else Cost.read(r["cost"]),
            tuple(string(v) for v in sequence(r["losses"])), no_op)

class Plan(Owned):
    def __init__(self, backend: Backend):
        super().__init__(backend, "plan")
        self._report: Report | None = None

    @classmethod
    def create(cls, backend: Backend, session: object, edit: object | None, intent: Intent) -> Plan:
        if not isinstance(intent, (meshes.RenameIntent, resources.RenameIntent)):
            raise TypeError("Expected a typed Plan intent")
        if any(not isinstance(v, str) for v in (intent.id, intent.expected_name, intent.name)):
            raise TypeError("Plan intent fields must be text")
        plan = cls(backend)
        def load(value: object) -> Plan:
            r = record(value)
            plan._token = r["plan"]
            plan._report = Report.read(r["source"], r["report"])
            return plan
        return plan._request("plan", lambda: backend.plan(session, edit, intent, plan), load)

    @property
    def report(self) -> Report:
        assert self._report is not None
        return self._report

    def __enter__(self) -> Plan:
        super().__enter__()
        return self
