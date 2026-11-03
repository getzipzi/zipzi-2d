"""Typed Python authoring API backed by the Zipzi Runtime."""
from pathlib import Path
from . import modeling as modeling, bindings, sampling, events, meshes, ops, parameters, resources, scene, references, animation, spatial, observation, inspection
from ._plans import Plan
from ._generated.plan_values import PlanCost, PlanReport
from .resources import PreparedImage
from .buffers import Buffer
from ._errors import Busy, JobTimeout, OutcomeUnknown, PendingRequest, RecoveryAction, SdkFailure
from ._lifecycle import Edit, EditState, Job, JobState, Runtime, Session
from ._calls import recover_call as recover_call
from ._results import Batch, Page, PageCursor, Snapshot
from ._values import ConnectionInfo, ConnectionLimits, Receipt, SaveReceipt, SessionId, SessionInfo, Source, Version, WriteResult
from . import _values as _values

# Resolve WriteResult's deferred annotation after modeling has finished importing.
_values.ManualMeshTransferReport = modeling.mesh.ManualMeshTransferReport

def local() -> Runtime:
    'Explicitly create an owned in-process Runtime with the paired optional native wheel. Importing zipzi alone does not start it. Use as a context manager; close releases its sessions without saving or connecting to another host.'
    from .backends.local import LocalBackend
    return Runtime(LocalBackend())

def connect(*, endpoint: Path | None = None, start: bool = False, host: Path | None = None,
    timeout: float = 10., startup_timeout: float = 15., sdk_build: str | None = None) -> Runtime:
    """Connect to an explicit/discovered host and return an owned Runtime client.

    endpoint is an absolute private Unix socket path or a local Windows Named
    Pipe name; None discovers it from ZIPZI_RUNTIME_DIR/ZIPZI_RUNTIME_PROFILE.
    On Windows, start=True requires the discovered endpoint (or None).
    start=False never launches a
    process. With start=True, host is an absolute executable, or discovery uses
    ZIPZI_EDITOR_HOST and the supported app locations. host requires start=True.
    timeout is the positive finite per-request transport timeout in seconds;
    startup_timeout is the positive finite total startup/handshake budget.
    sdk_build=None accepts any host build with the same wire protocol major;
    an explicit string additionally requires an exact build match. Optional
    capabilities do not gate connection; inspect runtime.connection.capabilities
    before selecting host services. Installing native never changes the host.

    This blocks for connection/startup, raising ValueError for invalid options,
    PermissionError for unsafe endpoint ownership, OS connection errors when
    no host is listening, or SdkFailure for pairing/startup refusal. Runtime
    resources are bounded by host limits. Close the returned client or use with;
    this releases its leases while formal Connected sessions remain resident.
    A request that later becomes unknown must recover on this original Runtime;
    calling connect again does not replay or resolve that request.

    Example: with zipzi.connect(endpoint=absolute_socket_path) as runtime: ...
    """
    from ._endpoint import connect_backend
    return Runtime(connect_backend(endpoint, start=start, host=host, timeout=timeout,
        startup_timeout=startup_timeout, sdk_build=sdk_build))

__all__ = ["local", "connect", "Runtime", "Session", "Edit", "Job", "SdkFailure", "Busy", "OutcomeUnknown",
    "PendingRequest", "Version", "Source", "Receipt", "SaveReceipt", "SessionId", "SessionInfo", "ConnectionInfo", "ConnectionLimits", "WriteResult", "modeling", "bindings", "sampling", "events", "meshes", "ops", "parameters", "resources", "scene", "references", "Plan", "PreparedImage", "Buffer"]
__all__ += ["animation", "spatial", "observation", "inspection"]

__all__ += ["recover_call", "EditState", "JobState", "JobTimeout", "RecoveryAction"]

from ._request_methods import register as _register_requests
_register_requests()

__all__ += ["Snapshot", "Batch", "Page", "PageCursor"]

__all__ += ["PlanCost", "PlanReport"]

from ._generated.discovery import RuntimeCapabilities, RuntimeLimits, ObjectSummary
__all__ += ["RuntimeCapabilities", "RuntimeLimits", "ObjectSummary"]

from ._documentation import ApiDescription, describe
__all__ += ["ApiDescription", "describe"]

from . import mesh_generation as mesh_generation
__all__ += ["mesh_generation"]
