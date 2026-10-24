"""Bounded synchronous local transport and explicit request recovery."""
from __future__ import annotations
import json
import socket
import struct
import threading
import time
from pathlib import Path
from ._backend import SdkFailure

CONTRACT = "zipzi-runtime-r3-2"
MAX_FRAME = 16 << 20


class ConnectionFailure(Exception):
    def __init__(self, code, message, **fields):
        super().__init__(message)
        self.code = code
        self.__dict__.update(fields)


class Busy(ConnectionFailure):
    """The operation was not executed. Retry with a new request ID."""


class OutcomeUnknown(ConnectionFailure):
    """Recover the retained request before any further writes."""


def _unwrap(response):
    if "error" not in response:
        return response["ok"]
    fields = dict(response["error"])
    code, message = fields.pop("code"), fields.pop("message")
    if fields.get("category") == "sdk":
        error = SdkFailure(message)
        error.code = code
        for name, value in fields.items():
            if name.endswith("_version") and value is not None:
                value = _version(value)
            setattr(error, name, value)
        raise error
    raise (Busy if code == "BUSY" else ConnectionFailure)(code, message, **fields)


def _version(value):
    if value is None:
        return None
    return tuple(value[key] for key in ("session_id", "generation", "revision"))


def _wire_version(value):
    return None if value is None else dict(zip(("session_id", "generation", "revision"), value, strict=True))


def _read_exact(stream, length):
    data = bytearray()
    while len(data) < length:
        chunk = stream.recv(length - len(data))
        if not chunk:
            raise EOFError("Host disconnected")
        data.extend(chunk)
    return data


def _exchange(stream, value):
    body = json.dumps(value, allow_nan=False, separators=(",", ":")).encode()
    if not 0 < len(body) <= MAX_FRAME:
        raise ValueError("Control frame exceeds 16 MiB")
    stream.sendall(struct.pack(">I", len(body)))
    for offset in range(0, len(body), 4096):
        stream.sendall(body[offset:offset + 4096])
    length, = struct.unpack(">I", _read_exact(stream, 4))
    if not 0 < length <= MAX_FRAME:
        raise ValueError("Invalid response length")
    return json.loads(_read_exact(stream, length), parse_constant=lambda x: (_ for _ in ()).throw(ValueError(x)))


