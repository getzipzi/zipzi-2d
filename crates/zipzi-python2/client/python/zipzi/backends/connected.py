"""Bounded synchronous transport: bounded frames, owned request bytes and same-ID recovery.

Host discovery/start ownership lives in _endpoint; request recovery never launches.
"""
from .._generated import binary as typed_binary
import hashlib
import json
from pathlib import Path
import socket
import struct
import threading
import time
from typing import Literal
from .. import _operations as ops
from .._generated.backend_bindings import ConnectedBindings
from .._errors import Busy, JobTimeout, OutcomeUnknown, PendingRequest, SdkFailure, unwrap
from .._values import ConnectionInfo, SessionId, Source, record, sequence
from . import JobKind
from ._transfers import Transfer

CONTRACT = "zipzi-runtime-r5-authoring-1"
MAX_FRAME = 16 << 20
MAX_BINARY_CHUNK = 1 << 20
MAX_BINARY_HEADER = 16 << 10

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
    binary = body.startswith(b"ZZB1")
    wire = body[4:] if binary else body
    limit = MAX_BINARY_CHUNK + MAX_BINARY_HEADER + 4 if binary else MAX_FRAME
    if not 0 < len(wire) <= limit:
        raise ValueError("Control frame exceeds budget")
    stream.sendall(struct.pack(">I", len(wire) | ((1 << 31) if binary else 0)))
    stream.sendall(wire)
    length, = struct.unpack(">I", _read(stream, 4))
    response_binary = bool(length & (1 << 31))
    length &= ~(1 << 31)
    if response_binary != binary or not 0 < length <= limit:
        raise ValueError("Invalid response length")
    def invalid_constant(value: str) -> object:
        raise ValueError(f"Non-finite response number: {value}")
    if not binary:
        return json.loads(_read(stream, length), parse_constant=invalid_constant)
    header_length, = struct.unpack(">I", _read(stream, 4))
    if not 0 < header_length <= MAX_BINARY_HEADER or header_length + 4 > length or length - header_length - 4 > MAX_BINARY_CHUNK:
        raise ValueError("Invalid binary response bounds")
    header = record(json.loads(_read(stream, header_length), parse_constant=invalid_constant))
    response = dict(record(header["response"]))
    payload = _read(stream, length - header_length - 4)
    if header["has_payload"] is True:
        if "ok" not in response:
            raise ValueError("Binary error carried a payload")
        response["ok"] = payload
    elif payload:
        raise ValueError("Unexpected binary response bytes")
    return response

