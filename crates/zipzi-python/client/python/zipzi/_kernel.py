"""Explicit Session, Draft and Job ownership over the shared v4 contract."""

from __future__ import annotations
from collections.abc import Callable, Mapping
from dataclasses import dataclass
from pathlib import Path
from types import TracebackType
from typing import Any, Literal, Self, cast, overload
import hashlib
import math
import reprlib
import time
from . import values as v
from . import results as r
from ._value_codec import Value, encode, _freeze
from ._errors import SdkFailure, Busy, OutcomeUnknown, JobTimeout, unwrap
from .backends import Backend

type WireMap = Mapping[str, Any]


def _identity(value: Any) -> Any:
    return value


def _ignore(error: SdkFailure) -> None:
    pass


@dataclass(frozen=True, repr=False)
class Snapshot[T = Any]:
    """A query value tied to an exact source and immutable content identity.

    value has the response shape selected by the query. records(Type) decodes
    object/collection payloads into generated values. Reuse source for subsequent
    pages or dependent reads; publication makes an old source stale.
    content identifies resident content, not a persistent file hash. Opening or
    importing creates a new identity; verify reloaded object fields and pixels.
    """

    source: v.QuerySource
    content: str
    value: T

    def __repr__(self) -> str:
        return f"Snapshot(source={self.source!r}, content={self.content!r}, value={reprlib.repr(self.value)})"

    def records[U: Value](self, record_type: type[U]) -> tuple[U, ...]:
        """Decode all object records in order, e.g. snapshot.records(values.Mesh).

        Use a query selecting the corresponding object kind. Mixed-kind reads
        should decode individual payloads with Type.from_wire instead. Invalid
        payloads raise TypeError/ValueError; summary/capability queries are rejected.
        """
        value: Any = self.value
        if not isinstance(value, Mapping):
            raise TypeError("records() requires an Objects or Collection query")
        data = cast(Mapping[str, Any], value)
        if "records" not in data:
            raise TypeError("records() requires an Objects or Collection query")
        items = cast(r.Objects, data)["records"]
        return tuple(record_type.from_wire(item["value"]) for item in items)

    @classmethod
    def read(cls, data: WireMap) -> Self:
        """Decode a source-bearing response; normally called by Session.query()."""
        return cls(
            v.QuerySource.from_wire(data["source"]),
            data["content"],
            _freeze(data["value"]),
        )


class Runtime:
    """One owned client, created by local() or connect(); use as a context manager."""

    sessions: Sessions

    def __init__(self, backend: Backend) -> None:
        self._backend = backend
        self.sessions = Sessions(self)
        self._closed = False

    @property
    def connection(self) -> WireMap | None:
        """Cached handshake metadata with the private secret removed; None for Local."""
        info = self._backend.connection_info
        return (
            None
            if info is None
            else _freeze({key: value for key, value in info.items() if key != "secret"})
        )

    def __enter__(self) -> Self:
        return self

    def __exit__(
        self,
        kind: type[BaseException] | None,
        error: BaseException | None,
        tb: TracebackType | None,
    ) -> None:
        if not isinstance(error, OutcomeUnknown):
            self.close()

    def close(self) -> None:
        """Release this client and its owned resources; does not save documents."""
        if not self._closed:
            self._backend.close()
            self._closed = True

    @overload
    def _call(self, operation: v.Operation) -> Any: ...
    @overload
    def _call[T](
        self,
        operation: v.Operation,
        project: Callable[[Any], T],
        failed: Callable[[SdkFailure], None] = _ignore,
    ) -> T: ...
    def _call(
        self,
        operation: v.Operation,
        project: Callable[[Any], Any] = _identity,
        failed: Callable[[SdkFailure], None] = _ignore,
    ) -> Any:
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

    def _binary[T](
        self, operation: v.Operation, payload: bytes, project: Callable[[Any], T]
    ) -> T:
        def resolve(run: Callable[[], Any]) -> T:
            try:
                result = run()
            except OutcomeUnknown as unknown:
                unknown._resume = lambda: resolve(self._backend.recover)
                raise
            return project(result)

        return resolve(lambda: self._backend.binary_request(encode(operation), payload))

    def generate_editor_mesh(
        self,
        width: int,
        height: int,
        *,
        alpha: bytes = b"",
        stage: Literal["edges", "interior", "full"] = "full",
        settings: Mapping[str, object] | None = None,
        geometry: v.MeshGeometry | None = None,
    ) -> v.MeshGeometry:
        """Generate detached geometry with the connected Editor's bounded service."""
        if (
            not 0 < width <= 16384
            or not 0 < height <= 16384
            or width * height > 4194304
        ):
            raise ValueError(
                "Editor mesh generation supports at most 4194304 source pixels"
            )
        if stage == "interior":
            if geometry is None or alpha:
                raise ValueError(
                    "Interior generation requires geometry and no alpha bytes"
                )
        elif geometry is not None or len(alpha) != width * height:
            raise ValueError(
                "Edge and full generation require one alpha byte per pixel"
            )
        value = dict(
            width=width,
            height=height,
            stage=stage,
            settings=dict(settings or {}),
            alpha_hex=alpha.hex(),
            geometry=None if geometry is None else geometry.to_wire(),
        )
        return self._call(
            v.OperationEditorMeshGenerate(input=value), v.MeshGeometry.from_wire
        )

    def blob(self, length: int) -> Blob:
        """Allocate an upload of length bytes; append chunks, seal, then use it.

        Use a context manager. Each transfer call supports at most 1 MiB."""
        return self._call(
            v.OperationCreateBlob(length=length), lambda id: Blob(self, id, length)
        )

    def capabilities(self) -> tuple[str, ...]:
        """Return supported host services, e.g. gpu_observation; does not reserve execution."""
        return tuple(self._call(v.OperationRuntimeCapabilities()))

    def limits(self) -> WireMap:
        """Read host admission and storage limits; calls can still be refused later."""
        return _freeze(self._call(v.OperationRuntimeLimits()))


