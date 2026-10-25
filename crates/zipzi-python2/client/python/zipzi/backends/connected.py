"""Bounded synchronous transport: bounded frames, owned request bytes and same-ID recovery.

Host discovery/start ownership lives in _endpoint; request recovery never launches.
"""
import hashlib
import json
from pathlib import Path
import socket
import struct
import threading
import time
from typing import Literal
from .. import ops, plans
from .._generated.backends import ConnectedQueries
from .._errors import Busy, OutcomeUnknown, PendingRequest, SdkFailure, unwrap
from .._values import ConnectionInfo, SessionId, record, sequence
from . import JobKind

CONTRACT = "zipzi-runtime-r4-1"
MAX_FRAME = 16 << 20

def _body(value: object) -> bytes:
    return json.dumps(value, allow_nan=False, separators=(",", ":"), sort_keys=True).encode()

def _read(stream: socket.socket, length: int) -> bytes:
    chunks = bytearray()
    while len(chunks) < length:
        chunk = stream.recv(min(65536, length - len(chunks)))
        if not chunk:
            raise EOFError("Host disconnected")
        chunks.extend(chunk)
    return bytes(chunks)

def _exchange(stream: socket.socket, body: bytes) -> object:
    if not 0 < len(body) <= MAX_FRAME:
        raise ValueError("Control frame exceeds budget")
    stream.sendall(struct.pack(">I", len(body)))
    stream.sendall(body)
    length, = struct.unpack(">I", _read(stream, 4))
    if not 0 < length <= MAX_FRAME:
        raise ValueError("Invalid response length")
    def invalid_constant(value: str) -> object:
        raise ValueError(f"Non-finite response number: {value}")
    return json.loads(_read(stream, length), parse_constant=invalid_constant)

