"""Explicit Session, Draft and Job ownership over the shared v4 contract."""
from __future__ import annotations
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from pathlib import Path
from types import TracebackType
from typing import Any, Literal, Self, cast, overload
import hashlib
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
        info = self._backend.connection_info
        return None if info is None else _freeze({key: value for key, value in info.items() if key != "secret"})

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

    def _binary[T](self, operation: v.Operation, payload: bytes, project: Callable[[Any], T]) -> T:
        def resolve(run: Callable[[], Any]) -> T:
            try:
                result = run()
            except OutcomeUnknown as unknown:
                unknown._resume = lambda: resolve(self._backend.recover)
                raise
            return project(result)
        return resolve(lambda: self._backend.binary_request(encode(operation), payload))

    def generate_editor_mesh(
        self, width: int, height: int, *, alpha: bytes = b'',
        stage: Literal['edges', 'interior', 'full'] = 'full',
        settings: Mapping[str, object] | None = None, geometry: v.MeshGeometry | None = None,
    ) -> v.MeshGeometry:
        """Generate detached geometry with the connected Editor's bounded service."""
        if not 0 < width <= 16384 or not 0 < height <= 16384 or width * height > 4194304:
            raise ValueError('Editor mesh generation supports at most 4194304 source pixels')
        if stage == 'interior':
            if geometry is None or alpha:
                raise ValueError('Interior generation requires geometry and no alpha bytes')
        elif geometry is not None or len(alpha) != width * height:
            raise ValueError('Edge and full generation require one alpha byte per pixel')
        value = dict(width=width, height=height, stage=stage, settings=dict(settings or {}),
                     alpha_hex=alpha.hex(), geometry=None if geometry is None else geometry.to_wire())
        return self._call(v.OperationEditorMeshGenerate(input=value), v.MeshGeometry.from_wire)

    def blob(self, length: int) -> Blob:
        return self._call(v.OperationCreateBlob(length=length), lambda id: Blob(self, id, length))

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

    def import_project(self, path: Path, format: v.ImportFormat, *, expected: v.Version | None = None) -> Job[WireMap]:
        if not path.is_absolute():
            raise ValueError('Import path must be absolute on the host')
        return self.runtime._call(v.OperationImport(expected=expected or self.version, path=str(path), format=format), lambda job: Job[WireMap](self.runtime, job, _freeze))

    def samples(self, input: v.SampleRequest, *, source: v.QuerySource | None = None) -> Job[WireMap]:
        return self.runtime._call(v.OperationSample(source=source or self.source, input=input), lambda id: Job(self.runtime, id, _freeze))

    def observe(self, input: v.SampleInput, view: v.ObservationView, *, source: v.QuerySource | None = None) -> Job[WireMap]:
        return self.runtime._call(v.OperationObserve(source=source or self.source, input=input, view=view), lambda id: Job(self.runtime, id, _freeze))

    def export(self, path: Path, format: v.ExportFormat, *, allow_loss: bool = False, source: v.QuerySource | None = None) -> Job[WireMap]:
        if not path.is_absolute(): raise ValueError('Export path must be absolute on the host')
        return self.runtime._call(v.OperationExport(source=source or self.source, path=str(path), format=format, allow_loss=allow_loss), lambda id: Job(self.runtime, id, _freeze))

    def image_blob(self, id: str, *, source: v.QuerySource | None = None) -> Blob:
        return self.runtime._call(v.OperationReadImageBlob(source=source or self.source, id=id), lambda info: Blob(self.runtime, info['blob'], info['length'], info))

    def show_window(self) -> None:
        """Ask the connected desktop host to show this resident Session."""
        self.runtime._call(v.OperationShowWindow(session=self.id))

    def editor_context(self) -> WireMap | None:
        return self.runtime._call(v.OperationEditorContext(expected=self.version), _freeze)

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

    def replace_image_blob(self, id: str, blob: Blob, *, width: int, height: int) -> None:
        self._ready()
        if blob.runtime is not self.session.runtime:
            raise ValueError('Blob belongs to another Runtime client')
        self.session.runtime._call(v.OperationReplaceImageBlob(expected=self._source, id=id, blob=blob.id, width=width, height=height), self._updated, self._failure)

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

    def blob(self) -> Blob:
        return self.runtime._call(v.OperationReadJobBlob(job=self.id), lambda info: Blob(self.runtime, info['blob'], info['length'], info))

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


class Blob:
    """Owned bulk bytes. Upload and download one bounded chunk per recoverable call."""
    def __init__(self, runtime: Runtime, id: str, length: int, info: WireMap | None = None) -> None:
        self.runtime = runtime
        self.id = id
        self.length = length
        self.info: WireMap = _freeze(info or {})
        self._closed = False

    def append(self, offset: int, payload: bytes) -> int:
        if not isinstance(payload, bytes) or len(payload) > 1 << 20:
            raise ValueError('Payload must be bytes of at most 1 MiB')
        operation = v.OperationAppendBlobBinary(blob=self.id, offset=offset, length=len(payload), payload_sha256=hashlib.sha256(payload).hexdigest())
        return self.runtime._binary(operation, payload, lambda result: int(result[0]))

    def seal(self) -> int:
        return int(self.runtime._call(v.OperationSealBlob(blob=self.id)))

    def read(self, offset: int, length: int) -> bytes:
        return self.runtime._binary(v.OperationReadBlobBinary(blob=self.id, offset=offset, length=length), b'', lambda result: bytes(result[1]))

    def close(self) -> None:
        if not self._closed:
            def done(_: Any) -> None:
                self._closed = True
            self.runtime._call(v.OperationReleaseBlob(blob=self.id), done)

    def __enter__(self) -> Self:
        return self

    def __exit__(self, kind: type[BaseException] | None, error: BaseException | None, tb: TracebackType | None) -> None:
        if not isinstance(error, OutcomeUnknown): self.close()
