"""Handwritten ownership/known-state wrappers. Runtime remains authoritative."""
from collections.abc import Callable, Mapping, Sequence
from pathlib import Path
from typing import Generic, Literal, TypeVar
from . import bindings, sampling, events, meshes, ops, parameters, plans, resources, scene, references, modeling
from ._errors import Busy, JobTimeout, OutcomeUnknown, SdkFailure
from ._values import ConnectionInfo, Receipt, SaveReceipt, Source, SessionId, SessionInfo, WriteResult, record, sequence
from .backends import Backend, JobKind
from ._recovery import RecoveryOwner
from ._calls import Call, known, project, single_id
from enum import Enum
import math
import time
from ._generated.conveniences import EditableMeshes as MeshConveniences
from ._generated.resource_conveniences import EditableImages as ImageConveniences

T = TypeVar("T")
U = TypeVar("U")

def absolute(path: Path) -> Path:
    if not path.is_absolute():
        raise ValueError("Path must be absolute on the execution host")
    return path

JobState = Literal["queued", "running", "cancelling", "succeeded", "failed", "cancelled"]

class Job(RecoveryOwner, Generic[T]):
    def __init__(self, backend: Backend, token: object, kind: JobKind, decode: Callable[[object], T], on_terminal: Callable[[bool | None], None] | None = None):
        super().__init__(backend)
        self._token, self._kind, self._decode = token, kind, decode
        self._kind: JobKind
        self._result: tuple[T] | None = None
        self._failure: SdkFailure | None = None
        self._on_terminal = on_terminal
        self._terminal_failure = False
        self._deadline: float | None = None

    @classmethod
    def start(cls, backend: Backend, begin: Callable[[object], object], kind: JobKind,
        decode: Callable[[object], T]) -> "Job[T]":
        job = cls(backend, None, kind, decode)
        def start() -> "Job[T]":
            job._token = begin(job)
            return job
        def resume() -> "Job[T]":
            job._token = record(backend.recover(job))["job"]
            return job
        return known(start, resume)

    def _request_unknown(self, action: str, error: OutcomeUnknown) -> None:
        super()._request_unknown(action, error)
        if action == "wait" and self._on_terminal is not None:
            self._on_terminal(None)

    def _request_failed(self, action: str, error: BaseException) -> None:
        if action == "wait" and isinstance(error, SdkFailure) and not isinstance(error, (Busy, JobTimeout)):
            self._failure = error
            self._terminal_failure = error.code not in {"REQUEST_EXPIRED", "CLIENT_EXPIRED", "HOST_RESTARTED",
                "HOST_CLOSED", "HOST_PANICKED", "INVALID_REFERENCE", "NO_PENDING_REQUEST", "RESYNC_REQUIRED"}
            if self._on_terminal is not None:
                self._on_terminal(False if self._terminal_failure else None)

    def _remaining(self) -> float | None:
        return None if self._deadline is None else max(0., self._deadline - time.monotonic())

    def _wait_value(self, raw: object) -> T:
        # A recovered sampled transfer can be a poll taken before completion.
        # Its empty result is not a sample; continue polling the accepted job.
        if raw is None and self._kind == "sampled":
            raw = self._backend.wait(self._token, self._kind, self, timeout=self._remaining())
        if "result" in record(raw):
            from ._errors import unwrap
            info = record(raw)
            if info["result"] is None:
                return self._wait_value(self._backend.wait(self._token, self._kind, self, timeout=self._remaining()))
            try:
                raw = unwrap(info["result"])
            except SdkFailure as e:
                self._failure = e
                raise
        result = self._decode(raw)
        self._result = (result,)
        if self._on_terminal is not None:
            self._on_terminal(True)
        return result

    def wait(self, *, timeout: float | None = None) -> T:
        """Wait for and cache the actual terminal result, releasing its slot.

        None waits indefinitely. A finite nonnegative timeout raises JobTimeout
        while preserving the accepted Job. Transport loss raises OutcomeUnknown
        for the complete original wait, which can be recovered without replay.
        A pending cancel/status request must be recovered before waiting.
        """
        if timeout is not None and (isinstance(timeout, bool) or not isinstance(timeout, (int, float))
                or not math.isfinite(timeout) or timeout < 0):
            raise ValueError("timeout must be None or a finite nonnegative number")
        if self._failure is not None:
            raise self._failure
        if self._result is not None:
            return self._result[0]
        if self.pending_request is None:
            self._deadline = None if timeout is None else time.monotonic() + timeout
        return self._request("wait", lambda: self._backend.wait(self._token, self._kind, self,
                            timeout=self._remaining()), self._wait_value)

    def cancel(self) -> None:
        """Request cancellation. Wait still reports the actual entered-IO result."""
        if self._failure is not None and not self._terminal_failure:
            raise self._failure
        if self._result is None and self._failure is None:
            self._request("cancel", lambda: self._backend.cancel_job(self._token, self), lambda _: None)

    def state(self) -> JobState:
        if self._failure is not None:
            if not self._terminal_failure:
                raise self._failure
            return "cancelled" if self._failure.code == "CANCELLED" else "failed"
        if self._result is not None:
            return "succeeded"
        def decode(raw: object) -> JobState:
            if raw not in ("queued", "running", "cancelling", "succeeded", "failed", "cancelled"):
                raise TypeError("Invalid job state")
            return raw
        return self._request("state", lambda: self._backend.job_state(self._token, self), decode)

    def close(self) -> None:
        """Wait for the actual outcome and release its Runtime result slot."""
        project(self.wait, lambda _: None, operation="job.close")

    def __enter__(self) -> "Job[T]":
        return self

    def __exit__(self, kind: object, exception: BaseException | None, traceback: object) -> None:
        try:
            self.close()
        except BaseException as cleanup:
            if exception is None:
                raise
            exception.add_note(f"Job cleanup failed: {cleanup}")

