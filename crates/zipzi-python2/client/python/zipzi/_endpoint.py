"""Unix endpoint discovery and ownership transfer to the desktop host."""
import errno
import math
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import threading
import time
from ._errors import SdkFailure
from .backends.connected import ConnectedBackend


def _private(metadata: os.stat_result, directory: bool) -> None:
    if sys.platform == "win32":
        raise RuntimeError("Unix permission check on Windows")
    valid_kind = stat.S_ISDIR(metadata.st_mode) if directory else stat.S_ISREG(metadata.st_mode)
    if not valid_kind or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
        raise PermissionError("Runtime directory/lock must be private, owned by this user and not a symlink")


def _endpoint(path: Path | None, start: bool) -> Path:
    if sys.platform == "win32":
        raise SdkFailure("HOST_UNAVAILABLE", "Local host connections currently require Unix")
    if path is None:
        directory = Path(os.environ.get("ZIPZI_RUNTIME_DIR", f"/tmp/zipzi-runtime-{os.getuid()}"))
        profile = os.environ.get("ZIPZI_RUNTIME_PROFILE", "default")
        path = directory / f"{profile}.sock"
    else:
        directory = path.parent
        profile = path.stem
    if not path.is_absolute() or path.suffix != ".sock" or not re.fullmatch(r"[a-z0-9_-]{1,24}", profile):
        raise ValueError("Endpoint must be an absolute <profile>.sock path with a valid profile")
    if len(os.fsencode(path)) >= 104:
        raise ValueError("Runtime socket path is too long")
    if start:
        try:
            directory.mkdir(mode=0o700)
        except FileExistsError:
            pass
    _private(directory.lstat(), True)
    return path


def _socket(path: Path) -> None:
    if sys.platform == "win32":
        raise RuntimeError("Unix socket check on Windows")
    try:
        metadata = path.lstat()
    except FileNotFoundError:
        return
    if not stat.S_ISSOCK(metadata.st_mode) or metadata.st_uid != os.getuid() or metadata.st_mode & 0o077:
        raise PermissionError("Runtime endpoint must be a private socket owned by this user")


def _host(explicit: Path | None) -> Path:
    configured = os.environ.get("ZIPZI_EDITOR_HOST")
    if explicit is not None or configured:
        candidates = [explicit if explicit is not None else Path(configured or "")]
    elif sys.platform == "darwin":
        candidates = [base / "Zipzi Editor.app/Contents/MacOS/zipzi-editor"
            for base in (Path.home() / "Applications", Path("/Applications"))]
    else:
        candidates = []
    for candidate in candidates:
        if not candidate.is_absolute():
            raise ValueError("Host executable must be absolute")
        if candidate.is_file() and os.access(candidate, os.X_OK):
            return candidate
    raise SdkFailure("HOST_NOT_FOUND", "Set ZIPZI_EDITOR_HOST or pass an absolute host executable")


def _claim(path: Path) -> int | None:
    if sys.platform == "win32":
        raise RuntimeError("Unix ownership claim on Windows")
    import fcntl
    descriptor = os.open(path.with_suffix(".lock"), os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
    try:
        _private(os.fstat(descriptor), False)
        try:
            fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except OSError as error:
            if error.errno in (errno.EAGAIN, errno.EWOULDBLOCK):
                os.close(descriptor)
                return None
            raise
        return descriptor
    except BaseException:
        os.close(descriptor)
        raise


def connect_backend(endpoint: Path | None, *, start: bool, host: Path | None,
    timeout: float, startup_timeout: float, sdk_build: str | None) -> ConnectedBackend:
    for value in (timeout, startup_timeout):
        if isinstance(value, bool) or not math.isfinite(value) or value <= 0:
            raise ValueError("Connection and startup timeouts must be finite and positive")
    if host is not None and not start:
        raise ValueError("A host executable requires start=True")
    if os.name == "nt":
        from ._windows_endpoint import connect
        return connect(endpoint, start=start, host=host, timeout=timeout,
            startup_timeout=startup_timeout, sdk_build=sdk_build)
    path = _endpoint(endpoint, start)
    deadline = time.monotonic() + startup_timeout
    child: subprocess.Popen[bytes] | None = None

    def attempt() -> ConnectedBackend:
        _socket(path)
        remaining = max(.001, deadline - time.monotonic()) if start else timeout
        return ConnectedBackend(path, timeout, sdk_build=sdk_build, handshake_timeout=min(timeout, remaining))

    while True:
        try:
            return attempt()
        except (FileNotFoundError, ConnectionRefusedError):
            if not start:
                raise
        if child is not None and child.poll() is not None:
            raise SdkFailure("HOST_START_FAILED", f"Host exited with status {child.returncode} before handshake")
        if time.monotonic() >= deadline:
            raise TimeoutError("Host startup timed out; endpoint ownership is retained by any running host")
        if child is None:
            descriptor = _claim(path)
            if descriptor is not None:
                try:
                    # Recheck after winning arbitration, before choosing a host.
                    try:
                        return attempt()
                    except (FileNotFoundError, ConnectionRefusedError):
                        pass
                    executable = _host(host)
                    environment = dict(os.environ, ZIPZI_RUNTIME_DIR=str(path.parent), ZIPZI_RUNTIME_PROFILE=path.stem,
                        ZIPZI_RUNTIME_OWNER_FD=str(descriptor))
                    # Development session/readiness files belong to their launcher.
                    for name in ("ZIPZI_DEV_SESSION_DIR", "ZIPZI_DEV_READY_FILE", "ZIPZI_DEV_PID_FILE"):
                        environment.pop(name, None)
                    child = subprocess.Popen([str(executable), "--no-window"], env=environment,
                        pass_fds=(descriptor,), start_new_session=True, stdin=subprocess.DEVNULL,
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                    # Reap without tying the host's lifetime to this client.
                    threading.Thread(target=child.wait, name="zipzi-host-reaper", daemon=True).start()
                finally:
                    # Never LOCK_UN: the child inherits the same open description.
                    os.close(descriptor)
        time.sleep(min(.025, max(0., deadline - time.monotonic())))