class Sessions:
    """Discover, create or attach documents on this Runtime."""

    def __init__(self, runtime: Runtime) -> None:
        self._runtime = runtime

    def list(self) -> tuple[r.SessionOverview, ...]:
        """List resident documents without attaching; choose by session ID and path.

        A path may be None and paths need not uniquely identify a document."""
        return tuple(
            _freeze(value) for value in self._runtime._call(v.OperationListSessions())
        )

    def create(
        self, *, document_id: str | None = None, setup: v.ModelSetup | None = None
    ) -> Session:
        """Create and attach a fresh unsaved document; document_id is its persistent UUID."""
        operation = (
            v.OperationCreateSession(document_id=document_id)
            if setup is None
            else v.OperationCreateSession(document_id=document_id, setup=setup)
        )
        return self._runtime._call(
            operation, lambda info: Session(self._runtime, info["session"])
        )

    def attach(self, session: str) -> Session:
        """Attach a session ID from list(); shares Editor content and undo history."""
        return self._runtime._call(
            v.OperationAttachSession(session=session),
            lambda info: Session(self._runtime, info["session"]),
        )


class Session:
    """An attachment to a resident document. Publishing and saving are separate."""

    runtime: Runtime
    id: str

    def __init__(self, runtime: Runtime, session: str) -> None:
        self.runtime = runtime
        self.id = session

    def info(self) -> r.SessionInfo:
        """Read version, content identity, path, dirty state and undo history."""
        return _freeze(self.runtime._call(v.OperationSessionInfo(session=self.id)))

    @property
    def version(self) -> v.Version:
        """Read the current publication identity; expected= guards later edits."""
        return v.Version.from_wire(self.info()["version"])

    @property
    def source(self) -> v.QuerySource:
        """Read the published query source; reuse it for dependent reads."""
        return v.QuerySourcePublished(value=self.version)

    def draft(
        self, description: str = "Edit", *, expected: v.Version | None = None
    ) -> Draft:
        """Begin an isolated candidate from expected or the current version.

        Normal context exit publishes; exceptions cancel. Another publication makes
        this candidate stale. Re-query and rebuild after STALE_SOURCE."""
        return self.runtime._call(
            v.OperationBeginDraft(expected=expected or self.version),
            lambda source: Draft(self, v.DraftSource.from_wire(source), description),
        )

    @overload
    def query(
        self, query: v.QueryObjects, *, source: v.QuerySource | None = None
    ) -> Snapshot[r.Objects]: ...
    @overload
    def query(
        self, query: v.QueryCollection, *, source: v.QuerySource | None = None
    ) -> Snapshot[r.Collection]: ...
    @overload
    def query(
        self, query: v.QuerySummary, *, source: v.QuerySource | None = None
    ) -> Snapshot[r.Summary]: ...
    @overload
    def query(
        self, query: v.QueryMetadata, *, source: v.QuerySource | None = None
    ) -> Snapshot[r.Metadata]: ...
    @overload
    def query(
        self,
        query: v.QueryCollectionCapabilities,
        *,
        source: v.QuerySource | None = None,
    ) -> Snapshot[r.CollectionCapabilities]: ...
    @overload
    def query(
        self, query: v.Query, *, source: v.QuerySource | None = None
    ) -> Snapshot: ...
    def query(self, query: v.Query, *, source: v.QuerySource | None = None) -> Snapshot:
        """Read a typed response without changing content or history.

        Object and collection queries support snapshot.records(values.Mesh) for typed
        payloads. Summary returns counts; capabilities returns permission reasons.
        Metadata returns document roots for read/modify/write using from_wire.
        Pass the original source for dependent reads and subsequent pages."""
        return self.runtime._call(
            v.OperationQuery(source=source or self.source, input=query), Snapshot.read
        )

    def undo(self, *, expected: v.Version | None = None) -> WireMap:
        """Restore previous history content; expected guards against concurrent publication."""
        return self.runtime._call(
            v.OperationUndo(expected=expected or self.version), _freeze
        )

    def redo(self, *, expected: v.Version | None = None) -> WireMap:
        """Restore next history content; each publication advances the revision."""
        return self.runtime._call(
            v.OperationRedo(expected=expected or self.version), _freeze
        )

    def save(
        self,
        path: Path | None = None,
        *,
        fingerprint: v.ProjectFingerprint | None = None,
    ) -> Job[r.SaveReceipt]:
        """Save and return an owned Job[results.SaveReceipt].

        An explicit path must be absolute on the host. With no path, save to the current
        project. Check published, durable and warnings. fingerprint guards replacement
        of an existing destination. Publication is blocked until the save completes."""
        expected = self.version
        if path is None:
            operation = v.OperationSave(expected=expected)
        else:
            if not path.is_absolute():
                raise ValueError("Project path must be absolute on the host")
            operation = v.OperationSaveAs(
                expected=expected, path=str(path), fingerprint=fingerprint
            )
        return self.runtime._call(
            operation, lambda job: Job[r.SaveReceipt](self.runtime, job, _freeze)
        )

    def open(self, path: Path) -> Job[WireMap]:
        """Replace content from an absolute host path; return an owned Job.

        Opening advances generation and resets history. Resolve unsaved work first."""
        if not path.is_absolute():
            raise ValueError("Project path must be absolute on the host")
        return self.runtime._call(
            v.OperationOpen(expected=self.version, path=str(path)),
            lambda job: Job[WireMap](self.runtime, job, _freeze),
        )

    def import_project(
        self, path: Path, format: v.ImportFormat, *, expected: v.Version | None = None
    ) -> Job[WireMap]:
        """Import PSD, Model3 or Cubism from an absolute host path as an owned Job.

        Replaces content and resets history. The result includes resource diagnostics."""
        if not path.is_absolute():
            raise ValueError("Import path must be absolute on the host")
        return self.runtime._call(
            v.OperationImport(
                expected=expected or self.version, path=str(path), format=format
            ),
            lambda job: Job[WireMap](self.runtime, job, _freeze),
        )

    def samples(
        self, input: v.SampleRequest, *, source: v.QuerySource | None = None
    ) -> Job[WireMap]:
        """Sample frozen parameters or animation into a Job blob of UTF-8 JSON.

        Wait, then read job.blob() in chunks. The result includes the captured source.
        Sampling does not publish or change history. See docs/recipes.py."""
        return self.runtime._call(
            v.OperationSample(source=source or self.source, input=input),
            lambda id: Job(self.runtime, id, _freeze),
        )

    def observe(
        self,
        input: v.SampleInput,
        view: v.ObservationView,
        *,
        source: v.QuerySource | None = None,
    ) -> Job[WireMap]:
        """Render frozen input into a Job blob; requires gpu_observation capability.

        Each packet is a 4-byte big-endian JSON-header length, header, then png_bytes
        PNG bytes. Headers identify the source. See docs/recipes.py for decoding."""
        return self.runtime._call(
            v.OperationObserve(source=source or self.source, input=input, view=view),
            lambda id: Job(self.runtime, id, _freeze),
        )

    def export(
        self,
        path: Path,
        format: v.ExportFormat,
        *,
        allow_loss: bool = False,
        source: v.QuerySource | None = None,
    ) -> Job[WireMap]:
        """Export captured content to an absolute host path as an owned Job.

        Check the receipt for publication, durability and format-loss notices.
        allow_loss explicitly permits lossy conversions."""
        if not path.is_absolute():
            raise ValueError("Export path must be absolute on the host")
        return self.runtime._call(
            v.OperationExport(
                source=source or self.source,
                path=str(path),
                format=format,
                allow_loss=allow_loss,
            ),
            lambda id: Job(self.runtime, id, _freeze),
        )

    def image_blob(self, id: str, *, source: v.QuerySource | None = None) -> Blob:
        """Lease captured RGBA pixels and metadata for an asset; close after reading."""
        return self.runtime._call(
            v.OperationReadImageBlob(source=source or self.source, id=id),
            lambda info: Blob(self.runtime, info["blob"], info["length"], info),
        )

    def show_window(self) -> None:
        """Ask the connected desktop host to show this resident Session."""
        self.runtime._call(v.OperationShowWindow(session=self.id))

    def editor_context(self) -> WireMap | None:
        """Read versioned Editor selection/tool context, or None without a desktop view."""
        return self.runtime._call(
            v.OperationEditorContext(expected=self.version), _freeze
        )

    def subscribe(self) -> WireMap:
        """Capture a session snapshot and event cursor; keep one cursor per reader."""
        return self.runtime._call(v.OperationSubscribe(session=self.id), _freeze)

    def events(self, cursor: v.Version) -> tuple[WireMap, ...]:
        """Read publications after cursor; EVENT_GAP requires subscribe() and a fresh read.

        Queries do not generate events. Advance to the last returned event version."""
        return self.runtime._call(v.OperationEvents(cursor=cursor), _freeze)

    def detach(self) -> None:
        """Release this attachment; retain the resident document and unsaved edits."""
        self.runtime._call(v.OperationDetachSession(session=self.id))

    def close(self, *, discard_modified: bool = False) -> None:
        """Close the document; rejects unsaved work or other attachments.

        Set discard_modified=True only when discarding edits is intended."""
        self.runtime._call(
            v.OperationCloseSession(session=self.id, discard_modified=discard_modified)
        )


