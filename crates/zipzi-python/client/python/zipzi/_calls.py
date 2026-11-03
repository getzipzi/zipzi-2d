"""One complete public return contract across request and transfer recovery."""
from __future__ import annotations

from collections.abc import Callable
from types import MethodType
from typing import Generic, Never, ParamSpec, TypeVar, cast
from ._errors import OutcomeUnknown, PendingRequest, SdkFailure

T = TypeVar("T", covariant=True)
U = TypeVar("U")
P = ParamSpec("P")


def _resolved_call(*_: object) -> Never:
    raise SdkFailure("NO_PENDING_REQUEST", "This public call has already been resolved")


class Call(Generic[T]):
    """Retain owned inputs, raw continuation and the complete return projection.

    The backend owns the immutable wire bytes and transfer stage. A Call never
    invokes its original execute closure during recovery. The same decoder is
    applied once after either normal execution or raw continuation succeeds.
    """
    def __init__(self, operation: str, execute: Callable[[], object], decode: Callable[[object], T], *,
                 resume: Callable[[], object] | None = None, inputs: tuple[object, ...] = (),
                 on_unknown: Callable[[OutcomeUnknown], None] | None = None,
                 on_error: Callable[[BaseException], None] | None = None):
        self.operation, self.inputs = operation, inputs
        self._execute, self._decode, self._resume = execute, decode, resume
        self._explicit_resume = resume is not None
        self._on_unknown, self._on_error = on_unknown, on_error
        self.phase = "prepared"
        self.pending_request: PendingRequest | None = None
        self._bound_resume: Callable[[], object] | None = None

    def run(self) -> T:
        if self.phase != "prepared":
            raise SdkFailure("NO_PENDING_REQUEST", "This public call has already started")
        return self._invoke(self._execute)

    def recover(self, expected: PendingRequest) -> T:
        if self.phase != "unknown" or self.pending_request != expected or self._resume is None:
            raise SdkFailure("NO_PENDING_REQUEST", "This public call has already been resolved")
        return self._invoke(self._resume)

    def _finish(self, phase: str) -> None:
        self.phase, self.pending_request = phase, None
        self._execute, self._resume, self._decode = _resolved_call, None, _resolved_call
        self._bound_resume = self._on_unknown = self._on_error = None
        self.inputs = ()

    def _invoke(self, action: Callable[[], object]) -> T:
        self.phase = "request"
        try:
            value = self._decode(action())
        except OutcomeUnknown as error:
            # An outer public projection captures the inner Call continuation
            # before replacing the exception callback with its own full result.
            if not self._explicit_resume and error._resume is not self._bound_resume and error._resume is not None:
                self._resume = error._resume
            self.phase, self.pending_request = "unknown", error.pending
            expected = error.pending
            self._bound_resume = lambda: self.recover(expected)
            error._resume = self._bound_resume
            if self._on_unknown is not None:
                self._on_unknown(error)
            raise
        except BaseException as error:
            callback = self._on_error
            self._finish("failed")
            if callback is not None:
                callback(error)
            raise
        self._finish("resolved")
        return value


def known(execute: Callable[[], U], resume: Callable[[], U]) -> U:
    """Adapter for existing complete decoders while facades move to generation."""
    operation = execute.__qualname__.split(".<locals>", 1)[0]
    return Call(operation, execute, lambda value: cast(U, value), resume=resume).run()


def project(execute: Callable[[], U], projection: Callable[[U], T], *, operation: str,
            inputs: tuple[object, ...] = ()) -> T:
    return Call(operation, execute, lambda value: projection(cast(U, value)), inputs=inputs).run()


def single_id(result: object) -> str:
    from ._values import WriteResult
    if not isinstance(result, WriteResult) or len(result.ids) != 1:
        raise SdkFailure("INVALID_RESULT", "A create operation must return exactly one ID")
    return result.ids[0]


_REQUEST_METHODS: set[object] = set()


def register_request(method: object) -> None:
    """Only library-owned public method implementations enter this directory."""
    _REQUEST_METHODS.add(getattr(method, "__func__", method))


def recover_call(method: Callable[P, U], /, *args: P.args, **kwargs: P.kwargs) -> U:
    """Call a registered SDK request method once and recover its Call once.

    Parameters exactly match the bound SDK method, preserving its return type.
    Arbitrary functions, lambdas and unregistered wrappers raise TypeError before
    execution. Busy, ordinary failures, budgets and JobTimeout are propagated.
    If recovery is still unknown, retain that exception and its owner to resume
    later; this helper never replays a user function or loops on recovery.
    """
    if not isinstance(method, MethodType) or method.__func__ not in _REQUEST_METHODS:
        raise TypeError("recover_call requires a registered bound SDK request method")
    try:
        return method(*args, **kwargs)
    except OutcomeUnknown as error:
        return cast(U, error.recover())
