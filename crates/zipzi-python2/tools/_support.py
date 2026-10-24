"""Shared build/measurement ownership for this package's mutable native fixture."""
from contextlib import contextmanager
from pathlib import Path

@contextmanager
def exclusive_run(path: Path):
    import fcntl
    with path.open("a+b") as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as e:
            raise RuntimeError("Another zipzi-python2 build or measurement is active") from e
        try:
            yield
        finally:
            fcntl.flock(lock, fcntl.LOCK_UN)
