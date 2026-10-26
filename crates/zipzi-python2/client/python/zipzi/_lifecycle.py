"""Handwritten ownership/known-state wrappers. Runtime remains authoritative."""
from ._generated import modeling as _modeling_codec
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import TypeVar, overload
from . import events, ops, _plans as plans, resources, references, modeling
from ._errors import Busy, JobTimeout, OutcomeUnknown, SdkFailure
from ._values import ConnectionInfo, Receipt, SaveReceipt, SessionId, SessionInfo, WriteResult, record, sequence
from .backends import Backend
from ._recovery import RecoveryOwner
from ._calls import Call, known
from . import _operations as operations
from ._generated.planning import ModelingPlanOperation, MODELING_PLAN_TYPES
from ._results import Snapshot
from . import _queries as queries
from enum import Enum

T = TypeVar("T")
U = TypeVar("U")

def absolute(path: Path) -> Path:
    if not path.is_absolute():
        raise ValueError("Path must be absolute on the execution host")
    return path

from ._jobs import Job, JobState as JobState

from ._generated.facades import Meshes, EditableMeshes, Images, EditableImages, Parameters, EditableParameters, Bindings, EditableBindings, Scene, Modeling
from ._generated.discovery import RuntimeCapabilities, RuntimeLimits

class _SessionBinding(RecoveryOwner):
    def __init__(self, backend: Backend, token: object):
        super().__init__(backend)
        self.token=token
        self.active=True
        self.closed: SessionInfo | None=None

class Runtime:
    'Explicit client lifetime. Construct with zipzi.local() or zipzi.connect(); Connected keeps formal sessions resident after client close.'
    def __init__(self, backend: Backend):
        self._backend = backend
        self._bindings: dict[SessionId, _SessionBinding] = {}
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
    def capabilities(self) -> RuntimeCapabilities:
        """Query actual implementation and environment capabilities; this does not reserve future admission."""
        return known(lambda: RuntimeCapabilities._read(self._backend.runtime_capabilities(self)),lambda: RuntimeCapabilities._read(self._backend.recover(self)))
    def limits(self) -> RuntimeLimits:
        """Query current Runtime count, byte and scan budgets. Values are estimates, not process RSS limits."""
        return known(lambda: RuntimeLimits._read(self._backend.runtime_limits(self)),lambda: RuntimeLimits._read(self._backend.recover(self)))
    def events_since(self, cursor: events.Cursor, *, limit: int = 128) -> tuple[events.Event, ...]:
        'Read a bounded event page after an exact registry/session Cursor. Limit is 1..256; Runtime identity or retained-history gaps raise RESYNC_REQUIRED. Recover a lost response to obtain its original page.'
        events.check_limit(limit)
        return known(lambda: events.batch(self._backend.events(cursor, limit, self), cursor),
            lambda: events.batch(self._backend.recover(self), cursor))

class Sessions:
    'Bounded host registry. Listing and attaching share the resident document; neither creates a Python mirror.'
    def __init__(self, runtime: Runtime):
        self._runtime = runtime
    def subscribe(self) -> events.RegistrySubscription:
        'Capture current registry metadata and an independent cursor. Poll bounded pages; resync explicitly after RESYNC_REQUIRED. This does not drain another subscriber.'
        return events.RegistrySubscription(self._runtime._backend)._start()
    def create(self, document_id: str | None = None) -> "Session":
        'Create a session with SDK defaults. None allocates a canonical document UUID in Rust. Returns the owned binding; a lost reply must recover this original call, never create another session.'
        backend = self._runtime._backend
        return known(lambda: Session(self._runtime, backend.create(document_id, self)),
            lambda: Session(self._runtime, backend.recover(self)))
    def list(self) -> tuple[SessionInfo, ...]:
        'Read one bounded tuple of resident SessionInfo values. Registry order uses SessionId. No object geometry or pixel payload is returned; transport loss recovers the captured tuple.'
        backend = self._runtime._backend
        def decode(raw: object) -> tuple[SessionInfo, ...]:
            return tuple(SessionInfo._read(item) for item in sequence(raw))
        return known(lambda: decode(backend.list_sessions(self)), lambda: decode(backend.recover(self)))
    def attach(self, id: SessionId) -> "Session":
        'Bind this client to an existing Runtime-scoped SessionId without copying content. Detached old wrappers stay invalid; a successful attach creates a fresh binding. Wrong Runtime or missing IDs fail.'
        backend = self._runtime._backend
        if not isinstance(id, SessionId):
            raise TypeError("Expected a SessionId")
        def decode(raw: object) -> "Session":
            return Session(self._runtime, record(raw)["id"])
        return known(lambda: decode(backend.attach(id, self)), lambda: decode(backend.recover(self)))
    def open(self, path: Path) -> Job["Session"]:
        'Submit host-absolute project open and return Job[Session], including already-open paths through a cached completed Job. Wait for the actual outcome; recover a lost start before waiting.'
        backend = self._runtime._backend
        path = absolute(path)
        return known(lambda: self._opened(backend.open(path, self)), lambda: self._opened(backend.recover(self)))
    def import_(self,path:Path,format:modeling.ImportFormat,*,textures:tuple[modeling.TextureSlot,...]=())->Job["Session"]:
        'Submit an explicit host format import into a new session. Textures are explicit host paths. Wait for Job[Session]; cancellation reports the real outcome if IO has already entered.'
        request=modeling.ProjectRequestImport(path=str(absolute(path)),format=format,textures=textures)
        return Job[Session]._start(self._runtime._backend,lambda owner:self._runtime._backend.project(None,request,owner),"project",lambda raw:Session(self._runtime,record(raw)["session"]))
    def _opened(self, value: object) -> Job["Session"]:
        backend = self._runtime._backend
        raw = record(value)
        job = Job(backend, raw.get("job"), "opened", lambda v: Session(self._runtime, record(v)["session"]))
        if "session" in raw:
            job._result = (Session(self._runtime, raw["session"]),)
        return job

