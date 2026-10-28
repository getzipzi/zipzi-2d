"""Small platform primitives shared by development and package test tools."""
from contextlib import contextmanager
import os
from pathlib import Path
import time


def venv_python(directory: Path) -> Path:
    return directory / ('Scripts/python.exe' if os.name == 'nt' else 'bin/python')


def executable_name(name: str) -> str:
    return name + ('.exe' if os.name == 'nt' else '')


def clean_python_environment(environment=None):
    result = dict(os.environ if environment is None else environment)
    for name in ('PYTHONPATH', 'PYTHONHOME'):
        result.pop(name, None)
    result['PYTHONUTF8'] = '1'
    return result


@contextmanager
def file_lock(path: Path, *, blocking=True):
    """Lock a persistent byte/inode; process death releases ownership on both OSes."""
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open('a+b') as stream:
        if os.name == 'nt':
            import msvcrt
            # msvcrt.locking locks from the current file position, including
            # beyond EOF. Never append a byte or change the locked byte range.
            while True:
                try:
                    stream.seek(0)
                    msvcrt.locking(stream.fileno(), msvcrt.LK_NBLCK, 1)
                    break
                except OSError as error:
                    if error.errno not in (13, 11, 36):
                        raise
                    if not blocking:
                        raise BlockingIOError('Another development operation is active') from error
                    time.sleep(.05)
            try:
                yield stream
            finally:
                stream.seek(0)
                msvcrt.locking(stream.fileno(), msvcrt.LK_UNLCK, 1)
        else:
            import fcntl
            fcntl.flock(stream, fcntl.LOCK_EX | (0 if blocking else fcntl.LOCK_NB))
            try:
                yield stream
            finally:
                fcntl.flock(stream, fcntl.LOCK_UN)
