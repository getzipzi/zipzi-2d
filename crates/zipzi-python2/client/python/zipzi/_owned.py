"""Client-owned handle lifecycle; host tables remain authoritative."""
from __future__ import annotations
from collections.abc import Callable
from typing import TYPE_CHECKING, Literal, TypeVar
from ._errors import Busy, SdkFailure
from ._recovery import RecoveryOwner
if TYPE_CHECKING:
    from .backends import Backend
T = TypeVar("T")

class Owned(RecoveryOwner):
    def __init__(self, backend: Backend, kind: Literal["image", "plan", "reference", "address", "buffer"]):
        super().__init__(backend)
        self._kind = kind
        self._kind: Literal["image", "plan", "reference", "address", "buffer"]
        self._token: object = None
        self._closed = False

    def _check(self, backend: Backend) -> None:
        if self._backend is not backend:
            raise SdkFailure("WRONG_OWNER", "Handle belongs to another Runtime connection")
        if self._closed:
            raise SdkFailure("INVALID_REFERENCE", "Handle is closed")
        if self.pending_request is not None:
            raise Busy("BUSY", "Recover this handle's pending request first", recovery_action="recover_original")

    def _request(self, action: str, call: Callable[[], object], decode: Callable[[object], T]) -> T:
        if self._closed:
            raise SdkFailure("INVALID_REFERENCE", "Handle is closed")
        return super()._request(action, call, decode)

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
