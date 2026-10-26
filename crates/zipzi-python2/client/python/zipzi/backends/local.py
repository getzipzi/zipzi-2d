"""Direct mechanical PyO3 conversion; local authoring does not serialize JSON."""
from .._generated import modeling
from .._generated.resource_backends import LocalResources
from importlib import import_module
from pathlib import Path
from typing import TYPE_CHECKING, Literal, cast
import time
from .. import _operations as ops, sampling, scene
from .._generated.backends import LocalQueries
from .._generated.planning import ModelingPlanOperation
from .._errors import JobTimeout, OutcomeUnknown, PendingRequest, SdkFailure, unwrap
from .._values import ConnectionInfo, SessionId, Source
from . import JobKind
from ..events import Cursor
if TYPE_CHECKING:
    from ._native_types import NativeModule, SessionToken

class LocalBackend(LocalQueries, LocalResources):
    def buffer_create(self, length: int, owner: object) -> object:
        return unwrap(self._native.buffer_create(length))
    def buffer_append(self, blob: object, offset: int, data: bytes, owner: object) -> object:
        return unwrap(self._native.buffer_append(blob, offset, data))
    def buffer_seal(self, blob: object, owner: object) -> object:
        return unwrap(self._native.buffer_seal(blob))
    def buffer_read(self, blob: object, offset: int, length: int, owner: object) -> object:
        return unwrap(self._native.buffer_read(blob, offset, length))
    def prepare_image_buffer(self, width: int, height: int, blob: object, owner: object) -> object:
        return unwrap(self._native.prepare_image_buffer(width, height, blob))
    def object_ref(self, session: object, kind: str, id: str, owner: object) -> object:
        return unwrap(self._native.object_ref(session, kind, id))
    def resolve_ref(self, reference: object, owner: object) -> object:
        return unwrap(self._native.resolve_ref(reference))
    def ref_capabilities(self, reference: object, owner: object) -> object:
        return unwrap(self._native.ref_capabilities(reference))
    def label_address(self, reference: object, owner: object) -> object:
        return unwrap(self._native.label_address(reference))
    def label_plan(self, address: object, name: str, owner: object) -> object:
        return unwrap(self._native.label_plan(address, name))
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
        if self._native.contract != "zipzi-python2-13":
            self._native.close()
            raise SdkFailure("CONTRACT_MISMATCH", "Native contract differs")

    def create(self, document_id: str, owner: object) -> object:
        return unwrap(self._native.create_session(document_id))
    def bindings(self, session: object, edit: object | None, ids: tuple[str, ...] | None, owner: object) -> object:
        return unwrap(self._native.bindings(session, edit, ids))
    def capture_address(self, session: object, selector: modeling.AddressSelector, owner: object) -> object:
        return unwrap(self._native.capture_address(session, selector))
    def address_plan(self, address: object, patch: modeling.AddressPatch, owner: object) -> object:
        return unwrap(self._native.address_plan(address, patch))
    def modeling_capabilities(self, reference: object, domain: modeling.CapabilityDomain, owner: object) -> object:
        return unwrap(self._native.modeling_capabilities(reference, domain))
    def collection_capabilities(self, session: object, query: modeling.CollectionQuery, owner: object) -> object:
        return unwrap(self._native.collection_capabilities(session, query))
    def sample_job(self, session: object, edit: object | None, poses: sampling.Inputs, checks: sampling.Checks, expected: Source | None, owner: object) -> object:
        return unwrap(self._native.sample_job(session, edit, poses, checks, expected))
    def modeling_plan_job(self, session: object, edit: object | None, intent: ModelingPlanOperation, owner: object) -> object:
        return unwrap(self._native.modeling_plan_job(session, edit, intent))
    def modeling_records(self, session: object, edit: object | None, query: modeling.ModelingQuery, owner: object) -> object:
        return unwrap(self._native.modeling_records(session, edit, query))
    def modeling_plan(self, session: object, edit: object | None, intent: ModelingPlanOperation, owner: object) -> object:
        return unwrap(self._native.modeling_plan(session, edit, intent))
    def scene(self, session: object, edit: object | None, ids: tuple[str, ...], owner: object) -> object:
        return unwrap(self._native.scene(session, edit, ids))
    def scene_plan(self, session: object, edit: object | None, operation: scene.Operation, owner: object) -> object:
        return unwrap(self._native.scene_plan(session, edit, operation))
    def sample(self, session: object, edit: object | None, poses: sampling.Inputs, checks: sampling.Checks, expected: Source | None, owner: object) -> object:
        return unwrap(self._native.sample(session, edit, poses, checks, expected))
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
    def apply(self, edit: object, operations: tuple[ops.WireOperation, ...], owner: object) -> object:
        return unwrap(self._native.apply(edit, operations))
    def prepare_image(self, width: int, height: int, rgba: bytes, owner: object) -> object:
        return unwrap(self._native.prepare_image(width, height, rgba))
    def release_handle(self, kind: Literal["image", "plan", "reference", "address", "buffer"], token: object, owner: object) -> None:
        unwrap(self._native.release_handle(kind, token))
    def statistics(self, owner: object) -> object:
        return unwrap(self._native.statistics())
    def release(self, edit: object, owner: object) -> None:
        unwrap(self._native.release_edit(edit))
    def commit(self, edit: object, owner: object) -> object:
        return unwrap(self._native.commit(edit))
    def prepare_image_job(self,path: str | None,blob:object | None,owner:object)->object:
        return unwrap(self._native.prepare_image_job(path,blob))
    def capture_image(self,session:object,edit:object | None,id:str,owner:object)->object:
        return unwrap(self._native.capture_image(session,edit,id))
    def editor_context(self,session:object,owner:object)->object:
        return unwrap(self._native.editor_context(session))
    def project(self, session: object | None, request: modeling.ProjectRequest, owner: object) -> object:
        return unwrap(self._native.project(session,request))
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
    def wait(self, job: object, kind: JobKind, owner: object, *, timeout: float | None = None) -> object:
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
    def close(self, *, abandon_pending: bool = False) -> None:
        unwrap(self._native.close())
