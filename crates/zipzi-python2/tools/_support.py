"""Shared build/measurement ownership for this package's mutable native fixture."""
from contextlib import contextmanager
from pathlib import Path
import sys
import hashlib
import shutil
import os
import tempfile

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / 'tools'))
from workspace.dev_platform import file_lock, venv_python, executable_name
from workspace.workspace_tools import executable

def publish_executable(directory: Path, source: Path) -> Path:
    """Keep running fixtures valid when a subsequent build changes the host."""
    with source.open('rb') as stream:
        version = hashlib.file_digest(stream, 'sha256').hexdigest()
    destination = directory / 'hosts' / version / source.name
    if not destination.exists():
        destination.parent.mkdir(parents=True, exist_ok=True)
        descriptor, temporary = tempfile.mkstemp(dir=destination.parent, prefix='.host-')
        os.close(descriptor)
        try:
            shutil.copy2(source, temporary)
            os.replace(temporary, destination)
        finally:
            Path(temporary).unlink(missing_ok=True)
    return destination

@contextmanager
def exclusive_run(path: Path):
    try:
        with file_lock(path, blocking=False):
            yield
    except BlockingIOError as e:
        raise RuntimeError("Another zipzi-python2 build or measurement is active") from e