class ConnectedBackend(ConnectedQueries):
    def __init__(self, endpoint: Path, timeout: float = 10., *, sdk_build: str | None = None,
        handshake_timeout: float | None = None) -> None:
        if not endpoint.is_absolute():
            raise ValueError("Endpoint must be absolute")
        self._endpoint, self._timeout = endpoint, timeout
        self._sdk_build = sdk_build
        self._runtime: object = None
        self._client: object = None
        self._secret: object = None
        self._stream: socket.socket | None = None
        self._closed = False
        self._next = 0
        self._pending: tuple[PendingRequest, bytes, object] | None = None
        self._lock = threading.RLock()
        self._connect(handshake_timeout)

    def _disconnect(self) -> None:
        if self._stream is not None:
            self._stream.close()
            self._stream = None

    def _connect(self, timeout: float | None = None) -> None:
        if self._closed:
            raise SdkFailure("HOST_CLOSED", "Connection is closed")
        self._disconnect()
        stream = socket.socket(socket.AF_UNIX)
        stream.settimeout(self._timeout if timeout is None else timeout)
        try:
            stream.connect(str(self._endpoint))
            hello = record(unwrap(_exchange(stream, _body(dict(protocol=CONTRACT, runtime=self._runtime,
                client=self._client, secret=self._secret, sdk_build=self._sdk_build)))))
            metadata = ConnectionInfo.read(hello)
            if metadata.protocol != CONTRACT:
                raise SdkFailure("PROTOCOL_MISMATCH", "Host returned an incompatible protocol")
            if self._sdk_build is not None and metadata.sdk_build != self._sdk_build:
                raise SdkFailure("BUILD_MISMATCH", "Host returned a different SDK build")
            if not {"typed_meshes", "typed_resources_plans", "typed_job_lifecycle"}.issubset(sequence(hello["capabilities"])):
                raise SdkFailure("CONTRACT_MISMATCH", "Host lacks typed authoring operations")
        except BaseException:
            stream.close()
            raise
        self._runtime, self._client, self._secret = hello["runtime"], hello["client"], hello["secret"]
        self.connection_info = metadata
        stream.settimeout(self._timeout)
        self._stream = stream

    def _send(self, pending: PendingRequest, body: bytes, owner: object) -> object:
        try:
            if self._stream is None:
                self._connect()
            assert self._stream is not None
            response = record(_exchange(self._stream, body))
            if ("ok" in response) == ("error" in response):
                raise ValueError("Invalid response envelope")
        except (OSError, EOFError, ValueError, TypeError) as e:
            self._disconnect()
            self._pending = pending, body, owner
            raise OutcomeUnknown(pending, str(e)) from e
        self._pending = None
        if "error" in response:
            fields = dict(record(response["error"]))
            fields.setdefault("operation", pending.operation)
            return unwrap(dict(error=fields))
        return response["ok"]

    def _request(self, value: dict[str, object], owner: object, operation: str) -> object:
        with self._lock:
            if self._closed:
                raise SdkFailure("HOST_CLOSED", "Connection is closed", operation=operation)
            if self._pending is not None:
                raise Busy("BUSY", "Recover the owning pending request first", operation=operation)
            operation_bytes = _body(value)
            request_id = self._next + 1
            body = _body(dict(protocol=CONTRACT, runtime=self._runtime, client=self._client,
                request_id=request_id, operation=value))
            if len(body) > MAX_FRAME:
                raise ValueError("Control frame exceeds budget")
            pending = PendingRequest(request_id, hashlib.sha256(operation_bytes).hexdigest(), operation)
            self._next = request_id
            return self._send(pending, body, owner)

    def pending(self, owner: object) -> PendingRequest | None:
        with self._lock:
            return self._pending[0] if self._pending is not None and self._pending[2] is owner else None

    def recover(self, owner: object) -> object:
        with self._lock:
            if self._pending is None or self._pending[2] is not owner:
                raise SdkFailure("NO_PENDING_REQUEST", "No request owned by this object needs recovery")
            pending, body, _ = self._pending
            try:
                self._connect()
            except (OSError, EOFError, ValueError, TypeError) as e:
                self._disconnect()
                raise OutcomeUnknown(pending, str(e)) from e
            return self._send(pending, body, owner)

    def create(self, document_id: str, owner: object) -> object:
        # ModelSetup defaults are supplied by serde/SDK, not copied into Python.
        return self._request(dict(op="create_session", document_id=document_id), owner, "sessions.create")
    def session_id(self, token: object) -> SessionId:
        return SessionId.read(token)
    def list_sessions(self, owner: object) -> object:
        return self._request(dict(op="list_sessions"), owner, "sessions.list")
    def attach(self, session: SessionId, owner: object) -> object:
        return self._request(dict(op="attach_session", session=session.wire()), owner, "sessions.attach")
    def show_window(self, session: object, owner: object) -> None:
        self._request(dict(op="show_window", session=session), owner, "session.show_window")
    def info(self, session: object, owner: object) -> object:
        return self._request(dict(op="session_info", session=session), owner, "session.info")
    def begin(self, session: object, label: str, owner: object) -> object:
        return self._request(dict(op="begin_edit", session=session, label=label), owner, "edit.begin")
    def apply(self, edit: object, operations: tuple[ops.Operation, ...], owner: object) -> object:
        return self._request(dict(op="apply_authoring", edit=edit, operations=[ops.wire(op) for op in operations]), owner, "edit.apply")
    def prepare_image(self, width: int, height: int, rgba: bytes, owner: object) -> object:
        return self._request(dict(op="prepare_image", width=width, height=height, rgba=list(rgba)), owner, "resources.prepare_image")
    def prepared_pixels(self, image: object, owner: object) -> object:
        return self._request(dict(op="prepared_pixels", image=image), owner, "resources.pixels")
    def image(self, session: object, id: str, edit: object | None, owner: object) -> object:
        return self._request(dict(op="image_snapshot", session=session, edit=edit, id=id), owner, "resources.get")
    def plan(self, session: object, edit: object | None, intent: plans.Intent, owner: object) -> object:
        return self._request(dict(op="plan_authoring", session=session, edit=edit,
            intent=dict(kind=intent.kind, id=intent.id, expected_name=intent.expected_name, name=intent.name)), owner, "plan.create")
    def release_handle(self, kind: Literal["image", "plan"], token: object, owner: object) -> None:
        operation = dict(op="release_prepared_image", image=token) if kind == "image" else dict(op="release_modeling_plan", plan=token)
        self._request(operation, owner, f"{kind}.close")
    def statistics(self, owner: object) -> object:
        return self._request(dict(op="statistics"), owner, "resources.usage")
    def release(self, edit: object, owner: object) -> None:
        self._request(dict(op="release_edit", edit=edit), owner, "edit.cancel")
    def commit(self, edit: object, owner: object) -> object:
        return record(self._request(dict(op="commit", edit=edit), owner, "edit.commit"))["job"]
    def save(self, session: object, path: Path, owner: object) -> object:
        return record(self._request(dict(op="save", session=session, path=str(path)), owner, "project.save"))["job"]
    def open(self, path: Path, owner: object) -> object:
        return self._request(dict(op="open", path=str(path)), owner, "sessions.open")
    def history(self, session: object, redo: bool, owner: object) -> object:
        return record(self._request(dict(op="redo" if redo else "undo", session=session), owner, "session.history"))["job"]
    def cancel_job(self, job: object, owner: object) -> None:
        self._request(dict(op="cancel_job", job=job), owner, "job.cancel")
    def job_state(self, job: object, owner: object) -> object:
        return self._request(dict(op="job_status", job=job), owner, "job.state")
    def _test_control(self, action: str) -> object:
        return self._request(dict(op="test_control", action=action), self, "test.control")
    def wait(self, job: object, kind: JobKind, owner: object) -> object:
        deadline = time.monotonic() + self._timeout
        while True:
            info = record(self._request(dict(op="job", job=job), owner, "job.wait"))
            if info["result"] is not None:
                result = dict(record(unwrap(info["result"])))
                result["kind"] = kind
                return result
            if time.monotonic() >= deadline:
                raise OutcomeUnknown(PendingRequest(0, "", "job.wait"), "Accepted Job is still pending; wait again")
            time.sleep(.005)

    def close(self) -> None:
        with self._lock:
            if self._closed:
                return
            # An explicit shutdown with another unresolved owner abandons its
            # connection; grace expiry reclaims it without a new write ID.
            if self._pending is not None and self._pending[2] is not self:
                self._closed = True
                self._disconnect()
                return
            try:
                if self.pending(self) is not None:
                    self.recover(self)
                else:
                    self._request(dict(op="release_client"), self, "runtime.close")
            except OutcomeUnknown as e:
                e._resume = self.close
                raise
            self._closed = True
            self._disconnect()