class Draft:
    """An isolated candidate. Normal context exit publishes one undo step.

    Accepted apply failure ends the candidate; preview rejection preserves it. Wait for
    apply_async jobs before further commands or leaving the context.
    """

    session: Session
    description: str

    def __init__(
        self, session: Session, source: v.DraftSource, description: str
    ) -> None:
        self.session = session
        self._source = source
        self.description = description
        self._state: Literal["ready", "working", "ended"] = "ready"

    @property
    def state(self) -> Literal["ready", "working", "ended"]:
        """Local lifecycle: ready, working during apply_async, or ended after publication/failure."""
        return self._state

    @property
    def source(self) -> v.DraftSource:
        """Exact candidate identity; successful commands advance its sequence."""
        return self._source

    def _ready(self) -> None:
        if self._state != "ready":
            raise SdkFailure(
                "DRAFT_" + ("BUSY" if self._state == "working" else "ENDED"),
                "Draft is " + self._state,
            )

    def _failure(self, error: SdkFailure) -> None:
        state = error.details.get("draft_state")
        if state in ("ready", "working", "ended"):
            self._state = cast(Literal["ready", "working", "ended"], state)
        elif error.code == "DRAFT_ENDED":
            self._state = "ended"

    def _updated(self, value: WireMap) -> tuple[WireMap, ...]:
        self._source = v.DraftSource.from_wire(value["source"])
        return tuple(_freeze(outcome) for outcome in value["outcomes"])

    def apply(self, *commands: v.Command) -> tuple[WireMap, ...]:
        """Apply one atomic validated batch and return command outcomes.

        An ordinary accepted failure ends this Draft. Use preview for repeated gesture
        proposals that must preserve the last valid candidate after rejection."""
        self._ready()
        return self.session.runtime._call(
            v.OperationBatch(expected=self._source, commands=commands),
            self._updated,
            self._failure,
        )

    def replace_image_blob(
        self, id: str, blob: Blob, *, width: int, height: int
    ) -> None:
        """Replace an asset from a sealed RGBA Blob owned by this Runtime.

        The upload must contain width * height * 4 bytes. Close the Blob after use."""
        self._ready()
        if blob.runtime is not self.session.runtime:
            raise ValueError("Blob belongs to another Runtime client")
        self.session.runtime._call(
            v.OperationReplaceImageBlob(
                expected=self._source, id=id, blob=blob.id, width=width, height=height
            ),
            self._updated,
            self._failure,
        )

    def preview(self, *commands: v.Command) -> tuple[WireMap, ...]:
        """Prepare a gesture position; rejection preserves the last valid Draft."""
        self._ready()
        return self.session.runtime._call(
            v.OperationPreview(expected=self._source, commands=commands),
            self._updated,
            self._failure,
        )

    def apply_async(self, *commands: v.Command) -> Job[v.DraftSource]:
        """Submit a batch as an owned Job; wait before using or exiting this Draft.

        The Draft is working until wait() resolves. A wait timeout retains the job.
        Leaving an unfinished Job context requests cancellation."""
        self._ready()

        def accepted(job: str) -> Job[v.DraftSource]:
            self._state = "working"

            def completed(value: WireMap) -> v.DraftSource:
                self._source = v.DraftSource.from_wire(value["source"])
                self._state = "ready"
                return self._source

            def failed(error: SdkFailure) -> None:
                self._state = "ended"

            return Job(self.session.runtime, job, completed, failed)

        return self.session.runtime._call(
            v.OperationBatchJob(expected=self._source, commands=commands),
            accepted,
            self._failure,
        )

    @overload
    def query(self, query: v.QueryObjects) -> Snapshot[r.Objects]: ...
    @overload
    def query(self, query: v.QueryCollection) -> Snapshot[r.Collection]: ...
    @overload
    def query(self, query: v.QuerySummary) -> Snapshot[r.Summary]: ...
    @overload
    def query(self, query: v.QueryMetadata) -> Snapshot[r.Metadata]: ...
    @overload
    def query(
        self, query: v.QueryCollectionCapabilities
    ) -> Snapshot[r.CollectionCapabilities]: ...
    @overload
    def query(self, query: v.Query) -> Snapshot: ...
    def query(self, query: v.Query) -> Snapshot:
        """Read this candidate at its current sequence without publishing.

        Returns the same typed shapes as Session.query; records(Type) decodes payloads.
        Formal Session reads continue to see published content."""
        self._ready()
        return self.session.runtime._call(
            v.OperationQuery(source=v.QuerySourceDraft(value=self._source), input=query),
            Snapshot.read,
            self._failure,
        )

    def publish(self) -> WireMap:
        """Publish as one undo step and return the publication receipt.

        A stale base rejects publication. Recover OutcomeUnknown before further work."""
        self._ready()

        def done(value: WireMap) -> WireMap:
            self._state = "ended"
            return _freeze(value)

        return self.session.runtime._call(
            v.OperationPublish(expected=self._source, description=self.description),
            done,
            self._failure,
        )

    def cancel(self) -> None:
        """Discard this candidate; cancelling an ended Draft does nothing."""
        if self._state != "ended":

            def done(value: Any) -> None:
                self._state = "ended"

            self.session.runtime._call(
                v.OperationCancelDraft(draft=self._source.draft), done, self._failure
            )

    def __enter__(self) -> Self:
        self._ready()
        return self

    def __exit__(
        self,
        kind: type[BaseException] | None,
        error: BaseException | None,
        tb: TracebackType | None,
    ) -> None:
        if isinstance(error, OutcomeUnknown):
            return
        if kind is None:
            if self._state == "ready":
                self.publish()
            elif self._state == "working":
                raise Busy(
                    "DRAFT_BUSY", "Wait for the Draft job before leaving its context"
                )
        elif self._state != "ended":
            self.cancel()