class Session:
    'One client binding to a resident document. Use runtime.sessions factories; aliases share detach/close state. id is pure cached identity, while info queries host state.'
    def __init__(self, runtime: Runtime, token: object):
        self._runtime, self._backend = runtime, runtime._backend
        self._id = self._backend.session_id(token)
        binding=runtime._bindings.get(self._id)
        if binding is None or not binding.active:
            binding=_SessionBinding(self._backend,token)
            runtime._bindings[self._id]=binding
        self._binding=binding
        self.meshes = Meshes(self)
        self.project = Project(self)
        self.resources = Images(self)
        self.parameters = Parameters(self)
        self.bindings = Bindings(self)
        self.modeling = Modeling(self)
        self.scene = Scene(self)
    @property
    def id(self) -> SessionId:
        'Return cached Runtime-scoped identity without IO. Persistent object IDs belong to this session and do not substitute for a SessionId.'
        return self._id
    @property
    def _token(self) -> object:
        self._check_binding()
        return self._binding.token
    def _check_binding(self) -> None:
        if not self._binding.active:
            raise SdkFailure("INVALID_REFERENCE", "This Session binding was detached or closed; attach again for a new binding")
    def detach(self) -> None:
        """Release this client's binding and its references. Active work refuses detach; the host session remains resident. Aliases become invalid."""
        binding=self._binding
        if not binding.active:return
        def finish(_:object)->None:binding.active=False
        binding._request("detach",lambda:self._backend.detach_session(binding.token,binding),finish)
    def close(self, *, discard_modified: bool = False) -> SessionInfo:
        """Close the host session and release its path/slot. Unsaved changes and other clients refuse closure without changes. Never saves implicitly."""
        binding=self._binding
        if binding.closed is not None:return binding.closed
        self._check_binding()
        if type(discard_modified) is not bool:raise TypeError("Expected discard_modified bool")
        def finish(raw:object)->SessionInfo:
            info=SessionInfo._read(raw);binding.closed=info;binding.active=False;return info
        return binding._request("close",lambda:self._backend.close_session(binding.token,discard_modified,binding),finish)
    def plan(self, operation: ops.PlannableOperation) -> plans.Plan:
        'Plan a registered typed domain operation against the exact formal Source. Read cost/losses, apply through an Edit, then close the Plan. Planning failures do not abort another candidate.'
        return plans.Plan._create(self._backend, self._token, None, operation)

    def plan_async(self, operation: ModelingPlanOperation) -> Job[plans.Plan]:
        'Submit a registered expensive modeling plan at the captured Source. Returns Job[Plan]; wait before applying and close the resulting Plan. Input type determines this explicit Job API.'
        return _plan_job(self._backend, self._token, None, operation)

    def address(self, selector: modeling.AddressSelector) -> references.FieldAddress:
        'Capture a field address with incarnation, context and field epochs. The returned handle owns a lease and must be closed; recreated IDs cannot revive an old address.'
        return references.FieldAddress._create(self._backend, self._token, selector)

    def capabilities(self, query: modeling.CollectionQuery) -> Snapshot[modeling.Capabilities]:
        'Query selected collection capabilities at the captured formal Source. Results explain available/unavailable/unsupported states and do not reserve future admission.'
        _modeling_codec.validate_CollectionQuery(query)
        return known(lambda: queries.capabilities(self._backend.collection_capabilities(self._token, query, self)),
            lambda: queries.capabilities(self._backend.recover(self)))

    def reference(self, kind: references.ObjectKind, id: str) -> references.ObjectRef:
        'Capture an SDK object reference for the declared kind and canonical UUID. Runtime validates session ownership and incarnation. Close the returned reference after use.'
        return references.ObjectRef._create(self._backend, self._token, kind, id)
    def info(self) -> SessionInfo:
        'Read current version, modified state, history lengths, path and host state. Returns an owned value; a lost response recovers the original query.'
        return known(lambda: SessionInfo._read(self._backend.info(self._token, self)),
            lambda: SessionInfo._read(self._backend.recover(self)))
    def editor_context(self)->Snapshot[modeling.EditorContext | None]:
        'Read an owned optional Editor context and exact formal Source. Local or non-Editor hosts can return None; no Editor object is exposed.'
        return known(lambda:queries.editor_context(self._backend.editor_context(self._token,self)),lambda:queries.editor_context(self._backend.recover(self)))
    def show_window(self) -> None:
        """Display this resident session in its desktop host."""
        known(lambda: self._backend.show_window(self._token, self), lambda: self._backend.recover(self))
    def subscribe(self) -> events.SessionSubscription:
        'Capture SessionInfo and an independent exact event cursor. Poll explicit bounded pages and resync on gaps; subscribed metadata does not hold a model mirror.'
        return events.SessionSubscription(self._backend, self._token, self.id)._start()
    def edit(self, label: str) -> "Edit":
        'Begin an explicit candidate with a history label. Reads see candidate changes; normal context exit commits and waits. Confirmed write failures require a new Edit and unknown outcomes require original-call recovery.'
        return Edit(self, label)
    def undo(self) -> Job[Receipt]:
        'Submit one formal history undo and return Job[Receipt]. Wait for confirmed content/version/history; conflicting or busy sessions reject without silently changing the target.'
        return Job[Receipt]._start(self._backend, lambda owner: self._backend.history(self._token, False, owner), "published", Receipt._read)
    def redo(self) -> Job[Receipt]:
        'Submit one formal history redo and return Job[Receipt]. Wait for the actual receipt. Cancelling or a no-op Edit preserves redo according to SDK history rules.'
        return Job[Receipt]._start(self._backend, lambda owner: self._backend.history(self._token, True, owner), "published", Receipt._read)

