"""Typed plans cache immutable reports independently of their execution lease."""
from __future__ import annotations
from typing import TYPE_CHECKING
from ._owned import Owned
from ._values import record
from . import scene
from ._generated import modeling
if TYPE_CHECKING:
    from .backends import Backend
from ._generated.plan_values import Cost as Cost, Report as Report
from ._generated.plan_intents import Intent as Intent, validate as validate_intent

class Plan(Owned):
    def __init__(self, backend: Backend):
        super().__init__(backend, "plan")
        self._report: Report | None = None

    def _load(self, value: object) -> Plan:
        r = record(value)
        self._token = r["plan"]
        self._report = Report.read(r["source"], r["report"])
        return self

    @classmethod
    def create_modeling(cls, backend: Backend, session: object, edit: object | None, intent: modeling.ModelingIntent) -> Plan:
        modeling.validate_ModelingIntent(intent)
        plan = cls(backend)
        return plan._request("plan", lambda: backend.modeling_plan(session, edit, intent, plan), plan._load)

    @classmethod
    def create_scene(cls, backend: Backend, session: object, edit: object | None, operation: scene.Operation) -> Plan:
        scene.validate(operation)
        plan = cls(backend)
        return plan._request("plan", lambda: backend.scene_plan(session, edit, operation, plan), plan._load)

    @classmethod
    def create(cls, backend: Backend, session: object, edit: object | None, intent: Intent) -> Plan:
        validate_intent(intent)
        plan = cls(backend)
        return plan._request("plan", lambda: backend.plan(session, edit, intent, plan), plan._load)

    @property
    def report(self) -> Report:
        assert self._report is not None
        return self._report

    def __enter__(self) -> Plan:
        super().__enter__()
        return self
