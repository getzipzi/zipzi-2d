"""Bounded cursor streams; polling never drains another client's events."""
from typing import TYPE_CHECKING, Callable, Generic, Self, TypeVar
from ._errors import SdkFailure
from ._recovery import RecoveryOwner
from ._values import SessionId, SessionInfo, record, sequence
if TYPE_CHECKING:
    from .backends import Backend
else:
    Backend = object

from ._generated.event_values import (
    Cursor as Cursor, Origin as Origin, Published as Published, Saved as Saved,
    StateChanged as StateChanged, Created as Created, Closed as Closed,
    Event as Event, read_event as _read_event,
)

def read(raw: object) -> Event:
    event = _read_event(raw)
    if any(origin is not None and origin.runtime != event.cursor.runtime
           for origin in (event.client, event.job, event.operation)):
        raise ValueError("Event mixes Runtime identities")
    return event

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
        "Return this subscription's cached exact cursor without IO. Recover initial subscription before using it; cursors belong to one Runtime and registry/session stream."
        if self._cursor is None:
            raise SdkFailure("NOT_SUBSCRIBED", "Recover the subscription first")
        return self._cursor

    @property
    def initial(self) -> T:
        'Return the immutable metadata captured at subscribe/resync, without IO. Later poll calls advance the cursor but do not silently refresh this initial value.'
        return self._initial

    def _reset(self, raw: object) -> T:
        value = record(raw)
        cursor = Cursor._read(value["cursor"])
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
        """Atomically recapture metadata and reset this subscriber's cached cursor.

        Returns the new initial Session/registry metadata after RESYNC_REQUIRED;
        it does not replay missed events or drain another subscriber. This is a
        synchronous host read under event/response budgets. Recover a lost reply
        through its original Call before polling again. No model write occurs.
        Example: initial = subscription.resync().
        """
        return self._request("resync", lambda: self._backend.subscribe(self._token, self), self._reset)

    def poll(self, *, limit: int = 128) -> tuple[Event, ...]:
        'Read up to limit events after this independent cursor, with limit in 1..256. Advance only after a complete valid page; gaps require explicit resync. Recover the original call after a lost response.'
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
        super().__init__(backend, token, id, lambda raw: SessionInfo._read(record(raw)["info"]))

class RegistrySubscription(_Subscription[tuple[SessionInfo, ...]]):
    def __init__(self, backend: "Backend"):
        super().__init__(backend, None, None, lambda raw: tuple(SessionInfo._read(item) for item in sequence(record(raw)["sessions"])))

__all__ = ['Cursor', 'Origin', 'Published', 'Saved', 'StateChanged', 'Created', 'Closed', 'Event', 'SessionSubscription', 'RegistrySubscription']
