"""Own one exact pending request across transport loss and recovery."""
from __future__ import annotations
from collections.abc import Callable
from typing import TYPE_CHECKING, TypeVar
from ._errors import Busy, OutcomeUnknown, PendingRequest, SdkFailure
if TYPE_CHECKING:
    from .backends import Backend
T = TypeVar("T")

class RecoveryOwner:
    def __init__(self, backend: Backend):
        self._backend = backend
        self._action: str | None = None

    @property
    def pending_request(self) -> PendingRequest | None:
        return self._backend.pending(self)

    def _request(self, action: str, call: Callable[[], object], decode: Callable[[object], T]) -> T:
        pending = self.pending_request is not None
        if pending and self._action != action:
            raise Busy("BUSY", "Recover the pending operation before another action")
        self._action = action
        try:
            result = decode(self._backend.recover(self) if pending else call())
        except OutcomeUnknown as e:
            expected = e.pending
            def resume() -> T:
                if self.pending_request != expected:
                    raise SdkFailure("NO_PENDING_REQUEST", "This request has already been resolved")
                return self._request(action, call, decode)
            e._resume = resume
            raise
        except BaseException:
            if self.pending_request is None:
                self._action = None
            raise
        self._action = None
        return result
