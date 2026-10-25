"""Backend-independent exceptions. Importing these never imports PyO3."""
from __future__ import annotations
from collections.abc import Callable
from dataclasses import dataclass
from typing import TYPE_CHECKING, cast
from ._values import Version, record, sequence, string
if TYPE_CHECKING:
    from ._lifecycle import Edit, Job, JobState, Session
    from ._values import Receipt, SaveReceipt, SessionInfo, WriteResult
    from .meshes import Snapshot
    from .plans import Plan
    from .buffers import Buffer
    from .modeling import Records as ModelingRecords, CapabilitySnapshot, ProjectReport
    from .references import ObjectRef, LabelAddress, FieldAddress
    from .resources import PreparedImage, ImagePixels, Snapshot as ImageSnapshot, Usage
    from .events import Event, SessionSubscription, RegistrySubscription
    from .bindings import Snapshot as BindingSnapshot, Batch as BindingBatch
    from .sampling import Batch as SampleBatch
    from .parameters import Snapshot as ParameterSnapshot, Batch as ParameterBatch
    RecoveryResult = ObjectRef | LabelAddress | FieldAddress | ModelingRecords | CapabilitySnapshot | Job[Plan] | Job[SampleBatch] | Buffer | bytes | JobState | Plan | PreparedImage | ImagePixels | ImageSnapshot | Usage | Edit | Session | SessionInfo | Snapshot | WriteResult | Receipt | SaveReceipt | Job[Receipt] | Job[SaveReceipt] | Job[Session] | SessionSubscription | RegistrySubscription | tuple[SessionInfo, ...] | tuple[Event, ...] | tuple[Snapshot, ...] | ParameterSnapshot | ParameterBatch | BindingSnapshot | BindingBatch | SampleBatch | None

class SdkFailure(Exception):
    def __init__(self, code: str, message: str, *, operation: str = "", object_ids: tuple[str, ...] = (),
        expected_version: Version | None = None, actual_version: Version | None = None,
        field_path: str | None = None, referrers: tuple[str, ...] = (), edit_state: str | None = None, project_report: ProjectReport | None = None):
        super().__init__(message)
        self.code, self.operation, self.object_ids = code, operation, object_ids
        self.expected_version, self.actual_version = expected_version, actual_version
        self.field_path, self.referrers, self.edit_state = field_path, referrers, edit_state
        self.project_report=project_report
        self.cleanup: OutcomeUnknown | None = None

class Busy(SdkFailure):
    """Confirmed refusal before execution; the caller may submit a new request."""

@dataclass(frozen=True)
class PendingRequest:
    request_id: int
    digest: str
    operation: str

class OutcomeUnknown(SdkFailure):
    def __init__(self, pending: PendingRequest, message: str):
        super().__init__("OUTCOME_UNKNOWN", message, operation=pending.operation)
        self.pending = pending
        self._resume: Callable[[], object] | None = None

    def recover(self) -> RecoveryResult:
        """Resume the owning lifecycle object using the retained request ID."""
        if self._resume is None:
            raise SdkFailure("NO_RECOVERY_OWNER", "No owning operation is available")
        return cast("RecoveryResult", self._resume())

def unwrap(value: object) -> object:
    r = record(value)
    if "error" not in r:
        return r["ok"]
    e = record(r["error"])
    code = string(e["code"])
    def optional(key: str) -> Version | None:
        return None if e.get(key) is None else Version.read(e[key])
    report=None
    if e.get("project_report") is not None:
        from .modeling import read_ProjectReport
        report=read_ProjectReport(e["project_report"])
    raise (Busy if code == "BUSY" else SdkFailure)(code, string(e["message"]),
        operation=string(e.get("operation", "")), object_ids=tuple(string(v) for v in sequence(e.get("object_ids", ()))),
        expected_version=optional("expected_version"), actual_version=optional("actual_version"),
        field_path=None if e.get("field_path") is None else string(e["field_path"]),
        referrers=tuple(string(v) for v in sequence(e.get("referrers", ()))),
        edit_state=None if e.get("edit_state") is None else string(e["edit_state"]),project_report=report)