class Connection:
    """One resumable client. Session content remains in the host on close.

    A failed exchange saves its exact request. ``recover()`` reconnects to the
    original instance and replays that ID; expired IDs never execute as new work.
    """
    def __init__(self, endpoint: Path, *, timeout=15.0, sdk_build=None):
        self.endpoint = Path(endpoint)
        self.timeout = timeout
        self.sdk_build = sdk_build
        self.runtime_instance_id = None
        self.client_id = None
        self._secret = None
        self._socket = None
        self._next = 0
        self._pending = None
        self._pending_owner = None
        self._lock = threading.RLock()
        self._connect()

    def _connect(self):
        if self._socket is not None:
            self._socket.close()
            self._socket = None
        stream = socket.socket(socket.AF_UNIX)
        stream.settimeout(self.timeout)
        try:
            stream.connect(str(self.endpoint))
            hello = _unwrap(_exchange(stream, {"protocol": CONTRACT, "runtime": self.runtime_instance_id,
                "client": self.client_id, "secret": self._secret, "sdk_build": self.sdk_build}))
        except BaseException:
            stream.close()
            raise
        self.runtime_instance_id, self.client_id, self._secret = hello["runtime"], hello["client"], hello["secret"]
        self.capabilities, self.limits = tuple(hello["capabilities"]), hello["limits"]
        self.host_pid = hello["host_pid"]
        self._socket = stream

    @property
    def pending_request(self):
        """Exact pending request, copied so callers cannot change retry arguments."""
        return None if self._pending is None else json.loads(json.dumps(self._pending))

    def _send(self, request, owner=None):
        try:
            if self._socket is None:
                self._connect()
            response = _exchange(self._socket, request)
            if not isinstance(response, dict) or not ("ok" in response or "error" in response):
                raise ValueError("Incomplete response")
        except (OSError, EOFError, ValueError, KeyError) as error:
            if self._socket is not None:
                self._socket.close()
                self._socket = None
            self._pending = request
            self._pending_owner = owner
            raise OutcomeUnknown("OUTCOME_UNKNOWN", str(error), request_id=request["request_id"]) from error
        self._pending = None
        self._pending_owner = None
        return _unwrap(response)

    def request(self, operation, *, _owner=None):
        with self._lock:
            if self._pending is not None:
                raise Busy("BUSY", "Request was not sent; recover the connection pending request first", request_id=self._pending["request_id"])
            # Validate/own arguments before allocating or transmitting an ID.
            operation = json.loads(json.dumps(operation, allow_nan=False))
            self._next += 1
            request = {"protocol": CONTRACT, "runtime": self.runtime_instance_id, "client": self.client_id,
                "request_id": self._next, "operation": operation}
            if len(json.dumps(request).encode()) > MAX_FRAME:
                raise ValueError("Control frame exceeds 16 MiB")
            return self._send(request, _owner)

    def recover(self, *, _owner=None):
        with self._lock:
            if self._pending is None:
                raise ConnectionFailure("NO_PENDING_REQUEST", "No request needs recovery")
            if _owner is not None and self._pending_owner is not _owner:
                raise OutcomeUnknown("OUTCOME_UNKNOWN", "Pending request belongs to another operation")
            self._connect()  # Reject restart/expired client before replay.
            return self._send(self._pending, self._pending_owner)

    def request_status(self, request_id):
        """Query an original ID without consuming it or replacing pending state."""
        with self._lock:
            if self._socket is None:
                self._connect()
            try:
                response = _exchange(self._socket, {"protocol": CONTRACT,
                    "runtime": self.runtime_instance_id, "client": self.client_id,
                    "request_id": 0, "operation": {"op": "request_status", "request_id": request_id}})
                if not isinstance(response, dict) or not ("ok" in response or "error" in response):
                    raise ValueError("Incomplete response")
            except (OSError, EOFError, ValueError, KeyError):
                self._socket.close()
                self._socket = None
                raise  # Keep the original request and its owner available for recovery.
            return _unwrap(response)

    def wait_job(self, job, *, _owner=None):
        # A JobId is an accepted operation, not a completed publication. A timeout
        # keeps the JobId in the owning adapter for later terminal queries.
        deadline = time.monotonic() + self.timeout
        while True:
            info = self.request({"op": "job", "job": job}, _owner=_owner)
            if info["result"] is not None:
                result = _unwrap(info["result"])
                # Release happens only after the result is retained by the caller.
                return result
            if time.monotonic() >= deadline:
                raise OutcomeUnknown("JOB_PENDING", "Job has not reached a terminal state", job_id=job)
            time.sleep(0.01)

    def statistics(self):
        return self.request({"op": "statistics"})

    def list_sessions(self):
        return self.request({"op": "list_sessions"})

    def create_session(self, document_id, width=None, height=None, origin=(0., 0.)):
        from ._session import Session
        setup = {"source_canvas": None, "export": {"pixels_per_unit": 1., "bounds": None,
            "padding_fraction": 0.1, "padding_pixels": 50., "minimum_extent": 256.}}
        if width is not None or height is not None:
            if width is None or height is None:
                raise ValueError("Both width and height are required")
            setup["source_canvas"] = {"width": width, "height": height, "origin": {"x": origin[0], "y": origin[1]}, "pixels_per_unit": 1.}
        sid = self.request({"op": "create_session", "document_id": document_id, "setup": setup})
        return Session._from_native(ConnectedBackend(self, sid))

    def attach_session(self, session_id):
        from ._session import Session
        self.request({"op": "attach_session", "session": session_id})
        return Session._from_native(ConnectedBackend(self, session_id))

    def open_project(self, path):
        result = self.request({"op": "open", "path": str(path)})
        if "job" in result:
            result = self.wait_job(result["job"])
        return self.attach_session(result["session"])

    def show_window(self, session=None):
        sid = None if session is None else session._native.session_id
        return self.request({"op": "show_window", "session": sid})

    def close(self):
        if self._socket is not None:
            self._socket.close()
            self._socket = None

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()