class Runtime:
    def __init__(self, backend: Backend):
        self._backend = backend
        self.sessions = Sessions(self)
        self.resources = resources.Resources(backend)
    @property
    def connection(self) -> ConnectionInfo | None:
        """Immutable negotiated host metadata; local runtimes return None."""
        return self._backend.connection_info
    def __enter__(self) -> "Runtime":
        return self
    def __exit__(self, kind: object, exception: BaseException | None, traceback: object) -> None:
        try:
            self.close()
        except BaseException as cleanup:
            if exception is None:
                raise
            exception.add_note(f"Runtime cleanup failed: {cleanup}")
    def close(self, *, abandon_pending: bool = False) -> None:
        """Release this client/Local Runtime; pending recovery defaults to Busy.

        Explicit abandon_pending disconnects and forfeits pending recovery.
        It never changes identity to replay an unknown write. Connected sessions
        remain resident; entered host Jobs retain their actual outcome.
        """
        def resume() -> None:
            if self._backend.pending(self._backend) is None:
                raise SdkFailure("NO_PENDING_REQUEST", "Runtime close has already been resolved")
            self._backend.close(abandon_pending=abandon_pending)
        known(lambda: self._backend.close(abandon_pending=abandon_pending), resume)
    def events_since(self, cursor: events.Cursor, *, limit: int = 128) -> tuple[events.Event, ...]:
        events.check_limit(limit)
        return known(lambda: events.batch(self._backend.events(cursor, limit, self), cursor),
            lambda: events.batch(self._backend.recover(self), cursor))

