"""Client-owned handle lifecycle; host tables remain authoritative."""
from __future__ import annotations
from collections.abc import Callable
from typing import TYPE_CHECKING, Literal, TypeVar
from ._errors import Busy, OutcomeUnknown, PendingRequest, SdkFailure
if TYPE_CHECKING:
    from .backends import Backend
T = TypeVar("T")

class Owned:
    def __init__(self, backend: Backend, kind: Literal["image", "plan"]):
        self._backend, self._kind = backend, kind
        self._kind: Literal["image", "plan"]
        self._token: object = None
        self._closed = False
        self._action: str | None = None

    def _check(self, backend: Backend) -> None:
        if self._backend is not backend:
            raise SdkFailure("WRONG_OWNER", "Handle belongs to another Runtime connection")
        if self._closed:
            raise SdkFailure("INVALID_REFERENCE", "Handle is closed")
        if self.pending_request is not None:
            raise Busy("BUSY", "Recover this handle's pending request first")

    @property
    def pending_request(self) -> PendingRequest | None:
        return self._backend.pending(self)

    def _request(self, action: str, call: Callable[[], object], decode: Callable[[object], T]) -> T:
        pending = self.pending_request is not None
        if pending and self._action != action:
            raise Busy("BUSY", "Recover the pending handle operation before another action")
        if self._closed:
            raise SdkFailure("INVALID_REFERENCE", "Handle is closed")
        self._action = action
        try:
            value = self._backend.recover(self) if pending else call()
            result = decode(value)
        except OutcomeUnknown as e:
            expected = e.pending
            def resume() -> T:
                if self.pending_request != expected:
                    raise SdkFailure("NO_PENDING_REQUEST", "This handle request has already been resolved")
                return self._request(action, call, decode)
            e._resume = resume
            raise
        except BaseException:
            if self.pending_request is None:
                self._action = None
            raise
        self._action = None
        return result

    def close(self) -> None:
        if self._closed:
            return
        def closed(_: object) -> None:
            self._closed = True
        self._request("close", lambda: self._backend.release_handle(self._kind, self._token, self), closed)

    def __enter__(self) -> Owned:
        self._check(self._backend)
        return self

    def __exit__(self, kind: object, exception: BaseException | None, traceback: object) -> None:
        try:
            self.close()
        except BaseException as cleanup:
            if exception is None:
                raise
            exception.add_note(f"Handle cleanup failed: {cleanup}")
