"""Direct mechanical PyO3 conversion; local authoring does not serialize JSON."""
from importlib import import_module
from pathlib import Path
from typing import TYPE_CHECKING, Literal, cast
from .. import ops, plans
from .._generated.backends import LocalQueries
from .._errors import PendingRequest, SdkFailure, unwrap
from .._values import ConnectionInfo, SessionId
from . import JobKind
from ..events import Cursor
if TYPE_CHECKING:
    from ._native_types import NativeModule, SessionToken

class LocalBackend(LocalQueries):
    connection_info: ConnectionInfo | None = None
    def __init__(self) -> None:
        try:
            module = cast("NativeModule", import_module("zipzi_native._native"))
            self._native = module._Runtime()
        except ImportError as e:
            raise SdkFailure("LOCAL_BACKEND_MISSING", "Install the zipzi-python2-native") from e
        except RuntimeError as e:
            raise SdkFailure("EXECUTOR_FAILURE", str(e)) from e
        except AttributeError as e:
            raise SdkFailure("CONTRACT_MISMATCH", "Native module lacks the typed Runtime contract") from e
        if self._native.contract != "zipzi-python2-5":
            self._native.close()
            raise SdkFailure("CONTRACT_MISMATCH", "Native contract differs")

    def create(self, document_id: str, owner: object) -> object:
        return unwrap(self._native.create_session(document_id))
    def subscribe(self, session: object, owner: object) -> object:
        return unwrap(self._native.subscribe(session))
    def events(self, cursor: Cursor, limit: int, owner: object) -> object:
        return unwrap(self._native.events(cursor, limit))
    def parameters(self, session: object, edit: object | None, ids: tuple[str, ...] | None, owner: object) -> object:
        return unwrap(self._native.parameters(session, edit, ids))
    def meshes(self, session: object, edit: object | None, ids: tuple[str, ...], owner: object) -> object:
        return unwrap(self._native.meshes(session, edit, ids))
    def session_id(self, token: object) -> SessionId:
        value = cast("SessionToken", token)
        return SessionId(value.runtime, value.serial)
    def list_sessions(self, owner: object) -> object:
        return unwrap(self._native.list_sessions())
    def attach(self, session: SessionId, owner: object) -> object:
        return unwrap(self._native.attach(session.runtime, session.serial))
    def show_window(self, session: object, owner: object) -> None:
        raise SdkFailure("UNSUPPORTED_OPERATION", "Local runtime has no Editor windows", operation="session.show_window")
    def info(self, session: object, owner: object) -> object:
        return unwrap(self._native.info(session))
    def begin(self, session: object, label: str, owner: object) -> object:
        return unwrap(self._native.begin_edit(session, label))
    def apply(self, edit: object, operations: tuple[ops.Operation, ...], owner: object) -> object:
        return unwrap(self._native.apply(edit, operations))
    def prepare_image(self, width: int, height: int, rgba: bytes, owner: object) -> object:
        return unwrap(self._native.prepare_image(width, height, rgba))
    def prepared_pixels(self, image: object, owner: object) -> object:
        return unwrap(self._native.prepared_pixels(image))
    def image(self, session: object, id: str, edit: object | None, owner: object) -> object:
        return unwrap(self._native.image(session, id, edit))
    def plan(self, session: object, edit: object | None, intent: plans.Intent, owner: object) -> object:
        return unwrap(self._native.plan(session, edit, intent))
    def release_handle(self, kind: Literal["image", "plan"], token: object, owner: object) -> None:
        unwrap(self._native.release_handle(kind, token))
    def statistics(self, owner: object) -> object:
        return unwrap(self._native.statistics())
    def release(self, edit: object, owner: object) -> None:
        unwrap(self._native.release_edit(edit))
    def commit(self, edit: object, owner: object) -> object:
        return unwrap(self._native.commit(edit))
    def save(self, session: object, path: Path, owner: object) -> object:
        return unwrap(self._native.save(session, str(path)))
    def open(self, path: Path, owner: object) -> object:
        return unwrap(self._native.open(str(path)))
    def history(self, session: object, redo: bool, owner: object) -> object:
        return unwrap(self._native.history(session, redo))
    def cancel_job(self, job: object, owner: object) -> None:
        unwrap(self._native.cancel_job(job))
    def job_state(self, job: object, owner: object) -> object:
        return unwrap(self._native.job_state(job))
    def _test_control(self, action: str) -> object:
        return unwrap(self._native._test_control(action))
    def wait(self, job: object, kind: JobKind, owner: object) -> object:
        return unwrap(self._native.wait(job))
    def pending(self, owner: object) -> PendingRequest | None:
        return None
    def recover(self, owner: object) -> object:
        raise SdkFailure("NO_PENDING_REQUEST", "Local calls have no transport replay window")
    def close(self) -> None:
        unwrap(self._native.close())
