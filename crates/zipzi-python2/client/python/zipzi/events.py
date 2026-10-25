"""Bounded cursor streams; polling never drains another client's events."""
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Callable, Generic, Literal, Self, TypeVar
from uuid import UUID
from ._errors import SdkFailure
from ._recovery import RecoveryOwner
from ._values import Receipt, SaveReceipt, SessionId, SessionInfo, integer, record, sequence, string
if TYPE_CHECKING:
    from .backends import Backend

@dataclass(frozen=True)
class Cursor:
    runtime: str
    session: SessionId | None
    sequence: int

    def __post_init__(self) -> None:
        if str(UUID(self.runtime)) != self.runtime or type(self.sequence) is not int or not 0 <= self.sequence < 1 << 64:
            raise ValueError("Invalid event cursor")
        if self.session is not None and self.session.runtime != self.runtime:
            raise ValueError("Cursor mixes Runtime identities")

    @classmethod
    def read(cls, raw: object) -> "Cursor":
        value = record(raw)
        return cls(string(value["runtime"]), None if value["session"] is None else SessionId.read(value["session"]), integer(value["sequence"]))

    def wire(self) -> dict[str, object]:
        return dict(runtime=self.runtime, session=None if self.session is None else self.session.wire(), sequence=self.sequence)

@dataclass(frozen=True)
class Origin:
    """Read-only operation/client/job origin, rather than a session token."""
    runtime: str
    serial: int

    @classmethod
    def read(cls, raw: object) -> "Origin":
        value = record(raw)
        return cls(string(value["runtime"]), integer(value["serial"]))

@dataclass(frozen=True)
class _Event:
    cursor: Cursor
    client: Origin | None
    job: Origin | None
    operation: Origin

@dataclass(frozen=True)
class Published(_Event):
    receipt: Receipt
    kind: Literal["published"] = field(default="published", init=False)

@dataclass(frozen=True)
class Saved(_Event):
    receipt: SaveReceipt
    kind: Literal["saved"] = field(default="saved", init=False)

@dataclass(frozen=True)
class StateChanged(_Event):
    info: SessionInfo
    kind: Literal["state_changed"] = field(default="state_changed", init=False)

@dataclass(frozen=True)
class Created(_Event):
    info: SessionInfo
    kind: Literal["created"] = field(default="created", init=False)

@dataclass(frozen=True)
class Closed(_Event):
    info: SessionInfo
    kind: Literal["closed"] = field(default="closed", init=False)

Event = Published | Saved | StateChanged | Created | Closed

def read(raw: object) -> Event:
    value = record(raw)
    cursor = Cursor.read(value["cursor"])
    client = None if value["client"] is None else Origin.read(value["client"])
    job = None if value["job"] is None else Origin.read(value["job"])
    operation = Origin.read(value["operation"])
    if any(origin is not None and origin.runtime != cursor.runtime for origin in (client, job, operation)):
        raise ValueError("Event mixes Runtime identities")
    match value["kind"]:
        case "published":
            return Published(cursor, client, job, operation, Receipt.read(value["value"]))
        case "saved":
            return Saved(cursor, client, job, operation, SaveReceipt.read(value["value"]))
        case "state_changed":
            return StateChanged(cursor, client, job, operation, SessionInfo.read(value["value"]))
        case "created":
            return Created(cursor, client, job, operation, SessionInfo.read(value["value"]))
        case "closed":
            return Closed(cursor, client, job, operation, SessionInfo.read(value["value"]))
        case _:
            raise ValueError("Unknown event kind")

def batch(raw: object, cursor: Cursor) -> tuple[Event, ...]:
    result = tuple(read(item) for item in sequence(raw))
    previous = cursor.sequence
    for event in result:
        if event.cursor.runtime != cursor.runtime or event.cursor.session != cursor.session or event.cursor.sequence != previous + 1:
            raise ValueError("Event stream has an invalid cursor")
        previous = event.cursor.sequence
    return result

def check_limit(limit: int) -> None:
    if type(limit) is not int or not 1 <= limit <= 256:
        raise ValueError("Event page limit must be 1..256")

T = TypeVar("T")

class _Subscription(RecoveryOwner, Generic[T]):
    def __init__(self, backend: "Backend", token: object, expected: SessionId | None, decode: Callable[[object], T]):
        super().__init__(backend)
        self._token, self._expected, self._decode = token, expected, decode
        self._cursor: Cursor | None = None
        self._initial: T

    @property
    def cursor(self) -> Cursor:
        if self._cursor is None:
            raise SdkFailure("NOT_SUBSCRIBED", "Recover the subscription first")
        return self._cursor

    @property
    def initial(self) -> T:
        return self._initial

    def _reset(self, raw: object) -> T:
        value = record(raw)
        cursor = Cursor.read(value["cursor"])
        if cursor.session != self._expected:
            raise ValueError("Subscription has another event stream")
        initial = self._decode(raw)
        self._initial, self._cursor = initial, cursor
        return initial

    def _start(self) -> Self:
        def decode(raw: object) -> Self:
            self._reset(raw)
            return self
        return self._request("subscribe", lambda: self._backend.subscribe(self._token, self), decode)

    def resync(self) -> T:
        """Explicitly replace the initial metadata and cursor atomically."""
        return self._request("resync", lambda: self._backend.subscribe(self._token, self), self._reset)

    def poll(self, *, limit: int = 128) -> tuple[Event, ...]:
        check_limit(limit)
        cursor = self.cursor
        def decode(raw: object) -> tuple[Event, ...]:
            result = batch(raw, cursor)
            if result:
                self._cursor = result[-1].cursor
            return result
        return self._request("poll", lambda: self._backend.events(cursor, limit, self), decode)

class SessionSubscription(_Subscription[SessionInfo]):
    def __init__(self, backend: "Backend", token: object, id: SessionId):
        super().__init__(backend, token, id, lambda raw: SessionInfo.read(record(raw)["info"]))

class RegistrySubscription(_Subscription[tuple[SessionInfo, ...]]):
    def __init__(self, backend: "Backend"):
        super().__init__(backend, None, None, lambda raw: tuple(SessionInfo.read(item) for item in sequence(record(raw)["sessions"])))
