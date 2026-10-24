"""Direct mechanical PyO3 conversion; local authoring does not serialize JSON."""
from importlib import import_module
from pathlib import Path
from typing import TYPE_CHECKING, cast
from .. import ops
from .._errors import PendingRequest, SdkFailure, unwrap
from . import JobKind
if TYPE_CHECKING:
    from ._native_types import NativeModule

class LocalBackend:
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
        if self._native.contract != "zipzi-python2-1":
            self._native.close()
            raise SdkFailure("CONTRACT_MISMATCH", "Native contract differs")

    def create(self, document_id: str, owner: object) -> object:
        return unwrap(self._native.create_session(document_id))
    def info(self, session: object, owner: object) -> object:
        return unwrap(self._native.info(session))
    def begin(self, session: object, label: str, owner: object) -> object:
        return unwrap(self._native.begin_edit(session, label))
    def apply(self, edit: object, operations: tuple[ops.MeshOperation, ...], owner: object) -> object:
        return unwrap(self._native.apply(edit, operations))
    def mesh(self, session: object, id: str, edit: object | None, owner: object) -> object:
        return unwrap(self._native.mesh(session, id, edit))
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
    def wait(self, job: object, kind: JobKind, owner: object) -> object:
        return unwrap(self._native.wait(job))
    def pending(self, owner: object) -> PendingRequest | None:
        return None
    def recover(self, owner: object) -> object:
        raise SdkFailure("NO_PENDING_REQUEST", "Local calls have no transport replay window")
    def close(self) -> None:
        unwrap(self._native.close())
