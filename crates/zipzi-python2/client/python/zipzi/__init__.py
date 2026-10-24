"""Typed Python authoring API backed by the Zipzi Runtime."""
from pathlib import Path
from . import meshes, ops, plans, resources
from .plans import Plan
from .resources import PreparedImage
from ._errors import Busy, OutcomeUnknown, PendingRequest, SdkFailure
from ._lifecycle import Edit, Job, Runtime, Session
from ._values import Receipt, SaveReceipt, SessionInfo, Source, Version, WriteResult

def local() -> Runtime:
    from .backends.local import LocalBackend
    return Runtime(LocalBackend())

def connect(*, endpoint: Path, timeout: float = 10.) -> Runtime:
    from .backends.connected import ConnectedBackend
    return Runtime(ConnectedBackend(endpoint, timeout))

__all__ = ["local", "connect", "Runtime", "Session", "Edit", "Job", "SdkFailure", "Busy", "OutcomeUnknown",
    "PendingRequest", "Version", "Source", "Receipt", "SaveReceipt", "SessionInfo", "WriteResult", "meshes", "ops", "plans", "resources", "Plan", "PreparedImage"]
