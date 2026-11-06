"""Explicit Session, Draft and Job ownership over the shared v4 contract."""
from __future__ import annotations
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from pathlib import Path
from types import TracebackType
from typing import Any, Literal, Self, cast, overload
import math
import time
from . import values as v
from ._value_codec import encode, _freeze
from ._errors import SdkFailure, Busy, OutcomeUnknown, JobTimeout, unwrap
from .backends import Backend

type WireMap = Mapping[str, Any]

def _identity(value: Any) -> Any:
    return value

def _ignore(error: SdkFailure) -> None:
    pass

@dataclass(frozen=True)
class Snapshot:
    source: v.QuerySource
    content: str
    value: Any

    @classmethod
    def read(cls, data: WireMap) -> Self:
        return cls(v.QuerySource.from_wire(data['source']), data['content'], _freeze(data['value']))

class Runtime:
    def __init__(self, backend: Backend) -> None:
        self._backend = backend
        self.sessions = Sessions(self)
        self._closed = False

    @property
    def connection(self) -> WireMap | None:
        return self._backend.connection_info

    def __enter__(self) -> Self:
        return self

    def __exit__(self, kind: type[BaseException] | None, error: BaseException | None, tb: TracebackType | None) -> None:
        if not isinstance(error, OutcomeUnknown):
            self.close()

    def close(self) -> None:
        if not self._closed:
            self._backend.close()
            self._closed = True

    @overload
    def _call(self, operation: v.Operation) -> Any: ...
    @overload
    def _call[T](self, operation: v.Operation, project: Callable[[Any], T], failed: Callable[[SdkFailure], None] = _ignore) -> T: ...
    def _call(self, operation: v.Operation, project: Callable[[Any], Any] = _identity, failed: Callable[[SdkFailure], None] = _ignore) -> Any:
        def resolve(run: Callable[[], Any]) -> Any:
            try:
                value = run()
            except OutcomeUnknown as unknown:
                unknown._resume = lambda: resolve(self._backend.recover)
                raise
            except SdkFailure as error:
                failed(error)
                raise
            return project(value)
        return resolve(lambda: self._backend.request(encode(operation)))

    def capabilities(self) -> tuple[str, ...]:
        return tuple(self._call(v.OperationRuntimeCapabilities()))

    def limits(self) -> WireMap:
        return _freeze(self._call(v.OperationRuntimeLimits()))

class Sessions:
    def __init__(self, runtime: Runtime) -> None:
        self._runtime = runtime

    def list(self) -> tuple[WireMap, ...]:
        return tuple(_freeze(value) for value in self._runtime._call(v.OperationListSessions()))

    def create(self, *, document_id: str | None = None, setup: v.ModelSetup | None = None) -> Session:
        operation = v.OperationCreateSession(document_id=document_id) if setup is None else v.OperationCreateSession(document_id=document_id, setup=setup)
        return self._runtime._call(operation, lambda info: Session(self._runtime, info['session']))

    def attach(self, session: str) -> Session:
        return self._runtime._call(v.OperationAttachSession(session=session), lambda info: Session(self._runtime, info['session']))

class Session:
    def __init__(self, runtime: Runtime, session: str) -> None:
        self.runtime = runtime
        self.id = session

    def info(self) -> WireMap:
        return _freeze(self.runtime._call(v.OperationSessionInfo(session=self.id)))

    @property
    def version(self) -> v.Version:
        return v.Version.from_wire(self.info()['version'])

    @property
    def source(self) -> v.QuerySource:
        return v.QuerySourcePublished(value=self.version)

    def draft(self, description: str = 'Edit', *, expected: v.Version | None = None) -> Draft:
        return self.runtime._call(v.OperationBeginDraft(expected=expected or self.version), lambda source: Draft(self, v.DraftSource.from_wire(source), description))

    def query(self, query: v.Query, *, source: v.QuerySource | None = None) -> Snapshot:
        return self.runtime._call(v.OperationQuery(source=source or self.source, input=query), Snapshot.read)

    def undo(self, *, expected: v.Version | None = None) -> WireMap:
        return self.runtime._call(v.OperationUndo(expected=expected or self.version), _freeze)

    def redo(self, *, expected: v.Version | None = None) -> WireMap:
        return self.runtime._call(v.OperationRedo(expected=expected or self.version), _freeze)

    def save(self, path: Path | None = None, *, fingerprint: v.ProjectFingerprint | None = None) -> Job[WireMap]:
        expected = self.version
        if path is None:
            operation = v.OperationSave(expected=expected)
        else:
            if not path.is_absolute():
                raise ValueError('Project path must be absolute on the host')
            operation = v.OperationSaveAs(expected=expected, path=str(path), fingerprint=fingerprint)
        return self.runtime._call(operation, lambda job: Job[WireMap](self.runtime, job, _freeze))

    def open(self, path: Path) -> Job[WireMap]:
        if not path.is_absolute():
            raise ValueError('Project path must be absolute on the host')
        return self.runtime._call(v.OperationOpen(expected=self.version, path=str(path)), lambda job: Job[WireMap](self.runtime, job, _freeze))

    def subscribe(self) -> WireMap:
        return self.runtime._call(v.OperationSubscribe(session=self.id), _freeze)

    def events(self, cursor: v.Version) -> tuple[WireMap, ...]:
        return self.runtime._call(v.OperationEvents(cursor=cursor), _freeze)

    def detach(self) -> None:
        self.runtime._call(v.OperationDetachSession(session=self.id))

    def close(self, *, discard_modified: bool = False) -> None:
        self.runtime._call(v.OperationCloseSession(session=self.id, discard_modified=discard_modified))

