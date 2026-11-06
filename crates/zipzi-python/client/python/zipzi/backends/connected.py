"""Private local IPC with bounded frames, resume and same-request recovery."""
from typing import Any
import hashlib
import json
from pathlib import Path
import struct
import threading
from .._transport import Stream, stream as create_stream
from .._errors import Busy, OutcomeUnknown, PendingRequest, SdkFailure, unwrap
from ..values import CONTRACT

MAX_FRAME = 16 << 20
MAX_BINARY = 1 << 20
MAX_HEADER = 16 << 10

def _body(value: Any) -> bytes:
    body=json.dumps(value,allow_nan=False,separators=(',',':'),sort_keys=True).encode()
    if not 0<len(body)<=MAX_FRAME: raise ValueError('Control frame exceeds budget')
    return body

def _read(stream: Stream, length: int) -> bytes:
    out=bytearray()
    while len(out)<length:
        block=stream.recv(min(65536,length-len(out)))
        if not block: raise EOFError('Host disconnected')
        out.extend(block)
    return bytes(out)

def _exchange(stream: Stream, body: bytes) -> Any:
    stream.sendall(struct.pack('>I',len(body))+body)
    size,=struct.unpack('>I',_read(stream,4))
    if not 0<size<=MAX_FRAME: raise ValueError('Invalid frame size')
    def invalid(v: str) -> Any: raise ValueError('Non-finite response number: '+v)
    return json.loads(_read(stream,size),parse_constant=invalid)

def _exchange_binary(stream: Stream, body: bytes, payload: bytes) -> tuple[Any, bytes]:
    stream.sendall(struct.pack('>II', (4 + len(body) + len(payload)) | (1 << 31), len(body)) + body + payload)
    framed, = struct.unpack('>I', _read(stream, 4))
    size = framed & 0x7fffffff
    if not framed & (1 << 31) or not 4 < size <= MAX_HEADER + MAX_BINARY + 4:
        raise ValueError('Invalid binary frame size')
    header_size, = struct.unpack('>I', _read(stream, 4))
    if not 0 < header_size <= MAX_HEADER or header_size + 4 > size or size - header_size - 4 > MAX_BINARY:
        raise ValueError('Invalid binary header')
    header = json.loads(_read(stream, header_size))
    payload = _read(stream, size - header_size - 4)
    if not header['has_payload'] and payload:
        raise ValueError('Unexpected binary response bytes')
    return header['response'], payload

class ConnectedBackend:
    def __init__(self, endpoint: Path, timeout: float=10., *, sdk_build: str | None=None, handshake_timeout: float | None=None) -> None:
        self._endpoint=endpoint; self._timeout=timeout; self._sdk_build=sdk_build
        self._runtime: str | None = None
        self._client: str | None = None
        self._secret: str | None = None
        self._stream: Stream | None = None
        self._pending: tuple[PendingRequest, bytes, bytes | None] | None = None
        self._next=0; self._closed=False
        self._lock=threading.RLock()
        self._connect(handshake_timeout)
    def _disconnect(self) -> None:
        if self._stream is not None: self._stream.close(); self._stream=None
    def _connect(self, timeout: float | None=None) -> None:
        if self._closed: raise SdkFailure('HOST_CLOSED','Connection closed')
        self._disconnect(); stream=create_stream(); stream.settimeout(self._timeout if timeout is None else timeout)
        try:
            stream.connect(str(self._endpoint))
            hello=unwrap(_exchange(stream,_body(dict(protocol=CONTRACT,runtime=self._runtime,client=self._client,secret=self._secret,sdk_build=self._sdk_build))))
            if hello['protocol']!=CONTRACT: raise SdkFailure('PROTOCOL_MISMATCH','Host uses another wire major')
            if self._sdk_build is not None and hello['sdk_build']!=self._sdk_build: raise SdkFailure('BUILD_MISMATCH','Host uses another build')
        except BaseException:
            stream.close(); raise
        self._runtime=hello['runtime']; self._client=hello['client']; self._secret=hello['secret']
        self.connection_info: dict[str, Any] = hello
        stream.settimeout(self._timeout); self._stream=stream
    def _send(self, pending: PendingRequest, body: bytes, payload: bytes | None = None) -> Any:
        try:
            if self._stream is None: self._connect()
            assert self._stream is not None
            data = b''
            if payload is None:
                value=_exchange(self._stream,body)
            else:
                value,data=_exchange_binary(self._stream,body,payload)
            if not isinstance(value,dict) or (('ok' in value)==('error' in value)): raise ValueError('Invalid response envelope')
        except (OSError,EOFError,ValueError,TypeError) as e:
            self._disconnect(); self._pending=(pending,body,payload)
            unknown=OutcomeUnknown(pending,str(e)); unknown._resume=self.recover
            raise unknown from e
        self._pending=None
        result=unwrap(value)
        if pending.operation=='release_client':
            self._closed=True
            self._disconnect()
        return result if payload is None else (result, data)
    def request(self, operation: dict[str, Any]) -> Any:
        with self._lock:
            if self._closed: raise SdkFailure('HOST_CLOSED','Connection closed')
            if self._pending is not None: raise Busy('RECOVERY_REQUIRED','Recover the original call before another request')
            request_id=self._next+1
            body=_body(dict(protocol=CONTRACT,runtime=self._runtime,client=self._client,request_id=request_id,operation=operation))
            pending=PendingRequest(request_id,hashlib.sha256(_body(operation)).hexdigest(),operation['op'])
            self._next=request_id
            return self._send(pending,body)
    def binary_request(self, operation: dict[str, Any], payload: bytes = b'') -> tuple[Any, bytes]:
        with self._lock:
            if self._closed: raise SdkFailure('HOST_CLOSED', 'Connection closed')
            if self._pending is not None: raise Busy('RECOVERY_REQUIRED', 'Recover the original call before another request')
            if not isinstance(payload, bytes) or len(payload) > MAX_BINARY: raise ValueError('Binary chunk exceeds budget')
            request_id = self._next + 1
            body = _body(dict(protocol=CONTRACT,runtime=self._runtime,client=self._client,request_id=request_id,operation=operation))
            if len(body) > MAX_HEADER: raise ValueError('Binary header exceeds budget')
            pending = PendingRequest(request_id, hashlib.sha256(_body(operation)).hexdigest(), operation['op'])
            self._next = request_id
            return self._send(pending, body, payload)
    def recover(self) -> Any:
        with self._lock:
            if self._pending is None: raise SdkFailure('NO_PENDING_REQUEST','Original call was already resolved')
            pending,body,payload=self._pending
            # The unchanged envelope can only return its cached outcome, execute
            # a previously unreceived ID, or refuse an expired ID. Never renumber.
            return self._send(pending,body,payload)
    def close(self) -> None:
        with self._lock:
            if self._closed: return
            if self._pending is not None: raise Busy('RECOVERY_REQUIRED','Recover the original call before closing')
            self.request({'op':'release_client'})
