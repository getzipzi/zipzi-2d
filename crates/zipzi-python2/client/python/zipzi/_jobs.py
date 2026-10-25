"""Handwritten Job lifecycle, independent of domain imports and host startup."""
from __future__ import annotations
from collections.abc import Callable
from typing import Generic, Literal, TypeVar, TYPE_CHECKING
import math
import time
from ._errors import Busy, JobTimeout, OutcomeUnknown, SdkFailure
from ._values import record
from ._recovery import RecoveryOwner
from ._calls import known, project
from ._backend_types import JobKind
if TYPE_CHECKING:
    from .backends import Backend
else:
    Backend = object
T=TypeVar('T')

JobState = Literal["queued", "running", "cancelling", "succeeded", "failed", "cancelled"]

class Job(RecoveryOwner, Generic[T]):
    def __init__(self, backend: Backend, token: object, kind: JobKind, decode: Callable[[object], T], on_terminal: Callable[[bool | None], None] | None = None):
        super().__init__(backend)
        self._token, self._kind, self._decode = token, kind, decode
        self._kind: JobKind
        self._result: tuple[T] | None = None
        self._failure: SdkFailure | None = None
        self._on_terminal = on_terminal
        self._terminal_failure = False
        self._deadline: float | None = None

    @classmethod
    def _start(cls, backend: Backend, begin: Callable[[object], object], kind: JobKind,
        decode: Callable[[object], T]) -> "Job[T]":
        job = cls(backend, None, kind, decode)
        def _start() -> "Job[T]":
            job._token = begin(job)
            return job
        def resume() -> "Job[T]":
            job._token = record(backend.recover(job))["job"]
            return job
        return known(start, resume)

    def _request_unknown(self, action: str, error: OutcomeUnknown) -> None:
        super()._request_unknown(action, error)
        if action == "wait" and self._on_terminal is not None:
            self._on_terminal(None)

    def _request_failed(self, action: str, error: BaseException) -> None:
        if action == "wait" and isinstance(error, SdkFailure) and not isinstance(error, (Busy, JobTimeout)):
            self._failure = error
            self._terminal_failure = error.code not in {"REQUEST_EXPIRED", "CLIENT_EXPIRED", "HOST_RESTARTED",
                "HOST_CLOSED", "HOST_PANICKED", "INVALID_REFERENCE", "NO_PENDING_REQUEST", "RESYNC_REQUIRED"}
            if self._on_terminal is not None:
                self._on_terminal(False if self._terminal_failure else None)

    def _remaining(self) -> float | None:
        return None if self._deadline is None else max(0., self._deadline - time.monotonic())

    def _wait_value(self, raw: object) -> T:
        # A recovered sampled transfer can be a poll taken before completion.
        # Its empty result is not a sample; continue polling the accepted job.
        if raw is None and self._kind == "sampled":
            raw = self._backend.wait(self._token, self._kind, self, timeout=self._remaining())
        if "result" in record(raw):
            from ._errors import unwrap
            info = record(raw)
            if info["result"] is None:
                return self._wait_value(self._backend.wait(self._token, self._kind, self, timeout=self._remaining()))
            try:
                raw = unwrap(info["result"])
            except SdkFailure as e:
                self._failure = e
                raise
        result = self._decode(raw)
        self._result = (result,)
        if self._on_terminal is not None:
            self._on_terminal(True)
        return result

    def wait(self, *, timeout: float | None = None) -> T:
        """Wait for and cache the actual terminal result, releasing its slot.

        None waits indefinitely. A finite nonnegative timeout raises JobTimeout
        while preserving the accepted Job. Transport loss raises OutcomeUnknown
        for the complete original wait, which can be recovered without replay.
        A pending cancel/status request must be recovered before waiting.
        """
        if timeout is not None and (isinstance(timeout, bool) or not isinstance(timeout, (int, float))
                or not math.isfinite(timeout) or timeout < 0):
            raise ValueError("timeout must be None or a finite nonnegative number")
        if self._failure is not None:
            raise self._failure
        if self._result is not None:
            return self._result[0]
        if self.pending_request is None:
            self._deadline = None if timeout is None else time.monotonic() + timeout
        return self._request("wait", lambda: self._backend.wait(self._token, self._kind, self,
                            timeout=self._remaining()), self._wait_value)

    def cancel(self) -> None:
        """Request cancellation. Wait still reports the actual entered-IO result."""
        if self._failure is not None and not self._terminal_failure:
            raise self._failure
        if self._result is None and self._failure is None:
            self._request("cancel", lambda: self._backend.cancel_job(self._token, self), lambda _: None)

    def state(self) -> JobState:
        if self._failure is not None:
            if not self._terminal_failure:
                raise self._failure
            return "cancelled" if self._failure.code == "CANCELLED" else "failed"
        if self._result is not None:
            return "succeeded"
        def decode(raw: object) -> JobState:
            if raw not in ("queued", "running", "cancelling", "succeeded", "failed", "cancelled"):
                raise TypeError("Invalid job state")
            return raw
        return self._request("state", lambda: self._backend.job_state(self._token, self), decode)

    def close(self) -> None:
        """Wait for the actual outcome and release its Runtime result slot."""
        project(self.wait, lambda _: None, operation="job.close")

    def __enter__(self) -> "Job[T]":
        return self

    def __exit__(self, kind: object, exception: BaseException | None, traceback: object) -> None:
        try:
            self.close()
        except BaseException as cleanup:
            if exception is None:
                raise
            exception.add_note(f"Job cleanup failed: {cleanup}")

