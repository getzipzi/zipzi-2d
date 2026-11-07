"""Authoring failures and explicit recovery of an unknown original call."""

from dataclasses import dataclass
from collections.abc import Callable
from typing import Any, cast


class SdkFailure(Exception):
    """Host failure with a stable code and structured details.

    Inspect code (e.g. STALE_SOURCE, MISSING_OBJECT, RESOURCE_LIMIT) to decide
    whether to re-query, select another object or reduce the request size.
    """

    def __init__(self, code: str, message: str, **details: Any) -> None:
        super().__init__(message)
        self.code = code
        self.details = details

    def __str__(self) -> str:
        return f"[{self.code}] {super().__str__()}"


class Busy(SdkFailure):
    """Confirmed admission refusal; the existing Draft remains usable."""


@dataclass(frozen=True)
class PendingRequest:
    """Identity of an unresolved call; preserve it for original-call recovery."""

    request_id: int
    digest: str
    operation: str


class OutcomeUnknown(SdkFailure):
    """Recover this original call; do not replay the mutation under a fresh ID."""

    def __init__(self, pending: PendingRequest, message: str) -> None:
        super().__init__("OUTCOME_UNKNOWN", message)
        self.pending = pending
        self._resume: Callable[[], Any] | None = None

    def recover(self) -> Any:
        """Resolve this original call, including its result projection and owner state."""
        if self._resume is None:
            raise SdkFailure("NO_RECOVERY_OWNER", "No owning call is available")
        return self._resume()


class JobTimeout(SdkFailure):
    """The Job is still owned and running; wait again or explicitly cancel it."""

    def __init__(self) -> None:
        super().__init__("JOB_TIMEOUT", "Job has not completed")


def unwrap(value: Any) -> Any:
    if not isinstance(value, dict) or (("ok" in value) == ("error" in value)):
        raise ValueError("Invalid response envelope")
    value = cast(dict[str, Any], value)
    if "ok" in value:
        return value["ok"]
    e = value["error"]
    cls = (
        Busy
        if e["code"] in ("PROJECT_BUSY", "DRAFT_BUSY", "JOB_BUSY", "SESSION_BUSY")
        else SdkFailure
    )
    raise cls(
        e["code"],
        e["message"],
        **{k: v for k, v in e.items() if k not in ("code", "message")},
    )


def recover_call(error: OutcomeUnknown) -> Any:
    """Recover a lost reply; result type is that of the original owning call.

    Keep its Runtime/Job/Draft alive. If recovery also raises OutcomeUnknown,
    recover that exception before continuing; never replay the original mutation.
    """
    return error.recover()