class Project:
    'Host-side format and persistence operations. Every path is absolute on the selected execution host; IO returns explicit Jobs.'
    def __init__(self, session: Session):
        self._session = session
    def run(self, request:modeling.ProjectRequest)->Job[modeling.ProjectReport]:
        'Submit one typed ProjectRequest and return Job[ProjectReport]. Input selects explicit format/IO policy; losses and notices belong to the report. Wait and verify before claiming publication.'
        _modeling_codec.validate_ProjectRequest(request)
        s=self._session
        return Job[modeling.ProjectReport]._start(s._backend,lambda owner:s._backend.project(s._token,request,owner),"project",lambda raw:_modeling_codec.read_ProjectReport(record(raw)["report"]))
    def diagnose(self)->Job[modeling.ProjectReport]:
        'Submit SDK project diagnostics without publishing edited model content. Returns Job[ProjectReport] with owned diagnostics; wait releases the result slot.'
        return self.run(modeling.ProjectRequestDiagnose())
    def replace(self,path:Path,format:modeling.ImportFormat,*,textures:tuple[modeling.TextureSlot,...]=())->Job[modeling.ProjectReport]:
        'Submit explicit format import to replace this session. Textures are host-absolute slots. Wait for the real report; generation changes only after confirmed publication.'
        return self.run(modeling.ProjectRequestImport(path=str(absolute(path)),format=format,textures=textures))
    def open(self,path:Path)->Job[modeling.ProjectReport]:
        'Replace this session from an absolute host project path through an explicit Job. Successful replacement advances generation and invalidates old source-bound addresses/plans.'
        return self.run(modeling.ProjectRequestOpen(path=str(absolute(path))))
    def export(self,path:Path,format:modeling.ExportFormat,*,name:str="model",object_id:str|None=None,allow_loss:bool=False,bake_animation:bool=False,auto_frame_rate:bool=False)->Job[modeling.ProjectReport]:
        'Submit a selected export to a host-absolute path. allow_loss, animation baking and frame-rate choices are explicit. Read report losses/published and wait for the durable IO outcome.'
        return self.run(modeling.ProjectRequestExport(path=str(absolute(path)),format=format,name=name,object_id=object_id,allow_loss=allow_loss,bake_animation=bake_animation,auto_frame_rate=auto_frame_rate))
    def save(self, path: Path) -> Job[SaveReceipt]:
        'Submit saving to a host-absolute project folder. Returns Job[SaveReceipt]; wait checks durable and before/after versions. A client-side Path.exists is not evidence of host persistence.'
        s = self._session
        path = absolute(path)
        return Job[SaveReceipt]._start(s._backend, lambda owner: s._backend.save(s._token, path, owner), "saved", SaveReceipt._read)

