"""Windows launch arbitration; host owns its first pipe instance independently."""
import ctypes as c
from ctypes import wintypes as w
import hashlib
import os
from pathlib import Path
import re
import subprocess
import tempfile
import threading
import time
from ._windows_pipe import api, kernel, advapi, current_sid, endpoint_name, close_handle, local_free, wait
from ._errors import SdkFailure
from .backends.connected import ConnectedBackend


class Attributes(c.Structure):
    _fields_ = [('length', w.DWORD), ('descriptor', c.c_void_p), ('inherit', w.BOOL)]


descriptor_from_text = api(advapi, 'ConvertStringSecurityDescriptorToSecurityDescriptorW', w.BOOL,
    w.LPCWSTR, w.DWORD, c.POINTER(c.c_void_p), c.c_void_p)
create_mutex = api(kernel, 'CreateMutexW', w.HANDLE, c.POINTER(Attributes), w.BOOL, w.LPCWSTR)
release_mutex = api(kernel, 'ReleaseMutex', w.BOOL, w.HANDLE)


def connect(endpoint: Path | None, *, start: bool, host: Path | None, timeout: float,
    startup_timeout: float, sdk_build: str | None) -> ConnectedBackend:
    from ._endpoint import _host
    directory = Path(os.environ.get('ZIPZI_RUNTIME_DIR', str(Path(tempfile.gettempdir()) / 'zipzi-runtime')))
    profile = os.environ.get('ZIPZI_RUNTIME_PROFILE', 'default')
    if not directory.is_absolute() or not re.fullmatch(r'[a-z0-9_-]{1,24}', profile):
        raise ValueError('Invalid absolute runtime directory or profile')
    default = endpoint_name(directory, profile)
    path = default if endpoint is None else endpoint
    if start and path != default:
        raise ValueError('Cold start endpoint must match ZIPZI_RUNTIME_DIR and ZIPZI_RUNTIME_PROFILE')
    deadline = time.monotonic() + startup_timeout
    mutex: int | None = None
    acquired = False
    child: subprocess.Popen[bytes] | None = None

    def attempt() -> ConnectedBackend:
        remaining = max(.001, deadline - time.monotonic()) if start else timeout
        return ConnectedBackend(path, timeout, sdk_build=sdk_build, handshake_timeout=min(timeout, remaining))

    try:
        while True:
            try:
                return attempt()
            except (FileNotFoundError, ConnectionRefusedError):
                if not start:
                    raise
            if child is not None and child.poll() is not None:
                raise SdkFailure('HOST_START_FAILED', f'Host exited with status {child.returncode} before handshake')
            if time.monotonic() >= deadline:
                raise TimeoutError('Host startup timed out; any running host retains pipe ownership')
            if mutex is None:
                descriptor = c.c_void_p()
                sid = current_sid()
                if not descriptor_from_text(f'O:{sid}D:P(A;;GA;;;{sid})', 1, c.byref(descriptor), None):
                    raise c.WinError(c.get_last_error())
                try:
                    attributes = Attributes(c.sizeof(Attributes), descriptor, False)
                    key = hashlib.sha256(str(path).encode()).hexdigest()
                    mutex = create_mutex(c.byref(attributes), False, f'Local\\zipzi-start-{key}')
                    if not mutex:
                        raise c.WinError(c.get_last_error())
                finally:
                    local_free(descriptor)
            if not acquired:
                status = wait(mutex, 0)
                if status in (0, 0x80):  # acquired or abandoned after launcher death
                    acquired = True
                elif status != 258:
                    raise c.WinError(c.get_last_error())
            if acquired and child is None:
                try:
                    return attempt()
                except (FileNotFoundError, ConnectionRefusedError):
                    pass
                executable = _host(host)
                environment = dict(os.environ, ZIPZI_RUNTIME_DIR=str(directory), ZIPZI_RUNTIME_PROFILE=profile)
                for name in ('ZIPZI_DEV_SESSION_DIR', 'ZIPZI_DEV_PID_FILE', 'ZIPZI_DEV_READY_FILE', 'ZIPZI_RUNTIME_OWNER_FD'):
                    environment.pop(name, None)
                child = subprocess.Popen([str(executable), '--no-window'], env=environment,
                    creationflags=subprocess.CREATE_NO_WINDOW | subprocess.CREATE_NEW_PROCESS_GROUP,
                    stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                threading.Thread(target=child.wait, daemon=True, name='zipzi-host-reaper').start()
            time.sleep(min(.025, max(0., deadline - time.monotonic())))
    finally:
        if mutex:
            if acquired:
                release_mutex(mutex)
            close_handle(mutex)
