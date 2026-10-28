"""Pure-Python Win32 byte pipe transport; no Local adapter dependency."""
import ctypes as c
from ctypes import wintypes as w
import hashlib
from pathlib import Path
import time
from typing import Any

kernel = c.WinDLL('kernel32', use_last_error=True)
advapi = c.WinDLL('advapi32', use_last_error=True)


def api(library: Any, name: str, result: Any, *arguments: Any) -> Any:
    function = getattr(library, name)
    function.restype = result
    function.argtypes = arguments
    return function


close_handle = api(kernel, 'CloseHandle', w.BOOL, w.HANDLE)
local_free = api(kernel, 'LocalFree', w.HANDLE, w.HANDLE)
current_process = api(kernel, 'GetCurrentProcess', w.HANDLE)
open_process = api(kernel, 'OpenProcess', w.HANDLE, w.DWORD, w.BOOL, w.DWORD)
open_token = api(advapi, 'OpenProcessToken', w.BOOL, w.HANDLE, w.DWORD, c.POINTER(w.HANDLE))
token_info = api(advapi, 'GetTokenInformation', w.BOOL, w.HANDLE, c.c_int, c.c_void_p, w.DWORD, c.POINTER(w.DWORD))
sid_string = api(advapi, 'ConvertSidToStringSidW', w.BOOL, c.c_void_p, c.POINTER(c.c_void_p))
create_file = api(kernel, 'CreateFileW', w.HANDLE, w.LPCWSTR, w.DWORD, w.DWORD, c.c_void_p, w.DWORD, w.DWORD, w.HANDLE)
server_pid = api(kernel, 'GetNamedPipeServerProcessId', w.BOOL, w.HANDLE, c.POINTER(w.DWORD))
create_event = api(kernel, 'CreateEventW', w.HANDLE, c.c_void_p, w.BOOL, w.BOOL, w.LPCWSTR)
wait = api(kernel, 'WaitForSingleObject', w.DWORD, w.HANDLE, w.DWORD)


class Overlapped(c.Structure):
    _fields_ = [('internal', c.c_size_t), ('internal_high', c.c_size_t),
                ('offset', w.DWORD), ('offset_high', w.DWORD), ('event', w.HANDLE)]


read_file = api(kernel, 'ReadFile', w.BOOL, w.HANDLE, c.c_void_p, w.DWORD, c.POINTER(w.DWORD), c.POINTER(Overlapped))
write_file = api(kernel, 'WriteFile', w.BOOL, w.HANDLE, c.c_void_p, w.DWORD, c.POINTER(w.DWORD), c.POINTER(Overlapped))
cancel = api(kernel, 'CancelIoEx', w.BOOL, w.HANDLE, c.POINTER(Overlapped))
completed = api(kernel, 'GetOverlappedResult', w.BOOL, w.HANDLE, c.POINTER(Overlapped), c.POINTER(w.DWORD), w.BOOL)


def token_sid(process: int) -> str:
    token = w.HANDLE()
    if not open_token(process, 8, c.byref(token)):
        raise c.WinError(c.get_last_error())
    try:
        size = w.DWORD()
        token_info(token, 1, None, 0, c.byref(size))
        if not size.value:
            raise c.WinError(c.get_last_error())
        buffer = c.create_string_buffer(size.value)
        if not token_info(token, 1, buffer, size, c.byref(size)):
            raise c.WinError(c.get_last_error())
        # TOKEN_USER begins with SID_AND_ATTRIBUTES, whose first field is PSID.
        sid = c.cast(buffer, c.POINTER(c.c_void_p))[0]
        text = c.c_void_p()
        if not sid_string(sid, c.byref(text)):
            raise c.WinError(c.get_last_error())
        try:
            assert text.value is not None
            return c.wstring_at(text.value)
        finally:
            local_free(text)
    finally:
        close_handle(token)


def current_sid() -> str:
    return token_sid(current_process())


def endpoint_name(directory: Path, profile: str) -> Path:
    normalized = str(directory).replace('\\', '/').lower().rstrip('/')
    scope = hashlib.sha256(normalized.encode()).hexdigest()[:32]
    return Path(f'\\\\.\\pipe\\zipzi-{current_sid()}-{scope}-{profile}')


class Pipe:
    def __init__(self) -> None:
        self._handle: int | None = None
        self._timeout = 10.

    def settimeout(self, timeout: float) -> None:
        self._timeout = timeout

    def connect(self, endpoint: str) -> None:
        if not endpoint.startswith('\\\\.\\pipe\\zipzi-'):
            raise ValueError('Expected a local Zipzi Named Pipe endpoint')
        deadline = time.monotonic() + self._timeout
        while True:
            handle = create_file(endpoint, 0xC0000000, 0, None, 3, 0x40000000 | 0x100000 | 0x10000, None)
            if handle != c.c_void_p(-1).value:
                break
            error = c.get_last_error()
            if error != 231:  # ERROR_PIPE_BUSY; missing endpoint is handled by discovery.
                raise c.WinError(error)
            if time.monotonic() >= deadline:
                raise TimeoutError('Named Pipe connection timed out')
            time.sleep(.01)
        self._handle = handle
        try:
            pid = w.DWORD()
            if not server_pid(handle, c.byref(pid)):
                raise c.WinError(c.get_last_error())
            process = open_process(0x1000, False, pid.value)
            if not process:
                raise c.WinError(c.get_last_error())
            try:
                if token_sid(process) != current_sid():
                    raise PermissionError('Named Pipe server belongs to another user')
            finally:
                close_handle(process)
        except BaseException:
            self.close()
            raise

    def close(self) -> None:
        if self._handle is not None:
            close_handle(self._handle)
            self._handle = None

    def _io(self, buffer: Any, size: int, writing: bool, timeout: float | None = None) -> int:
        if self._handle is None:
            raise OSError('Named Pipe is closed')
        event = create_event(None, True, False, None)
        if not event:
            raise c.WinError(c.get_last_error())
        operation = Overlapped()
        operation.event = event
        count = w.DWORD()
        try:
            function = write_file if writing else read_file
            if not function(self._handle, buffer, size, None, c.byref(operation)):
                error = c.get_last_error()
                if error != 997:  # ERROR_IO_PENDING
                    raise c.WinError(error)
                budget = self._timeout if timeout is None else timeout
                status = wait(event, max(1, min(0xFFFFFFFE, int(budget * 1000))))
                if status != 0:
                    error = c.get_last_error()
                    cancel(self._handle, c.byref(operation))
                    completed(self._handle, c.byref(operation), c.byref(count), True)
                    if status == 258:
                        raise TimeoutError('Named Pipe I/O timed out')
                    raise c.WinError(error)
            if not completed(self._handle, c.byref(operation), c.byref(count), False):
                raise c.WinError(c.get_last_error())
            return count.value
        finally:
            close_handle(event)

    def recv(self, size: int) -> bytes:
        buffer = c.create_string_buffer(size)
        count = self._io(buffer, size, False)
        return buffer.raw[:count]

    def sendall(self, data: bytes) -> None:
        offset = 0
        deadline = time.monotonic() + self._timeout
        while offset < len(data):
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise TimeoutError('Named Pipe write timed out')
            chunk = data[offset:offset + 65536]
            buffer = c.create_string_buffer(chunk)
            count = self._io(buffer, len(chunk), True, remaining)
            if not count:
                raise EOFError('Named Pipe closed during write')
            offset += count