class Sessions:
    def __init__(self, runtime: Runtime):
        self._runtime = runtime
    def subscribe(self) -> events.RegistrySubscription:
        return events.RegistrySubscription(self._runtime._backend)._start()
    def create(self, document_id: str) -> "Session":
        backend = self._runtime._backend
        return known(lambda: Session(self._runtime, backend.create(document_id, self)),
            lambda: Session(self._runtime, backend.recover(self)))
    def list(self) -> tuple[SessionInfo, ...]:
        backend = self._runtime._backend
        def decode(raw: object) -> tuple[SessionInfo, ...]:
            return tuple(SessionInfo.read(item) for item in sequence(raw))
        return known(lambda: decode(backend.list_sessions(self)), lambda: decode(backend.recover(self)))
    def attach(self, id: SessionId) -> "Session":
        backend = self._runtime._backend
        if not isinstance(id, SessionId):
            raise TypeError("Expected a SessionId")
        def decode(raw: object) -> "Session":
            return Session(self._runtime, record(raw)["id"])
        return known(lambda: decode(backend.attach(id, self)), lambda: decode(backend.recover(self)))
    def open(self, path: Path) -> Job["Session"]:
        backend = self._runtime._backend
        path = absolute(path)
        return known(lambda: self._opened(backend.open(path, self)), lambda: self._opened(backend.recover(self)))
    def import_(self,path:Path,format:modeling.ImportFormat,*,textures:tuple[modeling.TextureSlot,...]=())->Job["Session"]:
        request=modeling.ProjectRequestImport(path=str(absolute(path)),format=format,textures=textures)
        return Job[Session].start(self._runtime._backend,lambda owner:self._runtime._backend.project(None,request,owner),"project",lambda raw:Session(self._runtime,record(raw)["session"]))
    def _opened(self, value: object) -> Job["Session"]:
        backend = self._runtime._backend
        raw = record(value)
        job = Job(backend, raw.get("job"), "opened", lambda v: Session(self._runtime, record(v)["session"]))
        if "session" in raw:
            job._result = (Session(self._runtime, raw["session"]),)
        return job

class Session:
    def __init__(self, runtime: Runtime, token: object):
        self._runtime, self._token, self._backend = runtime, token, runtime._backend
        self._id = self._backend.session_id(token)
        self.meshes = Meshes(self)
        self.project = Project(self)
        self.resources = Images(self)
        self.parameters = Parameters(self)
        self.bindings = Bindings(self)
        self.modeling = Modeling(self)
        self.scene = Scene(self)
    @property
    def id(self) -> SessionId:
        return self._id
    def address(self, selector: modeling.AddressSelector) -> references.FieldAddress:
        return references.FieldAddress.create(self._backend, self._token, selector)

    def capabilities(self, query: modeling.CollectionQuery) -> modeling.CapabilitySnapshot:
        modeling.validate_CollectionQuery(query)
        return known(lambda: modeling.CapabilitySnapshot.read(self._backend.collection_capabilities(self._token, query, self)),
            lambda: modeling.CapabilitySnapshot.read(self._backend.recover(self)))

    def reference(self, kind: references.ObjectKind, id: str) -> references.ObjectRef:
        return references.ObjectRef.create(self._backend, self._token, kind, id)
    def info(self) -> SessionInfo:
        return known(lambda: SessionInfo.read(self._backend.info(self._token, self)),
            lambda: SessionInfo.read(self._backend.recover(self)))
    def editor_context(self)->modeling.EditorSnapshot:
        return known(lambda:modeling.EditorSnapshot.read(self._backend.editor_context(self._token,self)),lambda:modeling.EditorSnapshot.read(self._backend.recover(self)))
    def show_window(self) -> None:
        """Display this resident session in its desktop host."""
        known(lambda: self._backend.show_window(self._token, self), lambda: self._backend.recover(self))
    def subscribe(self) -> events.SessionSubscription:
        return events.SessionSubscription(self._backend, self._token, self.id)._start()
    def edit(self, label: str) -> "Edit":
        return Edit(self, label)
    def undo(self) -> Job[Receipt]:
        return Job[Receipt].start(self._backend, lambda owner: self._backend.history(self._token, False, owner), "published", Receipt.read)
    def redo(self) -> Job[Receipt]:
        return Job[Receipt].start(self._backend, lambda owner: self._backend.history(self._token, True, owner), "published", Receipt.read)

