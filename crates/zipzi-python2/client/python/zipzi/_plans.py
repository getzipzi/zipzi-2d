"""Typed plans cache immutable reports independently of their execution lease."""
from __future__ import annotations
from typing import TYPE_CHECKING
from ._owned import Owned
from ._values import record
if TYPE_CHECKING:
    from .backends import Backend
from ._generated.plan_values import PlanCost as PlanCost, PlanReport as PlanReport
from . import ops

class Plan(Owned):
    def __init__(self, backend: Backend):
        super().__init__(backend, "plan")
        self._report: PlanReport | None = None

    def _load(self, value: object) -> Plan:
        r = record(value)
        self._token = r["plan"]
        self._report = PlanReport.read(r["source"], r["report"])
        return self

    @classmethod
    def create(cls, backend: Backend, session: object, edit: object | None, operation: ops.PlannableOperation) -> Plan:
        from . import _planning
        plan = cls(backend)
        return plan._request("plan", lambda: _planning.request(backend, session, edit, operation, plan), plan._load)

    @property
    def report(self) -> PlanReport:
        assert self._report is not None
        return self._report

    def __enter__(self) -> Plan:
        super().__enter__()
        return self

Plan.__module__ = "zipzi"