class Draft:
    def __init__(self, session: Session, source: v.DraftSource, description: str) -> None:
        self.session = session
        self._source = source
        self.description = description
        self._state: Literal['ready', 'working', 'ended'] = 'ready'

    @property
    def state(self) -> Literal['ready', 'working', 'ended']:
        return self._state

    @property
    def source(self) -> v.DraftSource:
        return self._source

    def _ready(self) -> None:
        if self._state != 'ready':
            raise SdkFailure('DRAFT_' + ('BUSY' if self._state == 'working' else 'ENDED'), 'Draft is ' + self._state)

    def _failure(self, error: SdkFailure) -> None:
        if error.code not in ('DRAFT_BUSY', 'PROJECT_BUSY', 'NOT_OWNER', 'CLIENT_NOT_FOUND', 'RECOVERY_REQUIRED'):
            self._state = 'ended'

    def _updated(self, value: WireMap) -> tuple[WireMap, ...]:
        self._source = v.DraftSource.from_wire(value['source'])
        return tuple(_freeze(outcome) for outcome in value['outcomes'])

    def apply(self, *commands: v.Command) -> tuple[WireMap, ...]:
        self._ready()
        return self.session.runtime._call(v.OperationBatch(expected=self._source, commands=commands), self._updated, self._failure)

    def preview(self, *commands: v.Command) -> tuple[WireMap, ...]:
        """Prepare a gesture position; rejection preserves the last valid Draft."""
        self._ready()
        return self.session.runtime._call(v.OperationPreview(expected=self._source, commands=commands), self._updated)

    def apply_async(self, *commands: v.Command) -> Job[v.DraftSource]:
        self._ready()
        def accepted(job: str) -> Job[v.DraftSource]:
            self._state = 'working'
            def completed(value: WireMap) -> v.DraftSource:
                self._source = v.DraftSource.from_wire(value['source'])
                self._state = 'ready'
                return self._source
            def failed(error: SdkFailure) -> None:
                self._state = 'ended'
            return Job(self.session.runtime, job, completed, failed)
        def refused(error: SdkFailure) -> None:
            if error.code != 'RESOURCE_LIMIT':
                self._failure(error)
        return self.session.runtime._call(v.OperationBatchJob(expected=self._source, commands=commands), accepted, refused)

    def query(self, query: v.Query) -> Snapshot:
        self._ready()
        return self.session.query(query, source=v.QuerySourceDraft(value=self._source))

    def publish(self) -> WireMap:
        self._ready()
        def done(value: WireMap) -> WireMap:
            self._state = 'ended'
            return _freeze(value)
        return self.session.runtime._call(v.OperationPublish(expected=self._source, description=self.description), done, self._failure)

    def cancel(self) -> None:
        if self._state != 'ended':
            def done(value: Any) -> None:
                self._state = 'ended'
            self.session.runtime._call(v.OperationCancelDraft(draft=self._source.draft), done)

    def __enter__(self) -> Self:
        self._ready()
        return self

    def __exit__(self, kind: type[BaseException] | None, error: BaseException | None, tb: TracebackType | None) -> None:
        if isinstance(error, OutcomeUnknown):
            return
        if kind is None:
            if self._state == 'ready':
                self.publish()
            elif self._state == 'working':
                raise Busy('DRAFT_BUSY', 'Wait for the Draft job before leaving its context')
        elif self._state != 'ended':
            self.cancel()

class Job[T]:
    def __init__(self, runtime: Runtime, job: str, completed: Callable[[Any], T], failed: Callable[[SdkFailure], None] = _ignore) -> None:
        self.runtime = runtime
        self.id = job
        self._completed = completed
        self._failed = failed
        self._resolved = False
        self._value: T | None = None
        self._error: SdkFailure | None = None
        self._closed = False

    def status(self) -> WireMap:
        return self.runtime._call(v.OperationJob(job=self.id), _freeze)

    def _finish(self, value: WireMap) -> T | None:
        if value['state'] not in ('succeeded', 'failed', 'cancelled'):
            return None
        if not self._resolved:
            try:
                self._value = self._completed(unwrap(dict(value['result'])))
            except SdkFailure as error:
                self._error = error
                self._failed(error)
            self._resolved = True
        if self._error is not None:
            raise self._error
        return self._value

    def wait(self, timeout: float | None = None) -> T:
        if timeout is not None and (not math.isfinite(timeout) or timeout < 0):
            raise ValueError('Invalid timeout')
        deadline = None if timeout is None else time.monotonic() + timeout
        while True:
            if self._resolved:
                if self._error is not None:
                    raise self._error
                return cast(T, self._value)
            # Recovery completes this original poll and its Draft transition.
            result = self.runtime._call(v.OperationJob(job=self.id), self._finish)
            if self._resolved:
                return cast(T, result)
            if deadline is not None and time.monotonic() >= deadline:
                raise JobTimeout()
            time.sleep(.005)

    def cancel(self) -> None:
        self.runtime._call(v.OperationCancelJob(job=self.id))

    def close(self) -> None:
        if self._closed:
            return
        if not self._resolved:
            self.cancel()
            try:
                self.wait()
            except OutcomeUnknown:
                raise
            except SdkFailure:
                if not self._resolved:
                    raise
        self.runtime._call(v.OperationReleaseJob(job=self.id))
        self._closed = True

    def __enter__(self) -> Self:
        return self

    def __exit__(self, kind: type[BaseException] | None, error: BaseException | None, tb: TracebackType | None) -> None:
        if not isinstance(error, OutcomeUnknown):
            self.close()
