"""Handwritten ownership/known-state wrappers. Runtime remains authoritative."""
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import Generic, Literal, TypeVar
from . import meshes, ops, plans, resources
from ._errors import Busy, OutcomeUnknown, PendingRequest, SdkFailure
from ._values import ConnectionInfo, Receipt, SaveReceipt, SessionId, SessionInfo, WriteResult, record, sequence
from .backends import Backend, JobKind
from ._recovery import RecoveryOwner
from ._generated.conveniences import EditableMeshes as MeshConveniences

T = TypeVar("T")
U = TypeVar("U")

def known(call: Callable[[], T], resume: Callable[[], T]) -> T:
    """One recovery closure remains bound to its original outcome."""
    resolved = False
    def invoke(action: Callable[[], T]) -> T:
        nonlocal resolved
        if resolved:
            raise SdkFailure("NO_PENDING_REQUEST", "This request has already been resolved")
        try:
            result = action()
        except OutcomeUnknown as e:
            e._resume = lambda: invoke(resume)
            raise
        except Busy:
            raise
        except BaseException:
            resolved = True
            raise
        resolved = True
        return result
    return invoke(call)

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

    def _request(self, action: str, call: Callable[[], object], decode: Callable[[object], U]) -> U:
        try:
            return super()._request(action, call, decode)
        except OutcomeUnknown as e:
            if action == "wait" and self.pending_request is None:
                e._resume = self.wait
            raise
        except Busy:
            raise
        except SdkFailure as e:
            if action == "wait":
                self._failure = e
                self._terminal_failure = e.code not in {"REQUEST_EXPIRED", "CLIENT_EXPIRED", "HOST_RESTARTED",
                    "HOST_CLOSED", "HOST_PANICKED", "INVALID_REFERENCE", "NO_PENDING_REQUEST", "RESYNC_REQUIRED"}
                if self._on_terminal is not None:
                    self._on_terminal(False if self._terminal_failure else None)
            raise

    def _wait_value(self, raw: object) -> T:
        if "result" in record(raw):
            from ._errors import unwrap
            info = record(raw)
            if info["result"] is None:
                return self._wait_value(self._backend.wait(self._token, self._kind, self))
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

    def wait(self) -> T:
        if self._failure is not None:
            raise self._failure
        if self._result is not None:
            return self._result[0]
        try:
            return self._request("wait", lambda: self._backend.wait(self._token, self._kind, self), self._wait_value)
        except OutcomeUnknown as e:
            # A wait timeout has no pending wire request; the accepted Job is
            # still queryable by its stable identity.
            if self.pending_request is None:
                e._resume = self.wait
            raise
        except Busy:
            raise
        except SdkFailure as e:
            self._failure = e
            raise

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
        self.wait()

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
    def close(self) -> None:
        self._backend.close()

class Sessions:
    def __init__(self, runtime: Runtime):
        self._runtime = runtime
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
    @property
    def id(self) -> SessionId:
        return self._id
    def info(self) -> SessionInfo:
        return known(lambda: SessionInfo.read(self._backend.info(self._token, self)),
            lambda: SessionInfo.read(self._backend.recover(self)))
    def show_window(self) -> None:
        """Display this resident session in its desktop host."""
        known(lambda: self._backend.show_window(self._token, self), lambda: self._backend.recover(self))
    def edit(self, label: str) -> "Edit":
        return Edit(self, label)
    def undo(self) -> Job[Receipt]:
        return Job[Receipt].start(self._backend, lambda owner: self._backend.history(self._token, False, owner), "published", Receipt.read)
    def redo(self) -> Job[Receipt]:
        return Job[Receipt].start(self._backend, lambda owner: self._backend.history(self._token, True, owner), "published", Receipt.read)

class Project:
    def __init__(self, session: Session):
        self._session = session
    def save(self, path: Path) -> Job[SaveReceipt]:
        s = self._session
        path = absolute(path)
        return Job[SaveReceipt].start(s._backend, lambda owner: s._backend.save(s._token, path, owner), "saved", SaveReceipt.read)

