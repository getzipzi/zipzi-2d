"""Immutable reusable host buffers with explicit ownership and recovery."""
from __future__ import annotations
from typing import TYPE_CHECKING
from ._owned import Owned
from ._values import integer
from ._errors import OutcomeUnknown, SdkFailure
if TYPE_CHECKING:
    from .backends import Backend
else:
    Backend = object

MAX_BUFFER_BYTES = 256 << 20
CHUNK = 1 << 20

class Buffer(Owned):
    def __init__(self, backend: Backend, data: bytes):
        super().__init__(backend, "buffer")
        self._data = data
        self._size = len(data)
        self._offset = 0
        self._stage = "create"

    @property
    def size(self) -> int:
        return self._size

    @classmethod
    def _prepare(cls, backend: Backend, data: bytes | bytearray | memoryview) -> Buffer:
        if not isinstance(data, (bytes, bytearray, memoryview)):
            raise TypeError("Expected bytes or a buffer")
        size = data.nbytes if isinstance(data, memoryview) else len(data)
        if size > MAX_BUFFER_BYTES:
            raise ValueError("Buffer exceeds 256 MiB")
        value = cls(backend, bytes(data))
        return value._prepare()

    def _prepare(self) -> Buffer:
        try:
            while self._stage != "ready":
                if self._stage == "create":
                    self._token = self._request("create", lambda: self._backend.buffer_create(self.size, self), lambda raw: raw)
                    self._stage = "append"
                elif self._stage == "append":
                    if self._offset == self.size:
                        self._stage = "seal"
                        continue
                    chunk = self._data[self._offset:self._offset + CHUNK]
                    self._offset = self._request("append", lambda: self._backend.buffer_append(self._token, self._offset, chunk, self), integer)
                else:
                    self._request("seal", lambda: self._backend.buffer_seal(self._token, self), integer)
                    self._data = b""
                    self._stage = "ready"
        except OutcomeUnknown as error:
            expected = error.pending
            def resume() -> Buffer:
                if self.pending_request != expected:
                    raise SdkFailure("NO_PENDING_REQUEST", "Buffer request already resolved")
                return self._prepare()
            error._resume = resume
            raise
        except BaseException as error:
            self._data = b""
            if self._token is not None:
                try:
                    self.close()
                except OutcomeUnknown as cleanup:
                    if isinstance(error, SdkFailure):
                        error.cleanup = cleanup
                    else:
                        error.__context__ = cleanup
                    error.add_note("Buffer cleanup has a recoverable pending close")
                except BaseException as cleanup:
                    error.add_note(f"Buffer cleanup failed: {cleanup}")
            raise
        return self

    def read(self, offset: int = 0, length: int | None = None) -> bytes:
        if self._closed:
            self._check(self._backend)
        if self._stage != "ready":
            raise ValueError("Buffer preparation is incomplete")
        if type(offset) is not int:
            raise TypeError("Expected an integer offset")
        count = self.size - offset if length is None else length
        if type(offset) is not int or type(count) is not int or offset < 0 or count < 0 or offset + count > self.size:
            raise ValueError("Buffer range is invalid")
        def decode(raw: object) -> bytes:
            if not isinstance(raw, bytes) or len(raw) != count:
                raise ValueError("Invalid buffer result")
            return raw
        return self._request(f"read:{offset}:{count}", lambda: self._backend.buffer_read(self._token, offset, count, self), decode)

    def __enter__(self) -> Buffer:
        super().__enter__()
        return self
