"""Backend-independent exceptions. Importing these never imports PyO3."""
from __future__ import annotations
from collections.abc import Callable
from dataclasses import dataclass
from typing import Literal
from ._values import Version, record, sequence, string
from ._generated.values.project import ProjectReport

RecoveryAction = Literal["retry", "reduce_request", "release_resources", "recover_original",
                         "restart_edit", "refresh_source", "resync", "wait_job", "stop"]

def _recovery_action(code: str, edit_state: str | None, limit_name: str | None) -> RecoveryAction:
    if edit_state in {"aborted", "failed"}:
        return "restart_edit"
    if code == "OUTCOME_UNKNOWN":
        return "recover_original"
    if code == "JOB_TIMEOUT":
        return "wait_job"
    if code == "BUSY":
        return "retry"
    if code in {"BUDGET_EXCEEDED", "RESOURCE_BUDGET_EXCEEDED"}:
        return "release_resources" if limit_name in {"jobs_per_client", "jobs_total", "results_per_client", "plans_per_client", "prepared_images_per_client", "references_per_client", "blobs_per_client", "jobs and retained results", "plans", "prepared images", "object references"} else "reduce_request"
    if code in {"STALE_SOURCE", "STALE_PLAN"}:
        return "refresh_source"
    if code == "RESYNC_REQUIRED":
        return "resync"
    return "stop"

class SdkFailure(Exception):
    """Confirmed SDK/host refusal with structured recovery and ownership facts.

    code is the stable discriminator; message is human-readable. operation and
    object_ids identify the attempted domain action. expected_version,
    actual_version, field_path and referrers explain stale/invalid references.
    edit_state distinguishes aborted candidates from confirmed admission refusal.
    requested/allowed are counts or bytes named by limit_name; None means absent.
    project_report retains a failed IO report when available. recovery_action
    recommends a finite next action; it is not permission to replay unknown work.
    OutcomeUnknown has a separate retained request owner and recover method.
    This value alone performs no IO and owns no resource lease. Catch outside a
    failed Edit, inspect code/recovery_action, and start a fresh candidate when
    required. See docs/guide.md for the error/recovery workflow.
    """
    def __init__(self, code: str, message: str, *, operation: str = "", object_ids: tuple[str, ...] = (),
        expected_version: Version | None = None, actual_version: Version | None = None,
        field_path: str | None = None, referrers: tuple[str, ...] = (), edit_state: str | None = None, project_report: ProjectReport | None = None,
        recovery_action: RecoveryAction | None = None, requested: int | None = None,
        allowed: int | None = None, limit_name: str | None = None):
        super().__init__(message)
        self.code, self.operation, self.object_ids = code, operation, object_ids
        self.expected_version, self.actual_version = expected_version, actual_version
        self.field_path, self.referrers, self.edit_state = field_path, referrers, edit_state
        self.project_report = project_report
        self.requested, self.allowed, self.limit_name = requested, allowed, limit_name
        self.recovery_action = recovery_action or _recovery_action(code, edit_state, limit_name)
        self.cleanup: OutcomeUnknown | None = None

class Busy(SdkFailure):
    """Confirmed refusal before execution; the caller may submit a new request."""

@dataclass(frozen=True)
class PendingRequest:
    'Immutable identity of the original request whose outcome is unknown; recover through its owning call.\n\nFields:\nrequest_id: Original client request number; do not allocate a fresh write request to probe the outcome.\ndigest: Digest binding the request identity to its original payload; not the payload itself.\noperation: Original protocol operation name used for diagnostics and recovery ownership.'
    request_id: int
    digest: str
    operation: str

class OutcomeUnknown(SdkFailure):
    """The original public call has an unconfirmed outcome; retain this exception.

    pending identifies its immutable request, while the owning SDK wrapper keeps
    the complete result projection. recover returns that original runtime type;
    use recover_call for exact static typing. Further uncertainty raises another
    recoverable OutcomeUnknown. Never repeat a write under a new request ID.
    Recovery is synchronous, bounded by host retention/timeouts; expiration is a
    structured failure rather than proof the write did not happen.
    """
    def __init__(self, pending: PendingRequest, message: str):
        super().__init__("OUTCOME_UNKNOWN", message, operation=pending.operation)
        self.pending = pending
        self._resume: Callable[[], object] | None = None

    def recover(self) -> object:
        """Recover this complete public call using its original request identity.

        The result has the original method's runtime type. The exception cannot
        express that method's static type; use zipzi.recover_call for typed code.
        This never invokes the original method again. Repeated recovery after
        resolution raises NO_PENDING_REQUEST.
        """
        if self._resume is None:
            raise SdkFailure("NO_RECOVERY_OWNER", "No owning operation is available")
        return self._resume()

class JobTimeout(SdkFailure):
    """The accepted Job remains pending; retain it and wait again or cancel.

    This confirmed nonterminal status has no PendingRequest and is distinct
    from a lost transport reply. A timeout does not cancel the task.
    """
    def __init__(self, message: str = "Accepted Job is still pending"):
        super().__init__("JOB_TIMEOUT", message, operation="job.wait", recovery_action="wait_job")


def unwrap(value: object) -> object:
    r = record(value)
    if "error" not in r:
        return r["ok"]
    e = record(r["error"])
    code = string(e["code"])
    def optional(key: str) -> Version | None:
        return None if e.get(key) is None else Version._read(e[key])
    report=None
    if e.get("project_report") is not None:
        from ._generated.codecs.project import read_ProjectReport
        report=read_ProjectReport(e["project_report"])
    raise (Busy if code == "BUSY" else SdkFailure)(code, string(e["message"]),
        operation=string(e.get("operation", "")), object_ids=tuple(string(v) for v in sequence(e.get("object_ids", ()))),
        expected_version=optional("expected_version"), actual_version=optional("actual_version"),
        field_path=None if e.get("field_path") is None else string(e["field_path"]),
        referrers=tuple(string(v) for v in sequence(e.get("referrers", ()))),
        edit_state=None if e.get("edit_state") is None else string(e["edit_state"]), project_report=report,
        requested=None if e.get("requested") is None else _limit_integer(e["requested"]),
        allowed=None if e.get("allowed") is None else _limit_integer(e["allowed"]),
        limit_name=None if e.get("limit_name") is None else string(e["limit_name"]))


def _limit_integer(value: object) -> int:
    if type(value) is not int or value < 0:
        raise TypeError("Host budget details must be nonnegative integers")
    return value