class Job[T]:
    """Owned asynchronous work. wait() returns its projected result of type T.

    A wait timeout retains the job. Context exit closes it, cancelling unfinished
    work, so catch JobTimeout inside the context if you intend to wait again.
    """

    runtime: Runtime
    id: str

    def __init__(
        self,
        runtime: Runtime,
        job: str,
        completed: Callable[[Any], T],
        failed: Callable[[SdkFailure], None] = _ignore,
    ) -> None:
        self.runtime = runtime
        self.id = job
        self._completed = completed
        self._failed = failed
        self._resolved = False
        self._value: T | None = None
        self._error: SdkFailure | None = None
        self._closed = False

    def status(self) -> WireMap:
        """Read task state and its result envelope; does not finish local Draft transitions."""
        return self.runtime._call(v.OperationJob(job=self.id), _freeze)

    def _finish(self, value: WireMap) -> T | None:
        if value["state"] not in ("succeeded", "failed", "cancelled"):
            return None
        if not self._resolved:
            try:
                self._value = self._completed(unwrap(dict(value["result"])))
            except SdkFailure as error:
                self._error = error
                self._failed(error)
            self._resolved = True
        if self._error is not None:
            raise self._error
        return self._value

    def wait(self, timeout: float | None = None) -> T:
        """Wait and return the projected result, or raise the task failure.

        JobTimeout retains running work. Catch it inside the Job context to wait again.
        Recover OutcomeUnknown using its original exception before further work."""
        if timeout is not None and (not math.isfinite(timeout) or timeout < 0):
            raise ValueError("Invalid timeout")
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
            time.sleep(0.005)

    def blob(self) -> Blob:
        """Lease completed readback bytes and source metadata; close after reading."""
        return self.runtime._call(
            v.OperationReadJobBlob(job=self.id),
            lambda info: Blob(self.runtime, info["blob"], info["length"], info),
        )

    def cancel(self) -> None:
        """Request cancellation; an external publication already performed is not undone."""
        self.runtime._call(v.OperationCancelJob(job=self.id))

    def close(self) -> None:
        """Cancel unfinished work, resolve it and release the owned Job; idempotent."""
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
        def done(_: Any) -> None:
            self._closed = True

        self.runtime._call(v.OperationReleaseJob(job=self.id), done)

    def __enter__(self) -> Self:
        return self

    def __exit__(
        self,
        kind: type[BaseException] | None,
        error: BaseException | None,
        tb: TracebackType | None,
    ) -> None:
        if not isinstance(error, OutcomeUnknown):
            self.close()


