"""Handwritten ownership/known-state wrappers. Runtime remains authoritative."""
from collections.abc import Callable, Sequence
from pathlib import Path
from typing import Generic, TypeVar
from . import meshes, ops
from ._errors import Busy, OutcomeUnknown, PendingRequest, SdkFailure
from ._values import Receipt, SaveReceipt, SessionInfo, WriteResult, record
from .backends import Backend, JobKind

T = TypeVar("T")

def known(call: Callable[[], T], resume: Callable[[], T]) -> T:
    """Attach a typed wrapper's recovery to an otherwise opaque transport loss."""
    def retry() -> T:
        return known(resume, resume)
    try:
        return call()
    except OutcomeUnknown as e:
        e._resume = retry
        raise

def absolute(path: Path) -> Path:
    if not path.is_absolute():
        raise ValueError("Path must be absolute on the execution host")
    return path

class Job(Generic[T]):
    def __init__(self, backend: Backend, token: object, kind: JobKind, decode: Callable[[object], T]):
        self._backend, self._token, self._kind, self._decode = backend, token, kind, decode
        self._kind: JobKind
        self._result: tuple[T] | None = None
        self._failure: SdkFailure | None = None

    @classmethod
    def start(cls, backend: Backend, begin: Callable[[object], object], kind: JobKind,
        decode: Callable[[object], T]) -> "Job[T]":
        job = cls(backend, None, kind, decode)
        def resume() -> "Job[T]":
            job._token = record(backend.recover(job))["job"]
            return job
        def start() -> "Job[T]":
            job._token = begin(job)
            return job
        return known(start, resume)

    def wait(self) -> T:
        if self._failure is not None:
            raise self._failure
        if self._result is None:
            try:
                raw = self._backend.recover(self) if self.pending_request is not None else self._backend.wait(self._token, self._kind, self)
                # A recovered terminal query has the JobInfo envelope; a
                # nonterminal recovered query must continue waiting normally.
                if self.pending_request is None and "result" in record(raw):
                    from ._errors import unwrap
                    info = record(raw)
                    raw = self._backend.wait(self._token, self._kind, self) if info["result"] is None else unwrap(info["result"])
                self._result = (self._decode(raw),)
            except OutcomeUnknown as e:
                e._resume = self.wait
                raise
            except Busy:
                raise
            except SdkFailure as e:
                self._failure = e
                raise
        return self._result[0]

    @property
    def pending_request(self) -> PendingRequest | None:
        return self._backend.pending(self)

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
        self.meshes = Meshes(self)
        self.project = Project(self)
    def info(self) -> SessionInfo:
        return known(lambda: SessionInfo.read(self._backend.info(self._token, self)),
            lambda: SessionInfo.read(self._backend.recover(self)))
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

class EditableMeshes(Meshes):
    def __init__(self, session: Session, edit: "Edit"):
        super().__init__(session, edit)
        self._owner = edit
    def create(self, spec: meshes.MeshSpec) -> str:
        return self._owner.apply(ops.Create(spec)).ids[0]
    def rename(self, id: str, name: str) -> WriteResult:
        return self._owner.apply(ops.Rename(id, name))
    def set_positions(self, id: str, *, vertex_ids: Sequence[int], positions: Sequence[meshes.Point]) -> WriteResult:
        return self._owner.apply(ops.SetPositions(id, tuple(vertex_ids), tuple(positions)))

class Edit:
    def __init__(self, session: Session, label: str):
        self._session, self._backend = session, session._backend
        self._token: object = None
        self._state = "starting"
        self._action = "begin"
        self._job: Job[Receipt] | None = None
        self.meshes = EditableMeshes(session, self)
        try:
            self._token = self._backend.begin(session._token, label, self)
        except OutcomeUnknown as e:
            self._state = "unknown"
            e._resume = self.recover
            raise
        self._state = "editing"

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
            e._resume = self.recover
            raise

    def apply(self, operation: ops.MeshOperation) -> WriteResult:
        return self.apply_many((operation,))

    def apply_many(self, operations: Sequence[ops.MeshOperation]) -> WriteResult:
        self._ensure_editing()
        self._action = "apply"
        try:
            owned = tuple(operations)
            if any(not isinstance(op, (ops.Create, ops.Rename, ops.SetPositions)) for op in owned):
                raise TypeError("Expected typed mesh operations")
            for op in owned:
                ops.validate(op)
            return WriteResult.read(self._backend.apply(self._token, owned, self))
        except OutcomeUnknown as e:
            self._state = "unknown"
            e._resume = self.recover
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

    def commit(self) -> Job[Receipt]:
        self._ensure_editing()
        self._action = "commit"
        try:
            token = self._backend.commit(self._token, self)
        except OutcomeUnknown as e:
            self._state = "unknown"
            e._resume = self.recover
            raise
        self._state = "committing"
        self._job = Job(self._backend, token, "published", Receipt.read)
        return self._job

    def cancel(self) -> None:
        if self._state == "unknown":
            raise SdkFailure("OUTCOME_UNKNOWN", "Recover this Edit before cleanup")
        if self._state in {"cancelled", "committed"}:
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
            e._resume = self.recover
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
            self._job = Job(self._backend, record(raw)["job"], "published", Receipt.read)
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
                e._resume = self.recover
                raise
            self._state = "committed"