class ConnectedBackend:
    """The R3 session adapter; unsupported domains fail explicitly."""
    def __init__(self, connection, session_id):
        self.connection, self.session_id = connection, dict(session_id)

    def __getattr__(self, name):
        raise ConnectionFailure("UNSUPPORTED_OPERATION", f"ConnectedBackend does not yet support {name}")

    def _request(self, op, **fields):
        return self.connection.request({"op": op, "session": self.session_id, **fields})

    def _info(self):
        return self._request("session_info")

    def _query(self, meshes=(), parameters=(), edit=None):
        return self._request("query", meshes=list(meshes), parameters=list(parameters), edit=edit)

    def query_objects(self, meshes, parameters):
        q = self._query(meshes, parameters)
        return (_version(q["version"]), [_mesh(m, q["version"]) for m in q["meshes"]],
            [_parameter(p, q["version"]) for p in q["parameters"]])

    def version(self):
        return _version(self._info()["version"])

    def modified(self):
        return self._info()["modified"]

    def history_lengths(self):
        return tuple(self._info()["history_lengths"])

    def project_path(self):
        return self._info()["path"]

    def document_id(self):
        return self._query()["document_id"]

    def mesh_ids(self):
        return self._query()["mesh_ids"]

    def parameter_ids(self):
        return self._query()["parameter_ids"]

    def mesh(self, mesh_id):
        q = self._query(meshes=[mesh_id])
        return _mesh(q["meshes"][0], q["version"])

    def parameter(self, parameter_id):
        q = self._query(parameters=[parameter_id])
        return _parameter(q["parameters"][0], q["version"])

    def start_edit(self, label, expected_version):
        edit = self._request("begin_edit", label=label, expected=_wire_version(expected_version))
        return _RemoteEdit(self, edit)

    def _publish(self, op):
        result = self._request(op)
        return _version(self.connection.wait_job(result["job"])["after"])

    def undo(self):
        return self._publish("undo")

    def redo(self):
        return self._publish("redo")

    def save(self, path, expected):
        result = self._request("save", path=path, expected=_wire_version(expected))
        saved = self.connection.wait_job(result["job"])
        return saved["manifest"], saved["durable"], saved["warnings"], saved["history_warnings"]

    def subscribe(self):
        return self._request("subscribe")


def _mesh(m, version):
    if m is None:
        return None
    return m["id"], m["name"], m["vertex_ids"], [(p["x"], p["y"]) for p in m["base_positions"]], _version(version)


def _parameter(p, version):
    if p is None:
        return None
    return (p["id"], p["runtime_id"], p["name"], p["minimum"], p["maximum"], p["default_value"], p["repeat"], p["kind"], _version(version))