class Project:
    def __init__(self, session: Session):
        self._session = session
    def run(self, request:modeling.ProjectRequest)->Job[modeling.ProjectReport]:
        modeling.validate_ProjectRequest(request)
        s=self._session
        return Job[modeling.ProjectReport].start(s._backend,lambda owner:s._backend.project(s._token,request,owner),"project",lambda raw:modeling.read_ProjectReport(record(raw)["report"]))
    def diagnose(self)->Job[modeling.ProjectReport]:
        return self.run(modeling.ProjectRequestDiagnose())
    def replace(self,path:Path,format:modeling.ImportFormat,*,textures:tuple[modeling.TextureSlot,...]=())->Job[modeling.ProjectReport]:
        return self.run(modeling.ProjectRequestImport(path=str(absolute(path)),format=format,textures=textures))
    def open(self,path:Path)->Job[modeling.ProjectReport]:
        return self.run(modeling.ProjectRequestOpen(path=str(absolute(path))))
    def export(self,path:Path,format:modeling.ExportFormat,*,name:str="model",object_id:str|None=None,allow_loss:bool=False,bake_animation:bool=False,auto_frame_rate:bool=False)->Job[modeling.ProjectReport]:
        return self.run(modeling.ProjectRequestExport(path=str(absolute(path)),format=format,name=name,object_id=object_id,allow_loss=allow_loss,bake_animation=bake_animation,auto_frame_rate=auto_frame_rate))
    def save(self, path: Path) -> Job[SaveReceipt]:
        s = self._session
        path = absolute(path)
        return Job[SaveReceipt].start(s._backend, lambda owner: s._backend.save(s._token, path, owner), "saved", SaveReceipt.read)

class Meshes:
    def __init__(self, session: Session, edit: "Edit | None" = None):
        self._session, self._edit = session, edit
    def get(self,id:str)->meshes.Snapshot:
        s=self._session
        return known(lambda:meshes.Snapshot.read(s._backend.mesh(s._token,id,None if self._edit is None else self._edit._token,self)),lambda:meshes.Snapshot.read(s._backend.recover(self)))
    def get_many(self,ids:Sequence[str])->tuple[meshes.Snapshot,...]:
        owned=tuple(ids)
        if any(not isinstance(id,str) for id in owned):raise TypeError("Expected mesh ID text")
        s=self._session
        def decode(raw:object)->tuple[meshes.Snapshot,...]:return tuple(meshes.Snapshot.read(v) for v in sequence(raw))
        return known(lambda:decode(s._backend.meshes(s._token,None if self._edit is None else self._edit._token,owned,self)),lambda:decode(s._backend.recover(self)))

    def plan(self, intent: meshes.RenameIntent) -> plans.Plan:
        s = self._session
        return plans.Plan.create(s._backend, s._token, None if self._edit is None else self._edit._token, intent)

class Images:
    def snapshot(self,id:str)->resources.ImageLease:
        s=self._session
        return resources.ImageLease.capture(s._backend,s._token,None if self._edit is None else self._edit._token,id)
    def __init__(self, session: Session, edit: "Edit | None" = None):
        self._session, self._edit = session, edit
    def get(self, id: str) -> resources.Snapshot:
        s = self._session
        return known(lambda: resources.Snapshot.read(s._backend.image(s._token, id, None if self._edit is None else self._edit._token, self)),
            lambda: resources.Snapshot.read(s._backend.recover(self)))
    def plan(self, intent: resources.RenameIntent) -> plans.Plan:
        s = self._session
        return plans.Plan.create(s._backend, s._token, None if self._edit is None else self._edit._token, intent)

