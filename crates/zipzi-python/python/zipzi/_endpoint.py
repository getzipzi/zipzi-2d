"""One user/profile endpoint and flock ownership shared by desktop and Python."""
from __future__ import annotations
import os
from pathlib import Path
import re
import socket
import stat
import subprocess
import sys
import time
import threading
from ._connection import Connection, ConnectionFailure


def endpoint_paths(*, directory=None, profile=None):
    if os.name != "posix":
        raise ConnectionFailure("UNSUPPORTED_PLATFORM", "Local Editor connections require a Unix host")
    directory = Path(directory or os.environ.get("ZIPZI_RUNTIME_DIR", f"/tmp/zipzi-runtime-{os.getuid()}"))
    profile = profile or os.environ.get("ZIPZI_RUNTIME_PROFILE", "default")
    if not directory.is_absolute() or re.fullmatch(r"[a-z0-9_-]{1,24}", profile) is None:
        raise ValueError("Invalid absolute runtime directory or profile")
    try:
        directory.mkdir(mode=0o700)
    except FileExistsError:
        pass
    info = directory.lstat()
    if not stat.S_ISDIR(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
        raise PermissionError("Runtime directory must be private, owned by this user, and not a symlink")
    endpoint = directory / f"{profile}.sock"
    if len(os.fsencode(endpoint)) >= 104:
        raise ValueError("Runtime socket path is too long")
    return endpoint, directory / f"{profile}.lock"


def _host_executable(host):
    configured = host or os.environ.get("ZIPZI_EDITOR_HOST")
    if configured:
        path = Path(configured)
        if not path.is_absolute():
            raise ValueError("ZIPZI_EDITOR_HOST must be absolute")
        if path.suffix == ".app":
            path = path / "Contents/MacOS/zipzi-editor"
        if not path.is_file() or not os.access(path, os.X_OK):
            raise FileNotFoundError(path)
        return path
    # Explicit installed bundles only; never guess a development binary in cwd.
    candidates = [Path("/Applications/Zipzi.app"), Path.home() / "Applications/Zipzi.app"]
    if sys.platform == "darwin":
        result = subprocess.run(["/usr/bin/mdfind", "kMDItemCFBundleIdentifier == 'dev.zipzi.editor'"], text=True, capture_output=True)
        candidates.extend(Path(p) for p in result.stdout.splitlines())
    executables = {p.resolve() / "Contents/MacOS/zipzi-editor" for p in candidates if p.is_dir()}
    executables = [p for p in executables if p.is_file() and os.access(p, os.X_OK)]
    if len(executables) != 1:
        raise ConnectionFailure("HOST_NOT_FOUND", "Set ZIPZI_EDITOR_HOST to the absolute Editor executable or app bundle")
    return executables[0]


def connect(*, start=True, host=None, directory=None, profile=None, timeout=15., sdk_build=None):
    """Attach or cold-start one resident Editor host. Timeout never spawns twice."""
    endpoint, lock = endpoint_paths(directory=directory, profile=profile)
    import fcntl
    deadline = time.monotonic() + timeout
    attempted = False
    child = None
    while True:
        if time.monotonic() >= deadline:
            raise ConnectionFailure("HOST_NOT_READY", "Host did not complete handshake before timeout; ownership was retained")
        try:
            return Connection(endpoint, timeout=max(.01, deadline-time.monotonic()), sdk_build=sdk_build)
        except (FileNotFoundError, ConnectionRefusedError):
            if not start:
                raise
        # Protocol/build mismatch and live-host transport errors propagate. They
        # are never reasons to start a replacement host.
        if not attempted:
            fd = os.open(lock, os.O_RDWR | os.O_CREAT | os.O_NOFOLLOW | os.O_CLOEXEC, 0o600)
            try:
                info = os.fstat(fd)
                if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_mode & 0o077:
                    raise PermissionError("Unsafe endpoint lock")
                try:
                    fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except BlockingIOError:
                    pass  # Existing owner may be starting slowly.
                else:
                    executable = _host_executable(host)
                    environment = dict(os.environ, ZIPZI_RUNTIME_DIR=str(endpoint.parent),
                        ZIPZI_RUNTIME_PROFILE=endpoint.stem, ZIPZI_RUNTIME_OWNER_FD=str(fd))
                    child = subprocess.Popen([str(executable), "--no-window"], env=environment,
                        pass_fds=(fd,), start_new_session=True, stdin=subprocess.DEVNULL,
                        stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
                    # Reap independently; a client leaving must not kill the host.
                    threading.Thread(target=child.wait, name="zipzi-host-reaper", daemon=True).start()
                    attempted = True  # Never launch another process in this attempt.
            finally:
                os.close(fd)  # Host retains the same locked open-file description.
        if child is not None and child.poll() is not None:
            raise ConnectionFailure("HOST_START_FAILED", f"Host exited before handshake ({child.returncode})")
        if time.monotonic() >= deadline:
            raise ConnectionFailure("HOST_NOT_READY", "Host did not complete handshake before timeout; ownership was retained")
        time.sleep(min(.025, max(0., deadline-time.monotonic())))
