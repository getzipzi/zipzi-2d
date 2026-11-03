"""Client-owned handle lifecycle; host tables remain authoritative."""
from __future__ import annotations
from collections.abc import Callable
from typing import TYPE_CHECKING, Literal, TypeVar
from ._errors import Busy, SdkFailure
from ._recovery import RecoveryOwner
if TYPE_CHECKING:
    from .backends import Backend
else:
    Backend = object
T = TypeVar("T")

class Owned(RecoveryOwner):
    def __init__(self, backend: Backend, kind: Literal["image", "plan", "reference", "address", "buffer", "preview"]):
        super().__init__(backend)
        self._kind = kind
        self._kind: Literal["image", "plan", "reference", "address", "buffer", "preview"]
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
        'Release this owned handle once, without mutating formal model content. A lost release response keeps the original recovery owner; confirmed close is idempotent and later use fails.'
        if self._closed:
            return
        def closed(_: object) -> None:
            self._closed = True
        self._request("close", lambda: self._backend.release_handle(self._kind, self._token, self), closed)

    def __enter__(self) -> Owned:
        """Validate this client's live lease and return it without a host request.

        Closed/foreign leases fail; pending work must be recovered first. This
        does not refresh SDK object incarnation. Use with to close on exit.
        """
        self._check(self._backend)
        return self

    def __exit__(self, kind: object, exception: BaseException | None, traceback: object) -> None:
        """Close the owned lease on scope exit; never suppress the body exception.

        Python supplies the context arguments. Closing can synchronously contact
        the host. On normal exit an unknown release remains recoverable and is
        raised; during an existing exception cleanup failure is added as a note.
        Retain the handle for pending cleanup. No formal document write occurs.
        """
        try:
            self.close()
        except BaseException as cleanup:
            if exception is None:
                raise
            exception.add_note(f"Handle cleanup failed: {cleanup}")