class EditableImages(Images, ImageConveniences):
    def __init__(self, session: Session, edit: "Edit"):
        super().__init__(session, edit)
        self._owner = edit

class EditableMeshes(Meshes, MeshConveniences):
    def __init__(self, session: Session, edit: "Edit"):
        super().__init__(session, edit)
        self._owner = edit

class Parameters:
    def __init__(self, session: Session, edit: "Edit | None" = None):
        self._session, self._edit = session, edit
    def _read(self, ids: tuple[str, ...] | None) -> parameters.Batch:
        s = self._session
        return known(lambda: parameters.Batch.read(s._backend.parameters(s._token,
            None if self._edit is None else self._edit._token, ids, self)),
            lambda: parameters.Batch.read(s._backend.recover(self)))
    def get(self, id: str) -> parameters.Snapshot:
        s = self._session
        def decode(raw: object) -> parameters.Snapshot:
            return parameters.Batch.read(raw).items[0]
        return known(lambda: decode(s._backend.parameters(s._token,
            None if self._edit is None else self._edit._token, (id,), self)), lambda: decode(s._backend.recover(self)))
    def get_many(self, ids: Sequence[str]) -> parameters.Batch:
        owned = tuple(ids)
        if any(not isinstance(id, str) for id in owned):
            raise TypeError("Expected parameter ID text")
        return self._read(owned)
    def list(self) -> parameters.Batch:
        return self._read(None)

class EditableParameters(Parameters):
    def __init__(self, session: Session, edit: "Edit"):
        super().__init__(session, edit)
        self._owner = edit
    def create(self, spec: parameters.ParameterSpec) -> str:
        return project(lambda: self._owner._construct_and_apply(lambda: ops.CreateParameter(spec)),
                       single_id, operation="parameters.create", inputs=(spec,))
    def patch(self, id: str, patch: parameters.Patch) -> WriteResult:
        return self._owner._construct_and_apply(lambda: ops.PatchParameter(id, patch))

class Bindings:
    def __init__(self, session: Session, edit: "Edit | None" = None):
        self._session, self._edit = session, edit
    def _read(self, ids: tuple[str, ...] | None) -> bindings.Batch:
        s = self._session
        return known(lambda: bindings.Batch.read(s._backend.bindings(s._token,
            None if self._edit is None else self._edit._token, ids, self)), lambda: bindings.Batch.read(s._backend.recover(self)))
    def get(self, id: str) -> bindings.Snapshot:
        s = self._session
        def decode(raw: object) -> bindings.Snapshot:
            return bindings.Batch.read(raw).items[0]
        return known(lambda: decode(s._backend.bindings(s._token,
            None if self._edit is None else self._edit._token, (id,), self)), lambda: decode(s._backend.recover(self)))
    def get_many(self, ids: Sequence[str]) -> bindings.Batch:
        owned = tuple(ids)
        if any(not isinstance(id, str) for id in owned):
            raise TypeError("Expected binding ID text")
        return self._read(owned)
    def list(self) -> bindings.Batch:
        return self._read(None)

class EditableBindings(Bindings):
    def __init__(self, session: Session, edit: "Edit"):
        super().__init__(session, edit)
        self._owner = edit
    def create(self, spec: bindings.MeshBindingSpec) -> str:
        return project(lambda: self._owner._construct_and_apply(lambda: ops.CreateMeshBinding(spec)),
                       single_id, operation="bindings.create", inputs=(spec,))
    def set_keyform(self, id: str, keyform: bindings.Keyform) -> WriteResult:
        return self._owner._construct_and_apply(lambda: ops.SetMeshKeyform(id, keyform))

