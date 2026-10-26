"""Direct mechanical PyO3 conversion; local authoring does not serialize JSON."""
from importlib import import_module
from typing import TYPE_CHECKING, Literal, cast
import time
from .. import _operations as ops
from .._generated.backend_bindings import LocalBindings
from .._errors import JobTimeout, OutcomeUnknown, PendingRequest, SdkFailure, unwrap
from .._values import ConnectionInfo, SessionId, Source
from . import JobKind
if TYPE_CHECKING:
    from ._native_types import NativeModule, SessionToken

class LocalBackend(LocalBindings):
    def _transport_buffer_create(self, length: int, owner: object) -> object:
        return unwrap(self._native.buffer_create(length))
    def _transport_buffer_append(self, blob: object, offset: int, data: bytes, owner: object) -> object:
        return unwrap(self._native.buffer_append(blob, offset, data))
    def _transport_buffer_seal(self, blob: object, owner: object) -> object:
        return unwrap(self._native.buffer_seal(blob))
    def _transport_buffer_read(self, blob: object, offset: int, length: int, owner: object) -> object:
        return unwrap(self._native.buffer_read(blob, offset, length))
    def _transport_prepare_image_buffer(self, width: int, height: int, blob: object, owner: object) -> object:
        return unwrap(self._native.prepare_image_buffer(width, height, blob))
    connection_info: ConnectionInfo | None = None
    def __init__(self) -> None:
        try:
            module = cast("NativeModule", import_module("zipzi_native._native"))
            self._native = module._Runtime()
        except ImportError as e:
            raise SdkFailure("LOCAL_BACKEND_MISSING", "Install zipzi-native matching your zipzi version") from e
        except RuntimeError as e:
            raise SdkFailure("EXECUTOR_FAILURE", str(e)) from e
        except AttributeError as e:
            raise SdkFailure("CONTRACT_MISMATCH", "Native module lacks the typed Runtime contract") from e
        if self._native.contract != "zipzi-python2-15":
            self._native.close()
            raise SdkFailure("CONTRACT_MISMATCH", "Native contract differs")

    def session_id(self, token: object) -> SessionId:
        value = cast("SessionToken", token)
        return SessionId(value.runtime, value.serial)
    def show_window(self, session: object, owner: object) -> None:
        raise SdkFailure("UNSUPPORTED_OPERATION", "Local runtime has no Editor windows", operation="session.show_window")
    def _transport_apply(self, edit: object, operations: tuple[ops.WireOperation, ...], owner: object) -> object:
        return unwrap(self._native.apply(edit, operations))
    def _transport_prepare_image(self, width: int, height: int, rgba: bytes, owner: object) -> object:
        return unwrap(self._native.prepare_image(width, height, rgba))
    def _transport_release_handle(self, kind: Literal["image", "plan", "reference", "address", "buffer"], token: object, owner: object) -> None:
        unwrap(self._native.release_handle(kind, token))
    def _test_control(self, action: str) -> object:
        return unwrap(self._native._test_control(action))
    def _transport_wait(self, job: object, kind: JobKind, owner: object, *, timeout: float | None = None) -> object:
        if timeout is not None:
            deadline = time.monotonic() + timeout
            while unwrap(self._native.job_state(job)) not in {"succeeded", "failed", "cancelled"}:
                if time.monotonic() >= deadline:
                    raise JobTimeout()
                time.sleep(min(.005, max(0., deadline - time.monotonic())))
        return unwrap(self._native.wait(job))
    def pending(self, owner: object) -> PendingRequest | None:
        return None
    def pending_error(self) -> OutcomeUnknown | None:
        return None
    def recover(self, owner: object) -> object:
        raise SdkFailure("NO_PENDING_REQUEST", "Local calls have no transport replay window")
    def _transport_close(self, *, abandon_pending: bool = False) -> None:
        unwrap(self._native.close())


    def _transport_read_selection(self, session: object, edit: object | None, domain: str, ids: tuple[str,...], expected: Source | None, owner: object) -> object:
        return unwrap(self._native.read_selection(session,edit,domain,ids,expected))