class ConnectedBackend(ConnectedBindings):
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
        self._unknown_error: OutcomeUnknown | None = None
        self._flows: dict[int, Transfer] = {}
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
            metadata = ConnectionInfo._read(hello)
            if metadata.protocol != CONTRACT:
                raise SdkFailure("PROTOCOL_MISMATCH", "Host returned an incompatible protocol")
            if self._sdk_build is not None and metadata.sdk_build != self._sdk_build:
                raise SdkFailure("BUILD_MISMATCH", "Host returned a different SDK build")
            if not {"typed_binary_arrays", "typed_full_modeling", "typed_field_addresses", "read_jobs", "project_io", "resource_jobs", "editor_context", "typed_meshes", "typed_resources_plans", "typed_resource_values", "typed_job_lifecycle", "typed_parameters", "typed_event_pages", "typed_mesh_batch", "typed_mesh_bindings", "typed_modeling_samples", "typed_scene_authoring", "typed_object_references", "binary_blobs"}.issubset(sequence(hello["capabilities"])):
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
            self._unknown_error = OutcomeUnknown(pending, str(e))
            raise self._unknown_error from e
        self._pending = None
        self._unknown_error = None
        if "error" in response:
            fields = dict(record(response["error"]))
            fields.setdefault("operation", pending.operation)
            return unwrap(dict(error=fields))
        return response["ok"]

    def _request(self, value: dict[str, object], owner: object, operation: str) -> object:
        return self._request_message(value, owner, operation, None)

    def _binary_request(self, value: dict[str, object], payload: bytes, owner: object, operation: str) -> object:
        if len(payload) > MAX_BINARY_CHUNK:
            raise ValueError("Binary chunk exceeds budget")
        return self._request_message(value, owner, operation, payload)

    def _request_message(self, value: dict[str, object], owner: object, operation: str, payload: bytes | None) -> object:
        with self._lock:
            if self._closed:
                raise SdkFailure("HOST_CLOSED", "Connection is closed", operation=operation)
            if self._pending is not None:
                raise Busy("BUSY", "Recover the owning pending request first", operation=operation,
                           recovery_action="recover_original")
            operation_bytes = _body(value)
            request_id = self._next + 1
            body = _body(dict(protocol=CONTRACT, runtime=self._runtime, client=self._client,
                request_id=request_id, operation=value))
            if len(body) > (MAX_BINARY_HEADER if payload is not None else MAX_FRAME):
                raise ValueError("Control frame exceeds budget")
            if payload is not None:
                body = b"ZZB1" + struct.pack(">I", len(body)) + body + payload
            pending = PendingRequest(request_id, hashlib.sha256(operation_bytes).hexdigest(), operation)
            self._next = request_id
            return self._send(pending, body, owner)

    def pending(self, owner: object) -> PendingRequest | None:
        with self._lock:
            return self._pending[0] if self._pending is not None and self._pending[2] is owner else None

    def pending_error(self) -> OutcomeUnknown | None:
        with self._lock:
            return self._unknown_error if self._pending is not None else None

    def recover(self, owner: object) -> object:
        if id(owner) in self._flows:
            return self._drive_transfer(self._flows[id(owner)])
        return self._recover_wire(owner)

    def _drive_transfer(self, transfer: Transfer) -> object:
        owner = transfer.owner
        with self._lock:
            if self._closed:
                raise SdkFailure("HOST_CLOSED", "Connection is closed")
            existing = self._flows.get(id(owner))
            if (existing is not None or self.pending(owner) is not None) and existing is not transfer:
                raise Busy("BUSY", "Recover the original transfer before starting another operation")
            self._flows[id(owner)] = transfer
            try:
                return transfer.drive()
            finally:
                if self.pending(owner) is None:
                    self._flows.pop(id(owner), None)

    def _recover_wire(self, owner: object) -> object:
        with self._lock:
            if self._pending is None or self._pending[2] is not owner:
                raise SdkFailure("NO_PENDING_REQUEST", "No request owned by this object needs recovery")
            pending, body, _ = self._pending
            try:
                self._connect()
            except (OSError, EOFError, ValueError, TypeError) as e:
                self._disconnect()
                self._unknown_error = OutcomeUnknown(pending, str(e))
                raise self._unknown_error from e
            return self._send(pending, body, owner)

    def session_id(self, token: object) -> SessionId:
        return SessionId._read(token)
    def show_window(self, session: object, owner: object) -> None:
        self._request(dict(op="show_window", session=session), owner, "session.show_window")
    def _transport_apply(self, edit: object, operations: tuple[ops.WireOperation, ...], owner: object) -> object:
        if len(operations)>1024:
            return self._request(dict(op="apply_authoring",edit=edit,operations=tuple(ops.wire(op) for op in operations)),owner,"edit.apply")
        payload = typed_binary.encode_operations(operations)
        if len(payload) > 65536:
            return self._drive_transfer(Transfer(self,owner,"authoring",data=payload,edit=edit))
        return self._request(dict(op="apply_authoring", edit=edit, operations=[ops.wire(op) for op in operations]), owner, "edit.apply")
    def _transport_prepare_image(self, width: int, height: int, rgba: bytes, owner: object) -> object:
        if len(rgba) > 65536:
            return self._drive_transfer(Transfer(self, owner, "image", data=rgba, width=width, height=height))
        return self._request(dict(op="prepare_image", width=width, height=height, rgba=list(rgba)), owner, "resources.prepare_image")
    def _transport_prepare_image_buffer(self, width: int, height: int, blob: object, owner: object) -> object:
        return self._request(dict(op="prepare_image_blob", width=width, height=height, blob=blob), owner, "resources.prepare_buffer")
    def _transport_buffer_create(self, length: int, owner: object) -> object:
        return self._request(dict(op="create_blob", length=length), owner, "buffer.create")
    def _transport_buffer_append(self, blob: object, offset: int, data: bytes, owner: object) -> object:
        return self._binary_request(dict(op="append_blob_binary", blob=blob, offset=offset,
            length=len(data), payload_sha256=hashlib.sha256(data).hexdigest()), data, owner, "buffer.append")
    def _transport_buffer_seal(self, blob: object, owner: object) -> object:
        return self._request(dict(op="seal_blob", blob=blob), owner, "buffer.seal")
    def _transport_buffer_read(self, blob: object, offset: int, length: int, owner: object) -> object:
        return self._drive_transfer(Transfer(self, owner, "read", blob=blob, offset=offset, length=length))
    def _transport_release_handle(self, kind: Literal["image", "plan", "reference", "address", "buffer"], token: object, owner: object) -> None:
        if kind == "image":
            operation = dict(op="release_prepared_image", image=token)
        elif kind == "plan":
            operation = dict(op="release_modeling_plan", plan=token)
        elif kind == "reference":
            operation = dict(op="release_object_ref", reference=token)
        elif kind == "address":
            operation = dict(op="release_field_address", address=token)
        else:
            operation = dict(op="release_blob", blob=token)
        self._request(operation, owner, f"{kind}.close")
    def _test_control(self, action: str) -> object:
        return self._request(dict(op="test_control", action=action), self, "test.control")
    def _transport_wait(self, job: object, kind: JobKind, owner: object, *, timeout: float | None = None) -> object:
        deadline = None if timeout is None else time.monotonic() + timeout
        while True:
            if kind=="sampled":
                selector=dict(op="job",job=job)
                raw=self._drive_transfer(Transfer(self,owner,"samples",selector=selector))
                if raw is not None: return raw
                if deadline is not None and time.monotonic() >= deadline:
                    raise JobTimeout()
                time.sleep(.005)
                continue
            info = record(self._request(dict(op="job", job=job), owner, "job.wait"))
            if info["result"] is not None:
                result = dict(record(unwrap(info["result"])))
                result["kind"] = kind
                return result
            if deadline is not None and time.monotonic() >= deadline:
                raise JobTimeout()
            time.sleep(.005)

    def _transport_close(self, *, abandon_pending: bool = False) -> None:
        with self._lock:
            if self._closed:
                return
            if self._pending is not None and (abandon_pending or self._pending[2] is not self):
                if not abandon_pending:
                    raise Busy("BUSY", "Recover the original pending call before closing, or explicitly abandon it",
                               operation="runtime.close", recovery_action="recover_original")
                self._closed = True
                self._disconnect()
                self._flows.clear()
                self._pending = None
                self._unknown_error = None
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
            self._flows.clear()


    def _transport_read_selection(self, session: object, edit: object | None, domain: str, ids: tuple[str,...], expected: Source | None, owner: object) -> object:
        selector=dict(op="read_selection",session=session,edit=edit,domain=domain,ids=ids,expected=None if expected is None else expected._wire())
        if domain in {"meshes","bindings","resources"}:
            kind = "images" if domain=="resources" else "meshes" if domain=="meshes" else "bindings"
            return self._drive_transfer(Transfer(self,owner,kind,selector=selector))
        return self._request(selector,owner,"collection.get_many")
