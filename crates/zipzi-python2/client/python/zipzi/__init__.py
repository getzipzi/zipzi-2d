"""Typed Python authoring API backed by the Zipzi Runtime."""
from pathlib import Path
from . import modeling as modeling, bindings, sampling, events, meshes, ops, parameters, plans, resources, scene, references
from .plans import Plan
from .resources import PreparedImage
from .buffers import Buffer
from ._errors import Busy, OutcomeUnknown, PendingRequest, SdkFailure
from ._lifecycle import Edit, Job, Runtime, Session
from ._values import ConnectionInfo, ConnectionLimits, Receipt, SaveReceipt, SessionId, SessionInfo, Source, Version, WriteResult

def local() -> Runtime:
    from .backends.local import LocalBackend
    return Runtime(LocalBackend())

def connect(*, endpoint: Path | None = None, start: bool = False, host: Path | None = None,
    timeout: float = 10., startup_timeout: float = 15., sdk_build: str | None = None) -> Runtime:
    """Discover a host, optionally start one, and bind a strictly checked client."""
    from ._endpoint import connect_backend
    return Runtime(connect_backend(endpoint, start=start, host=host, timeout=timeout,
        startup_timeout=startup_timeout, sdk_build=sdk_build))

__all__ = ["local", "connect", "Runtime", "Session", "Edit", "Job", "SdkFailure", "Busy", "OutcomeUnknown",
    "PendingRequest", "Version", "Source", "Receipt", "SaveReceipt", "SessionId", "SessionInfo", "ConnectionInfo", "ConnectionLimits", "WriteResult", "modeling", "bindings", "sampling", "events", "meshes", "ops", "parameters", "plans", "resources", "scene", "references", "Plan", "PreparedImage", "Buffer"]