class Scene:
    def __init__(self, session: Session, edit: "Edit | None" = None):
        self._session, self._edit = session, edit

    def get_many(self, ids: Sequence[str]) -> scene.Batch:
        owned = tuple(ids)
        if any(not isinstance(id, str) for id in owned):
            raise TypeError("Expected scene IDs")
        s = self._session
        return known(lambda: scene.Batch.read(s._backend.scene(s._token,
            None if self._edit is None else self._edit._token, owned, self)),
            lambda: scene.Batch.read(s._backend.recover(self)))

    def get(self, id: str) -> scene.Snapshot:
        return project(lambda: self.get_many((id,)), lambda batch: scene.Snapshot(batch.source, batch.items[0]),
                       operation="scene.get", inputs=(id,))

    def plan(self, operation: scene.Operation) -> plans.Plan:
        s = self._session
        return plans.Plan.create_scene(s._backend, s._token,
            None if self._edit is None else self._edit._token, operation)

class Modeling:
    def __init__(self, session: Session, edit: "Edit | None" = None):
        self._session, self._edit = session, edit
    def records(self, query: modeling.ModelingQuery) -> modeling.Records:
        modeling.validate_ModelingQuery(query)
        s = self._session
        return known(lambda: modeling.Records.read(s._backend.modeling_records(s._token,
            None if self._edit is None else self._edit._token, query, self)),
            lambda: modeling.Records.read(s._backend.recover(self)))

    def plan(self, intent: modeling.ModelingIntent) -> plans.Plan:
        s = self._session
        return plans.Plan.create_modeling(s._backend, s._token,
            None if self._edit is None else self._edit._token, intent)

    def plan_async(self, intent: modeling.ModelingIntent) -> Job[plans.Plan]:
        modeling.validate_ModelingIntent(intent)
        s = self._session
        return Job[plans.Plan].start(s._backend, lambda owner: s._backend.modeling_plan_job(s._token,
            None if self._edit is None else self._edit._token, intent, owner), "planned",
            lambda value: plans.Plan(s._backend)._load(value))

    def sample_async(self, poses: Sequence[Mapping[str, float]], *, checks: sampling.Checks = sampling.Checks(),
        expected: Source | None = None) -> Job[sampling.Batch]:
        owned = sampling.inputs(poses)
        if not isinstance(checks, sampling.Checks) or (expected is not None and not isinstance(expected, Source)):
            raise TypeError("Expected Checks and optional Source")
        checks.wire()
        s = self._session
        return Job[sampling.Batch].start(s._backend, lambda owner: s._backend.sample_job(s._token,
            None if self._edit is None else self._edit._token, owned, checks, expected, owner), "sampled", sampling.Batch.read)

    def sample(self, poses: Sequence[Mapping[str, float]], *, checks: sampling.Checks = sampling.Checks(),
        expected: Source | None = None) -> sampling.Batch:
        owned = sampling.inputs(poses)
        if not isinstance(checks, sampling.Checks) or (expected is not None and not isinstance(expected, Source)):
            raise TypeError("Expected Checks and optional Source")
        checks.wire()
        s = self._session
        return known(lambda: sampling.Batch.read(s._backend.sample(s._token,
            None if self._edit is None else self._edit._token, owned, checks, expected, self)),
            lambda: sampling.Batch.read(s._backend.recover(self)))

class EditState(str, Enum):
    editing = "editing"
    committing = "committing"
    committed = "committed"
    cancelled = "cancelled"
    aborted = "aborted"
    failed = "failed"
    unknown = "unknown"