class Meshes:
    def __init__(self, session: Session, edit: "Edit | None" = None):
        self._session, self._edit = session, edit
    def get(self, id: str) -> meshes.Snapshot:
        s = self._session
        return known(lambda: meshes.Snapshot.read(s._backend.mesh(s._token, id, None if self._edit is None else self._edit._token, self)),
            lambda: meshes.Snapshot.read(s._backend.recover(self)))

    def plan(self, intent: meshes.RenameIntent) -> plans.Plan:
        s = self._session
        return plans.Plan.create(s._backend, s._token, None if self._edit is None else self._edit._token, intent)

class Images:
    def __init__(self, session: Session, edit: "Edit | None" = None):
        self._session, self._edit = session, edit
    def get(self, id: str) -> resources.Snapshot:
        s = self._session
        return known(lambda: resources.Snapshot.read(s._backend.image(s._token, id, None if self._edit is None else self._edit._token, self)),
            lambda: resources.Snapshot.read(s._backend.recover(self)))
    def plan(self, intent: resources.RenameIntent) -> plans.Plan:
        s = self._session
        return plans.Plan.create(s._backend, s._token, None if self._edit is None else self._edit._token, intent)

class EditableImages(Images):
    def __init__(self, session: Session, edit: "Edit"):
        super().__init__(session, edit)
        self._owner = edit
    def create_image(self, name: str, image: resources.PreparedImage, *, id: str | None = None) -> str:
        return self._owner.apply(ops.CreateImage(name, image, id)).ids[0]
    def replace_image(self, id: str, image: resources.PreparedImage) -> WriteResult:
        return self._owner.apply(ops.ReplaceImage(id, image))

class EditableMeshes(Meshes, MeshConveniences):
    def __init__(self, session: Session, edit: "Edit"):
        super().__init__(session, edit)
        self._owner = edit