class EditState(str, Enum):
    editing = "editing"
    committing = "committing"
    committed = "committed"
    cancelled = "cancelled"
    aborted = "aborted"
    failed = "failed"
    unknown = "unknown"


class Edit(RecoveryOwner):
    'One mutable candidate owned by one sequential caller. Use session.edit. Successful writes advance sequence; publication is a separate Job and ordinary failures cannot be caught into a successful commit.'
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

    def plan(self, operation: ops.PlannableOperation) -> plans.Plan:
        'Plan a registered typed operation against this candidate and exact sequence. A subsequent write invalidates the Plan. Apply as the first and only Plan in a batch, then close it.'
        self._ensure_editing()
        return plans.Plan._create(self._backend, self._session._token, self._token, operation)

    def plan_async(self, operation: ModelingPlanOperation) -> Job[plans.Plan]:
        'Submit registered modeling planning against this candidate Source. Wait for Plan before applying; a changed candidate makes it stale. Job timeout does not cancel or replay work.'
        self._ensure_editing()
        return _plan_job(self._backend, self._session._token, self._token, operation)

    @overload
    def apply(self, operation: ops.Operation) -> WriteResult: ...
    @overload
    def apply(self, operation: plans.Plan) -> WriteResult: ...
    def apply(self, operation: ops.Operation | plans.Plan) -> WriteResult:
        'Apply one domain Operation or owned Plan to the candidate. Returns WriteResult with exact new Source and ordered IDs; commit remains separate. On transport loss recover the original call, never resend the operation.'
        return self.apply_many((operation,))

    def _construct_and_apply(self, construct: Callable[[], ops.Operation]) -> WriteResult:
        self._ensure_editing()
        try:
            operation = construct()
        except Exception as error:
            self._poison(error)
            raise
        return self.apply(operation)

    def apply_many(self, values: Sequence[ops.Operation | plans.Plan]) -> WriteResult:
        'Own and execute an ordered atomic candidate batch. At most one Plan is allowed, in the first position. Success advances sequence once; partial candidate failure aborts the Edit and never publishes formal partial content.'
        self._ensure_editing()
        try:
            owned = tuple(values)
        except (TypeError, ValueError, OverflowError, AttributeError) as error:
            self._poison(error)
            raise
        def execute() -> object:
            if any(isinstance(op, plans.Plan) for op in owned[1:]):
                raise ValueError("A batch accepts at most one Plan, in its first position")
            wire = tuple(operations.ApplyPlan(op) if isinstance(op, plans.Plan) else op for op in owned)
            for op in wire:
                operations.validate(op)
                if isinstance(op, (resources.CreateImage, resources.ReplaceImage)):
                    op.image._check(self._backend)
                elif isinstance(op, operations.ApplyPlan):
                    op.plan._check(self._backend)
            return self._backend.apply(self._token, wire, self)
        def applied(raw: object) -> WriteResult:
            value = WriteResult._read(raw)
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
        'Accept publication and return Job[Receipt]. A recovered commit returns that same Job without implicitly waiting. Wait confirms publication; context exit waits if necessary. Never replay an unknown commit.'
        self._ensure_editing()
        def accepted(raw: object) -> Job[Receipt]:
            self._job = Job(self._backend, record(raw)["job"], "published", Receipt._read, self._finished)
            self._state, self._unknown = EditState.committing, None
            return self._job
        return self._run("commit", lambda: dict(job=self._backend.commit(self._token, self)), accepted)

    def cancel(self) -> None:
        'Release this candidate without publishing or inventing a Receipt. Cancellation is idempotent after confirmation; a lost release response remains recoverable by its original owner.'
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


def _plan_job(backend: Backend, session: object, edit: object | None,
              operation: ModelingPlanOperation) -> Job[plans.Plan]:
    if not isinstance(operation, MODELING_PLAN_TYPES):
        raise TypeError("This operation is not registered for asynchronous planning")
    _modeling_codec.validate_ModelingCommand(operation)
    return Job[plans.Plan]._start(backend, lambda owner: backend.modeling_plan_job(session, edit, operation, owner),
        "planned", lambda raw: plans.Plan(backend)._load(raw))