class _RemoteEdit:
    connected = True

    def __init__(self, backend, token):
        self.backend, self.token = backend, token
        self.pending_operation = None
        self.pending_job = None

    def __getattr__(self, name):
        raise ConnectionFailure("UNSUPPORTED_OPERATION", f"Connected Edit does not yet support {name}")

    @property
    def pending_request(self):
        connection = self.backend.connection
        return connection.pending_request if connection._pending_owner is self else None

    def _request(self, op, **fields):
        return self.backend.connection.request({"op": op, "edit": self.token, **fields}, _owner=self)

    def _apply(self, command):
        self.pending_operation = "apply"
        self._request("apply", commands=[command])
        self.pending_operation = None

    def mesh(self, mesh_id):
        q = self.backend._query(meshes=[mesh_id], edit=self.token)
        return _mesh(q["meshes"][0], q["version"])

    def parameter(self, parameter_id):
        q = self.backend._query(parameters=[parameter_id], edit=self.token)
        return _parameter(q["parameters"][0], q["version"])

    def create_parameter(self, parameter_id, name, minimum, maximum, default_value, repeat, kind, runtime_id):
        self._apply({"op": "create_parameter", "parameter": {"id": parameter_id, "runtime_id": runtime_id or parameter_id,
            "name": name, "minimum": minimum, "maximum": maximum, "default_value": default_value,
            "repeat": repeat, "kind": kind, "decimal_places": 6}})

    def replace_parameter(self, parameter_id, name, minimum, maximum, default_value, repeat, kind, runtime_id):
        self._apply({"op": "replace_parameter", "parameter": {"id": parameter_id,
            "runtime_id": runtime_id if runtime_id is not None else parameter_id,
            "name": name, "minimum": minimum, "maximum": maximum, "default_value": default_value,
            "repeat": repeat, "kind": kind if kind is not None else "normal", "decimal_places": 6},
            "preserve_runtime_id": runtime_id is None, "preserve_kind": kind is None,
            "preserve_decimal_places": True})

    def create_mesh(self, data):
        mesh_id, name, texture, geometry, relations, appearance, drawing = data
        vertex_ids, positions, uvs, triangles = geometry
        self._apply({"op": "create_mesh", "mesh": {
            "id": mesh_id, "runtime_id": mesh_id, "name": name,
            "texture_asset_id": ({"kind": "resolved", "object_id": texture} if texture else {"kind": "unspecified"}), "vertex_ids": vertex_ids,
            "base_positions": [{"x": x, "y": y} for x, y in positions],
            "uvs": [{"x": x, "y": y} for x, y in uvs], "triangles": triangles,
            "part_id": relations[0], "deformer_id": relations[1],
            "appearance": {"opacity": appearance[0], "multiply": appearance[1], "screen": appearance[2]},
            "draw_order": drawing[0], "blend_mode": drawing[1], "enabled": drawing[2],
            "double_sided": drawing[3], "inverted_mask": drawing[4], "masks": drawing[5],
            "raw_blend_mode": drawing[6],
        }})

    def rename_mesh(self, mesh_id, name):
        self._apply({"op": "rename_mesh", "id": mesh_id, "name": name})

    def update_positions(self, mesh_id, vertex_ids, positions):
        self._apply({"op": "update_positions", "id": mesh_id, "vertex_ids": vertex_ids, "positions": positions})

    def erase_object(self, object_id):
        self._apply({"op": "erase_object", "id": object_id})

    def commit(self):
        self.pending_operation = "commit"
        accepted = self._request("commit")
        self.pending_job = accepted["job"]
        return self._finish_commit()

    def _finish_commit(self):
        try:
            result = self.backend.connection.wait_job(self.pending_job, _owner=self)
        except Busy as error:
            # Publication was already admitted; a blocked poll cannot make this
            # edit writable again or permit a second commit request.
            raise OutcomeUnknown("JOB_PENDING", "Recover the connection before polling this commit",
                job_id=self.pending_job) from error
        self.pending_job = None
        self.pending_operation = None
        return _version(result["after"])

    def recover(self):
        connection = self.backend.connection
        result = connection.recover(_owner=self) if connection.pending_request is not None else None
        if self.pending_operation == "commit":
            if self.pending_job is None:
                self.pending_job = result["job"]
            elif result is not None and result.get("result") is not None:
                terminal = _unwrap(result["result"])
                self.pending_job = self.pending_operation = None
                return _version(terminal["after"])
            return self._finish_commit()
        self.pending_operation = None
        return result

    def cancel(self):
        self.pending_operation = "cancel"
        self._request("release_edit")
        self.pending_operation = None

    def abort(self):
        self.cancel()
