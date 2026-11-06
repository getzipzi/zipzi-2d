"""Immutable authoring with explicit sourced Drafts and shared Runtime publication."""
from pathlib import Path
from . import values as values
from .values import Command, ObjectAddress, ObjectKind, Version, DraftSource, QuerySource, Query
from ._kernel import Runtime, Session, Draft, Job, Snapshot
from ._errors import SdkFailure, Busy, OutcomeUnknown, PendingRequest, JobTimeout, recover_call

def local() -> Runtime:
    """Create an owned native host. Closing it releases its sessions without saving."""
    from .backends.local import LocalBackend
    return Runtime(LocalBackend())

def connect(*, endpoint: Path | None=None, start: bool=False, host: Path | None=None,
            timeout: float=10., startup_timeout: float=15., sdk_build: str | None=None) -> Runtime:
    """Attach to a compatible v4 host. Discovery retains private endpoint checks."""
    from ._endpoint import connect_backend
    return Runtime(connect_backend(endpoint,start=start,host=host,timeout=timeout,
        startup_timeout=startup_timeout,sdk_build=sdk_build))

__all__=['local','connect','Runtime','Session','Draft','Job','Snapshot','values','Command',
    'ObjectAddress','ObjectKind','Version','DraftSource','QuerySource','Query','SdkFailure',
    'Busy','OutcomeUnknown','PendingRequest','JobTimeout','recover_call']
