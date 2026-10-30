"""Typed plans cache immutable reports independently of their execution lease."""
from __future__ import annotations
from typing import TYPE_CHECKING
from ._owned import Owned
from ._values import record
if TYPE_CHECKING:
    from .backends import Backend
else:
    Backend = object
from ._generated.plan_values import PlanCost as PlanCost, PlanReport as PlanReport
from . import ops, modeling
from ._generated.codecs import deletion as _codec

class Plan(Owned):
    """Owned native plan returned by Session/Edit/address planning factories.

    report is cached and remains readable after close or invalidation. Applying
    through Edit requires the captured exact source and identity/context epochs;
    later writes, replacement or Undo can make it stale. Apply as the first and
    only Plan in a batch. Close the lease or use with after inspection/application;
    closing does not publish or revoke an already accepted document change.
    """
    def __init__(self, backend: Backend):
        super().__init__(backend, "plan")
        self._report: PlanReport | None = None

    def _load(self, value: object) -> Plan:
        r = record(value)
        self._token = r["plan"]
        self._report = PlanReport._read(r["source"], r["report"])
        return self

    @classmethod
    def _create(cls, backend: Backend, session: object, edit: object | None, operation: ops.PlannableOperation) -> Plan:
        from . import _planning
        plan = cls(backend)
        return plan._request("plan", lambda: _planning.request(backend, session, edit, operation, plan), plan._load)

    @property
    def report(self) -> PlanReport:
        'Return the immutable cached report, exact Source, SDK version, affected/copied IDs, cost, losses and no-op state. No IO occurs; the report remains readable after Plan close or invalidation.'
        assert self._report is not None
        return self._report

    def deletion_page(self, section: modeling.deletion.DeletionSection, *, offset: int = 0, limit: int = 64) -> modeling.deletion.DeletionPage:
        """Read a bounded section of this immutable deletion report.

        Source changes do not change these rows. The live Plan lease and its
        original recovery owner are required; each page contains 1 to 256 rows.
        """
        self._check(self._backend)
        if isinstance(offset, bool) or not isinstance(offset, int) or offset < 0:
            raise ValueError("offset must be a nonnegative integer")
        if isinstance(limit, bool) or not isinstance(limit, int) or not 1 <= limit <= 256:
            raise ValueError("limit must be an integer from 1 to 256")
        return self._request("deletion_page", lambda: self._backend.deletion_page(self._token, section, offset, limit, self), _codec.read_DeletionPage)

    def __enter__(self) -> Plan:
        'Check the owning client/lease locally and return this Plan without IO. Use with to close on exit; pending work must recover on its original owner. Entering does not refresh or rebind stale SDK identity.'
        super().__enter__()
        return self

Plan.__module__ = "zipzi"
