"""Create and animate models through Local or Connected Runtime sessions.

Read the installed workflow guide with
importlib.resources.files('zipzi').joinpath('docs/guide.md').read_text().
Discover a method with describe('Session.query'), command constructors with
describe('values.Command'), and response fields with describe('results').
Edits publish through a Draft; saving is separate. Use Runtime/Job/Blob context
managers and recover OutcomeUnknown with recover_call(error).
"""

from pathlib import Path
from . import values as values
from . import results as results
from .values import (
    Command,
    ObjectAddress,
    ObjectKind,
    Version,
    DraftSource,
    QuerySource,
    Query,
)
from ._kernel import Runtime, Sessions, Session, Draft, Job, Snapshot, Blob
from ._documentation import ApiDescription, describe
from ._errors import (
    SdkFailure,
    Busy,
    OutcomeUnknown,
    PendingRequest,
    JobTimeout,
    recover_call,
)


def local() -> Runtime:
    """Create an owned native host. Closing it releases its sessions without saving."""
    from .backends.local import LocalBackend

    return Runtime(LocalBackend())


def connect(
    *,
    endpoint: Path | None = None,
    start: bool = False,
    host: Path | None = None,
    timeout: float = 10.0,
    startup_timeout: float = 15.0,
    sdk_build: str | None = None,
) -> Runtime:
    """Connect to an Editor's private v4 endpoint; use as a context manager.

    endpoint selects a host explicitly. Without it, discover the running host.
    start=True allows launching host (an absolute Editor executable path) when
    needed. timeout bounds calls; startup_timeout bounds host startup. sdk_build
    optionally pins a build. Install the matching client/native pair for local().
    Closing this client detaches its documents; edits require a separate save.
    """
    from ._endpoint import connect_backend

    return Runtime(
        connect_backend(
            endpoint,
            start=start,
            host=host,
            timeout=timeout,
            startup_timeout=startup_timeout,
            sdk_build=sdk_build,
        )
    )


__all__ = [
    "local",
    "connect",
    "Runtime",
    "Sessions",
    "Session",
    "Draft",
    "Job",
    "Snapshot",
    "Blob",
    "values",
    "results",
    "ApiDescription",
    "describe",
    "Command",
    "ObjectAddress",
    "ObjectKind",
    "Version",
    "DraftSource",
    "QuerySource",
    "Query",
    "SdkFailure",
    "Busy",
    "OutcomeUnknown",
    "PendingRequest",
    "JobTimeout",
    "recover_call",
]
