"""Platform transport and owned-host cleanup for installed acceptance gates."""
import os
import signal
from unittest.mock import patch
import zipzi


def connect(*, endpoint=None, **options):
    """Select a transport endpoint for a logical test directory/profile."""
    if os.name == 'nt' and endpoint is not None:
        from zipzi._windows_pipe import endpoint_name
        with patch.dict(os.environ, ZIPZI_RUNTIME_DIR=str(endpoint.parent), ZIPZI_RUNTIME_PROFILE=endpoint.stem):
            return zipzi.connect(endpoint=endpoint_name(endpoint.parent, endpoint.stem), **options)
    return zipzi.connect(endpoint=endpoint, **options)


def stop(pid):
    if os.name == 'nt':
        import ctypes
        from zipzi._windows_pipe import open_process, wait, close_handle
        handle = open_process(0x100000, False, pid)  # SYNCHRONIZE
        if not handle:
            error = ctypes.get_last_error()
            if error == 87:  # Process already exited.
                return
            raise ctypes.WinError(error)
        try:
            if wait(handle, 0) == 0:
                return
            os.kill(pid, signal.SIGTERM)
            if wait(handle, 5000) != 0:
                raise TimeoutError('Fixture did not exit')
        finally:
            close_handle(handle)
        return
    try:
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
