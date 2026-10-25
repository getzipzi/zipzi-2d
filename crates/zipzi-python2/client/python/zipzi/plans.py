"""Typed plans cache immutable reports independently of their execution lease."""
from __future__ import annotations
from typing import TYPE_CHECKING
from ._owned import Owned
from ._values import record
if TYPE_CHECKING:
    from .backends import Backend
from ._generated.plan_values import Cost as Cost, Report as Report
from ._generated.plan_intents import Intent as Intent, validate as validate_intent

class Plan(Owned):
    def __init__(self, backend: Backend):
        super().__init__(backend, "plan")
        self._report: Report | None = None

    @classmethod
    def create(cls, backend: Backend, session: object, edit: object | None, intent: Intent) -> Plan:
        validate_intent(intent)
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