class Edit:
    def __init__(self, session: Session, label: str):
        self._session, self._backend = session, session._backend
        self._token: object = None
        self._state = "starting"
        self._action = "begin"
        self._job: Job[Receipt] | None = None
        self.meshes = EditableMeshes(session, self)
        self.resources = EditableImages(session, self)
        try:
            self._token = self._backend.begin(session._token, label, self)
        except OutcomeUnknown as e:
            self._state = "unknown"
            self._bind_unknown(e)
            raise
        self._state = "editing"

    def _bind_unknown(self, error: OutcomeUnknown) -> None:
        expected, action = error.pending, self._action
        def resume() -> "Edit | WriteResult | Receipt | None":
            pending = self._job.pending_request if action == "wait" and self._job is not None else self.pending_request
            if self._action != action or (pending is not None and pending != expected):
                raise SdkFailure("NO_PENDING_REQUEST", "This Edit request has already been resolved")
            return self.recover()
        error._resume = resume

    def __enter__(self) -> "Edit":
        self._ensure_editing()
        return self

    def _ensure_editing(self) -> None:
        if self._state != "editing":
            raise SdkFailure("EDIT_ABORTED" if self._state == "aborted" else "EDIT_CLOSED",
                f"Edit is {self._state}")

    def _poison(self, original: BaseException) -> None:
        self._state = "aborted"
        try:
            self._release()
        except BaseException as cleanup:
            original.add_note(f"Candidate cleanup failed: {cleanup}")

    def _release(self) -> None:
        self._action = "release"
        try:
            self._backend.release(self._token, self)
        except OutcomeUnknown as e:
            if self._state != "aborted":
                self._state = "unknown"
            self._bind_unknown(e)
            raise

    def apply(self, operation: ops.Operation) -> WriteResult:
        return self.apply_many((operation,))

    def apply_plan(self, plan: plans.Plan) -> WriteResult:
        return self.apply(ops.ApplyPlan(plan))

    def apply_many(self, operations: Sequence[ops.Operation]) -> WriteResult:
        self._ensure_editing()
        self._action = "apply"
        try:
            owned = tuple(operations)
            if any(not isinstance(op, (ops.Create, ops.Rename, ops.SetPositions, ops.CreateImage, ops.ReplaceImage, ops.ApplyPlan)) for op in owned):
                raise TypeError("Expected typed authoring operations")
            for op in owned:
                ops.validate(op)
                if isinstance(op, (ops.CreateImage, ops.ReplaceImage)):
                    op.image._check(self._backend)
                elif isinstance(op, ops.ApplyPlan):
                    op.plan._check(self._backend)
            return WriteResult.read(self._backend.apply(self._token, owned, self))
        except OutcomeUnknown as e:
            self._state = "unknown"
            self._bind_unknown(e)
            raise
        except Busy:
            raise
        except SdkFailure as e:
            if e.code != "BUDGET_EXCEEDED" or e.edit_state != "editing":
                self._poison(e)
            raise
        except (TypeError, ValueError, OverflowError, AttributeError) as e:
            self._poison(e)
            raise

    def _finished(self, success: bool | None) -> None:
        if success is None:
            self._state, self._action = "unknown", "wait"
        else:
            self._state = "committed" if success else "failed"

    def commit(self) -> Job[Receipt]:
        self._ensure_editing()
        self._action = "commit"
        try:
            token = self._backend.commit(self._token, self)
        except OutcomeUnknown as e:
            self._state = "unknown"
            self._bind_unknown(e)
            raise
        self._state = "committing"
        self._job = Job(self._backend, token, "published", Receipt.read, self._finished)
        return self._job

    def cancel(self) -> None:
        if self._state == "unknown":
            raise SdkFailure("OUTCOME_UNKNOWN", "Recover this Edit before cleanup")
        if self._state in {"cancelled", "committed", "failed"}:
            return
        if self._state == "committing":
            raise Busy("BUSY", "Accepted commit must complete before its outcome is known")
        self._release()
        self._state = "cancelled"

    @property
    def pending_request(self) -> PendingRequest | None:
        return self._backend.pending(self)

    def recover(self) -> "Edit | WriteResult | Receipt | None":
        if self._state != "unknown" and not (self._state == "aborted" and self._action == "release" and self.pending_request is not None):
            raise SdkFailure("NO_PENDING_REQUEST", "Edit has no unknown result")
        if self._action == "wait" and self._job is not None:
            result = self._job.wait()
            self._state = "committed"
            return result
        try:
            raw = self._backend.recover(self)
        except OutcomeUnknown as e:
            self._bind_unknown(e)
            raise
        except Busy:
            raise
        except SdkFailure as e:
            if e.code not in {"REQUEST_EXPIRED", "HOST_RESTARTED", "CLIENT_EXPIRED", "NO_PENDING_REQUEST"}:
                if e.code == "BUDGET_EXCEEDED" and e.edit_state == "editing":
                    self._state = "editing"
                else:
                    self._poison(e)
            raise
        if self._action == "apply":
            self._state = "editing"
            return WriteResult.read(raw)
        if self._action == "begin":
            self._token = raw
            self._state = "editing"
            return self
        if self._action == "release":
            self._state = "cancelled"
            return None
        if self._action == "commit":
            self._job = Job(self._backend, record(raw)["job"], "published", Receipt.read, self._finished)
            self._state = "committing"
            result = self._job.wait()
            self._state = "committed"
            return result
        raise SdkFailure("INVALID_RECOVERY", "Unknown edit recovery action")

    def __exit__(self, kind: object, exception: BaseException | None, traceback: object) -> None:
        if self._state == "unknown":
            return
        if isinstance(exception, OutcomeUnknown):
            if self._job is not None:
                self._state, self._action = "unknown", "wait"
            return
        if exception is not None:
            try:
                self.cancel()
            except BaseException as cleanup:
                exception.add_note(f"Candidate cleanup failed: {cleanup}")
            return
        if self._state == "aborted":
            failure = SdkFailure("EDIT_ABORTED", "Caught failure made this candidate uncommittable")
            try:
                self.cancel()
            except BaseException as cleanup:
                failure.add_note(f"Candidate cleanup failed: {cleanup}")
            raise failure
        if self._state == "editing":
            self.commit()
        if self._state == "committing" and self._job is not None:
            try:
                self._job.wait()
            except OutcomeUnknown as e:
                self._state, self._action = "unknown", "wait"
                self._bind_unknown(e)
                raise
            self._state = "committed"
