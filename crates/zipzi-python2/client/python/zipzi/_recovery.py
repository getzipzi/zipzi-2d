"""Own one exact pending request across transport loss and recovery."""
from __future__ import annotations
from collections.abc import Callable
from typing import TYPE_CHECKING, TypeVar
from ._errors import Busy, OutcomeUnknown, PendingRequest
from ._calls import Call
if TYPE_CHECKING:
    from .backends import Backend
T = TypeVar("T")

class RecoveryOwner:
    def __init__(self, backend: Backend):
        self._backend = backend
        self._action: str | None = None
        self._call: Call[object] | None = None
        self._unknown_error: OutcomeUnknown | None = None

    @property
    def pending_request(self) -> PendingRequest | None:
        return self._backend.pending(self)

    def _request_unknown(self, action: str, error: OutcomeUnknown) -> None:
        self._unknown_error = error

    def _request_failed(self, action: str, error: BaseException) -> None:
        pass

    def _request(self, action: str, call: Callable[[], object], decode: Callable[[object], T]) -> T:
        pending = self.pending_request
        if pending is not None and self._action != action:
            raise Busy("BUSY", "Recover the pending operation before another action", recovery_action="recover_original")
        if pending is not None and self._call is not None:
            from typing import cast
            return cast(T, self._call.recover(pending))
        self._action = action
        request = Call(action, call, decode, resume=lambda: self._backend.recover(self),
                       on_unknown=lambda error: self._request_unknown(action, error),
                       on_error=lambda error: self._request_failed(action, error))
        self._call = request
        return request.run()