class Blob:
    """Owned bulk bytes. Upload and download one bounded chunk per recoverable call."""

    runtime: Runtime
    id: str
    length: int
    info: WireMap

    def __init__(
        self, runtime: Runtime, id: str, length: int, info: WireMap | None = None
    ) -> None:
        self.runtime = runtime
        self.id = id
        self.length = length
        self.info: WireMap = _freeze(info or {})
        self._closed = False

    def append(self, offset: int, payload: bytes) -> int:
        """Write bytes at offset and return the next offset; at most 1 MiB per call."""
        if not isinstance(payload, bytes) or len(payload) > 1 << 20:
            raise ValueError("Payload must be bytes of at most 1 MiB")
        operation = v.OperationAppendBlobBinary(
            blob=self.id,
            offset=offset,
            length=len(payload),
            payload_sha256=hashlib.sha256(payload).hexdigest(),
        )
        return self.runtime._binary(operation, payload, lambda result: int(result[0]))

    def seal(self) -> int:
        """Verify the complete upload and make it immutable; return its length."""
        return int(self.runtime._call(v.OperationSealBlob(blob=self.id)))

    def read(self, offset: int, length: int) -> bytes:
        """Read length bytes from offset; request chunks of at most 1 MiB."""
        return self.runtime._binary(
            v.OperationReadBlobBinary(blob=self.id, offset=offset, length=length),
            b"",
            lambda result: bytes(result[1]),
        )

    def close(self) -> None:
        """Release this bulk handle and storage lease; idempotent."""
        if not self._closed:

            def done(_: Any) -> None:
                self._closed = True

            self.runtime._call(v.OperationReleaseBlob(blob=self.id), done)

    def __enter__(self) -> Self:
        return self

    def __exit__(
        self,
        kind: type[BaseException] | None,
        error: BaseException | None,
        tb: TracebackType | None,
    ) -> None:
        if not isinstance(error, OutcomeUnknown):
            self.close()