class Edit(RecoveryOwner):
    def __init__(self, session: Session, label: str):
        super().__init__(session._backend)
        self._session = session
        self._token: object = None
        self._state = EditState.editing
        self._receipt: Receipt | None = None
        self._unknown: OutcomeUnknown | None = None
        self._failure: BaseException | None = None
        self._released = False
        self._job: Job[Receipt] | None = None
        self.meshes = EditableMeshes(session, self)
        self.resources = EditableImages(session, self)
        self.parameters = EditableParameters(session, self)
        self.bindings = EditableBindings(session, self)
        self.modeling = Modeling(session, self)
        self.scene = Scene(session, self)
        def begun(raw: object) -> "Edit":
            self._token, self._state = raw, EditState.editing
            self._unknown = None
            return self
        self._run("begin", lambda: self._backend.begin(session._token, label, self), begun, (label,))

    @property
    def state(self) -> EditState:
        """Cached confirmed or unknown client state; no request is sent."""
        return self._state

    @property
    def receipt(self) -> Receipt | None:
        """Cached confirmed commit receipt, or None before success/after cancel."""
        return self._receipt

    def _run(self, action: str, execute: Callable[[], object], decode: Callable[[object], T],
             inputs: tuple[object, ...] = ()) -> T:
        def unknown(error: OutcomeUnknown) -> None:
            self._unknown = error
            if self._state != EditState.aborted:
                self._state = EditState.unknown
        request = Call("edit." + action, execute, decode, resume=lambda: self._backend.recover(self),
                       inputs=inputs, on_unknown=unknown, on_error=lambda error: self._write_error(action, error))
        self._call = request
        return request.run()

    def _write_error(self, action: str, error: BaseException) -> None:
        self._unknown = None
        if isinstance(error, Busy) or (isinstance(error, SdkFailure)
                and error.code in {"BUDGET_EXCEEDED", "RESOURCE_BUDGET_EXCEEDED"} and error.edit_state == "editing"):
            self._state = EditState.editing
            return
        if action == "release" or (isinstance(error, SdkFailure) and error.code in {
                "REQUEST_EXPIRED", "CLIENT_EXPIRED", "HOST_RESTARTED", "HOST_CLOSED", "HOST_PANICKED", "NO_PENDING_REQUEST"}):
            if self._state != EditState.aborted:
                self._state = EditState.failed
            self._failure = error
            return
        if action in {"apply", "commit"}:
            self._poison(error)

    def __enter__(self) -> "Edit":
        self._ensure_editing()
        return self

    def _ensure_editing(self) -> None:
        if self._state != EditState.editing:
            raise SdkFailure("EDIT_ABORTED" if self._state == EditState.aborted else
                "OUTCOME_UNKNOWN" if self._state == EditState.unknown else "EDIT_CLOSED",
                f"Edit is {self._state.value}", edit_state=self._state.value)

    def _poison(self, original: BaseException) -> None:
        self._state, self._failure = EditState.aborted, original
        if isinstance(original, SdkFailure):
            original.edit_state, original.recovery_action = "aborted", "restart_edit"
        try:
            self._release()
        except BaseException as cleanup:
            if isinstance(original, SdkFailure) and isinstance(cleanup, OutcomeUnknown):
                original.cleanup = cleanup
            else:
                original.__context__ = cleanup
            original.add_note(f"Candidate cleanup failed: {cleanup}")

    def _release(self) -> None:
        if self._released:
            return
        def released(_: object) -> None:
            self._released, self._unknown = True, None
            if self._state != EditState.aborted:
                self._state = EditState.cancelled
        self._run("release", lambda: self._backend.release(self._token, self), released)

    def apply(self, operation: ops.Operation) -> WriteResult:
        return self.apply_many((operation,))

    def _construct_and_apply(self, construct: Callable[[], ops.Operation]) -> WriteResult:
        self._ensure_editing()
        try:
            operation = construct()
        except Exception as error:
            self._poison(error)
            raise
        return self.apply(operation)

    def apply_plan(self, plan: plans.Plan) -> WriteResult:
        return self.apply(ops.ApplyPlan(plan))

    def apply_many(self, operations: Sequence[ops.Operation]) -> WriteResult:
        self._ensure_editing()
        try:
            owned = tuple(operations)
        except (TypeError, ValueError, OverflowError, AttributeError) as error:
            self._poison(error)
            raise
        def execute() -> object:
            if any(not isinstance(op, (ops.Create, ops.Rename, ops.SetPositions, ops.CreateImage, ops.ReplaceImage, ops.ApplyPlan, ops.CreateParameter, ops.PatchParameter, ops.CreateMeshBinding, ops.SetMeshKeyform, *ops.scene.OPERATION_TYPES, *modeling.OPERATION_TYPES)) for op in owned):
                raise TypeError("Expected typed authoring operations")
            for op in owned:
                ops.validate(op)
                if isinstance(op, (ops.CreateImage, ops.ReplaceImage)):
                    op.image._check(self._backend)
                elif isinstance(op, ops.ApplyPlan):
                    op.plan._check(self._backend)
            return self._backend.apply(self._token, owned, self)
        def applied(raw: object) -> WriteResult:
            value = WriteResult.read(raw)
            self._state, self._unknown = EditState.editing, None
            return value
        return self._run("apply", execute, applied, owned)

    def _finished(self, success: bool | None) -> None:
        if success is None:
            if self._job is not None and self._job._failure is not None:
                self._state, self._failure = EditState.failed, self._job._failure
                self._unknown = None
            else:
                self._state = EditState.unknown
                self._unknown = None if self._job is None else self._job._unknown_error
        else:
            self._state = EditState.committed if success else EditState.failed
            self._unknown = None
            if success and self._job is not None and self._job._result is not None:
                self._receipt = self._job._result[0]
            elif self._job is not None:
                self._failure = self._job._failure

    def commit(self) -> Job[Receipt]:
        self._ensure_editing()
        def accepted(raw: object) -> Job[Receipt]:
            self._job = Job(self._backend, record(raw)["job"], "published", Receipt.read, self._finished)
            self._state, self._unknown = EditState.committing, None
            return self._job
        return self._run("commit", lambda: dict(job=self._backend.commit(self._token, self)), accepted)

    def cancel(self) -> None:
        if self._state == EditState.unknown or self.pending_request is not None:
            raise Busy("BUSY", "Recover the original Edit call before cleanup", edit_state=self._state.value,
                       recovery_action="recover_original")
        if self._state in {EditState.cancelled, EditState.committed, EditState.failed}:
            return
        if self._state == EditState.committing:
            raise Busy("BUSY", "Accepted commit must complete before cleanup")
        self._release()

    def __exit__(self, kind: object, exception: BaseException | None, traceback: object) -> None:
        if exception is not None:
            if self._state == EditState.unknown or isinstance(exception, (OutcomeUnknown, JobTimeout)) or self.pending_request is not None:
                exception.add_note("Edit outcome/cleanup remains owned by the original call or accepted Job")
                return
            try:
                if self._state == EditState.committing and self._job is not None:
                    self._job.wait()
                else:
                    self.cancel()
            except BaseException as cleanup:
                exception.add_note(f"Candidate cleanup failed: {cleanup}")
            return
        if self._state == EditState.aborted:
            failure = SdkFailure("EDIT_ABORTED", "Caught failure made this candidate uncommittable", edit_state="aborted")
            if self._unknown is not None:
                failure.cleanup = self._unknown
                failure.add_note("Candidate cleanup retains a recoverable original request")
            raise failure
        if self._state == EditState.unknown and self._unknown is not None:
            raise self._unknown
        pending = self._backend.pending_error()
        if pending is not None:
            # A read or independent preparation has its own complete return
            # contract. Keep that owner and leave this candidate editable.
            raise pending
        if self._job is not None and self._job.pending_request is not None and self._job._unknown_error is not None:
            raise self._job._unknown_error
        if self._state == EditState.editing:
            self.commit()
        if self._state == EditState.committing and self._job is not None:
            self._job.wait()
        if self._state not in {EditState.committed, EditState.cancelled}:
            if self._failure is not None:
                raise self._failure
            raise SdkFailure("EDIT_CLOSED", "Edit did not reach a confirmed terminal state", edit_state=self._state.value)
