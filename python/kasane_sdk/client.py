"""Line-delimited JSON client for the currently open Kasane Document."""
from __future__ import annotations

from contextlib import contextmanager
import json
import socket
from typing import Iterator
from uuid import uuid4


class KasaneError(RuntimeError):
    def __init__(self, code: str, message: str, revision: int | None = None):
        self.code = code
        self.revision = revision
        super().__init__(f"{code}: {message}")


class KasaneClient:
    def __init__(self, host: str = "127.0.0.1", port: int = 43884, timeout: float = 5.0):
        if host not in ("127.0.0.1", "localhost"):
            raise ValueError("Stage 04 client only connects to the local host")
        self._socket = socket.create_connection((host, port), timeout=timeout)
        self._socket.settimeout(timeout)
        self._reader = self._socket.makefile("rb")
        self._draft = False

    def close(self) -> None:
        self._reader.close()
        self._socket.close()

    def __enter__(self) -> "KasaneClient":
        return self

    def __exit__(self, *_: object) -> None:
        self.close()

    def _call(self, method: str, **fields: object) -> dict:
        request = json.dumps({"method": method, **fields}, allow_nan=False, separators=(",", ":"))
        self._socket.sendall(request.encode("utf-8") + b"\n")
        line = self._reader.readline(1048577)
        if not line:
            raise ConnectionError("Kasane session closed the connection")
        if len(line) > 1048576:
            raise ConnectionError("Kasane response exceeds one MiB")
        response = json.loads(line)
        if not isinstance(response, dict):
            raise ConnectionError("Invalid Kasane response")
        if not response.get("ok", False):
            raise KasaneError(response.get("code", "UNKNOWN"), response.get("message", ""), response.get("revision"))
        return response

    def summary(self) -> dict:
        return self._call("summary")

    def get_mesh(self, mesh_id: str) -> dict:
        return self._call("get_mesh", id=mesh_id)

    def get_asset(self, asset_id: str) -> dict:
        return self._call("get_asset", id=asset_id)

    def preview(self, mesh_id: str) -> dict:
        """Read current renderer positions, useful for checking live synchronization."""
        return self._call("preview", id=mesh_id)

    def begin(self, revision: int | None = None) -> dict:
        if revision is None:
            revision = self.summary()["revision"]
        result = self._call("begin", revision=revision)
        self._draft = True
        return result

    def stage_positions(self, mesh_id: str, vertex_ids: list[int], positions: list[list[float]]) -> dict:
        if not self._draft:
            raise KasaneError("NO_TRANSACTION", "Begin a transaction first")
        return self._call("stage_positions", mesh_id=mesh_id, vertex_ids=vertex_ids, positions=positions)

    def commit(self) -> dict:
        try:
            return self._call("commit")
        finally:
            self._draft = False

    def cancel(self) -> dict:
        try:
            return self._call("cancel")
        finally:
            self._draft = False

    @contextmanager
    def transaction(self, revision: int | None = None) -> Iterator["KasaneClient"]:
        self.begin(revision)
        try:
            yield self
        except BaseException:
            self.cancel()
            raise
        else:
            self.commit()

    def create_grid(self, name: str, texture_asset_id: str, columns: int, rows: int,
                    width: float, height: float, *, mesh_id: str | None = None,
                    revision: int | None = None) -> dict:
        if revision is None:
            revision = self.summary()["revision"]
        return self._call("create_grid", id=mesh_id or str(uuid4()), name=name,
                          texture_asset_id=texture_asset_id, columns=columns, rows=rows,
                          width=width, height=height, revision=revision)

    def undo(self) -> dict:
        return self._call("undo")

    def redo(self) -> dict:
        return self._call("redo")
