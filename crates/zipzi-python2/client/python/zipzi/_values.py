"""Handwritten selected values for P1. No native imports or implicit queries."""
from collections.abc import Mapping, Sequence
from ._presentation import CompactValue
from dataclasses import dataclass
from pathlib import Path
from typing import cast
from uuid import UUID

Record = Mapping[str, object]

def record(value: object) -> Record:
    if not isinstance(value, dict):
        raise TypeError("Expected a response record")
    return cast(Record, value)

def sequence(value: object) -> Sequence[object]:
    if not isinstance(value, (list, tuple)):
        raise TypeError("Expected a response sequence")
    return cast(Sequence[object], value)

def integer(value: object) -> int:
    if type(value) is not int:
        raise TypeError("Expected an integer")
    return value

def string(value: object) -> str:
    if not isinstance(value, str):
        raise TypeError("Expected a string")
    return value

def number(value: object) -> float:
    if not isinstance(value, (int, float)) or isinstance(value, bool):
        raise TypeError("Expected a number")
    return float(value)

@dataclass(frozen=True, repr=False)
class SessionId(CompactValue):
    """Runtime-scoped identity, independent of paths and document UUIDs."""
    runtime: str
    serial: int

    def __post_init__(self) -> None:
        if str(UUID(self.runtime)) != self.runtime or type(self.serial) is not int or not 0 < self.serial < 1 << 64:
            raise ValueError("Invalid session identity")

    @classmethod
    def _read(cls, value: object) -> "SessionId":
        raw = record(value)
        return cls(string(raw["runtime"]), integer(raw["serial"]))

    def _wire(self) -> dict[str, object]:
        return dict(runtime=self.runtime, serial=self.serial)

@dataclass(frozen=True, repr=False)
class ConnectionLimits(CompactValue):
    frame_bytes: int
    reconnect_seconds: float
    result_seconds: float
    results_per_client: int
    result_bytes_per_client: int
    result_bytes_total: int
    binary_bytes_per_client:int
    binary_bytes_total:int

    @classmethod
    def _read(cls, value: object) -> "ConnectionLimits":
        raw = record(value)
        return cls(integer(raw["frame_bytes"]), number(raw["reconnect_seconds"]), number(raw["result_seconds"]),
            integer(raw["results_per_client"]), integer(raw["result_bytes_per_client"]), integer(raw["result_bytes_total"]),integer(raw["binary_bytes_per_client"]),integer(raw["binary_bytes_total"]))

@dataclass(frozen=True, repr=False)
class ConnectionInfo(CompactValue):
    runtime: str
    host_pid: int
    sdk_build: str
    protocol: str
    capabilities: tuple[str, ...]
    limits: ConnectionLimits

    @classmethod
    def _read(cls, value: object) -> "ConnectionInfo":
        raw = record(value)
        return cls(string(raw["runtime"]), integer(raw["host_pid"]), string(raw["sdk_build"]), string(raw["protocol"]),
            tuple(string(item) for item in sequence(raw["capabilities"])), ConnectionLimits._read(raw["limits"]))

@dataclass(frozen=True, repr=False)
class Version(CompactValue):
    session_id: int
    generation: int
    revision: int

    @classmethod
    def _read(cls, value: object) -> "Version":
        r = record(value)
        return cls(*(integer(r[k]) for k in ("session_id", "generation", "revision")))

@dataclass(frozen=True, repr=False)
class Source(CompactValue):
    runtime: str
    session: int
    version: Version
    edit: int | None = None
    sequence: int = 0

    @classmethod
    def _read(cls, value: object) -> "Source":
        r = record(value)
        session = record(r["session"])
        if r["kind"] == "formal":
            return cls(string(session["runtime"]), integer(session["serial"]), Version._read(r["version"]))
        if r["kind"] != "edit":
            raise ValueError("Unknown source kind")
        edit = record(r["edit"])
        if session["runtime"] != edit["runtime"]:
            raise ValueError("Source has mixed Runtime identities")
        return cls(string(session["runtime"]), integer(session["serial"]), Version._read(r["base"]),
            integer(edit["serial"]), integer(r["sequence"]))

    def _wire(self) -> dict[str, object]:
        version = dict(session_id=self.version.session_id, generation=self.version.generation, revision=self.version.revision)
        result: dict[str, object] = dict(session=dict(runtime=self.runtime, serial=self.session))
        if self.edit is None:
            result.update(kind="formal", version=version)
        else:
            result.update(kind="edit", base=version, edit=dict(runtime=self.runtime, serial=self.edit), sequence=self.sequence)
        return result

@dataclass(frozen=True, repr=False)
class Receipt(CompactValue):
    before: Version
    after: Version
    changed: bool
    label: str
    object_ids: tuple[str, ...]

    @classmethod
    def _read(cls, value: object) -> "Receipt":
        r = record(value)
        if type(r["changed"]) is not bool:
            raise TypeError("Expected changed flag")
        return cls(Version._read(r["before"]), Version._read(r["after"]), r["changed"],
            string(r["label"]), tuple(string(v) for v in sequence(r["object_ids"])))

@dataclass(frozen=True, repr=False)
class SaveReceipt(CompactValue):
    manifest: Path
    durable: bool
    before: Version
    after: Version

    @classmethod
    def _read(cls, value: object) -> "SaveReceipt":
        r = record(value)
        if type(r["durable"]) is not bool:
            raise TypeError("Expected durability flag")
        return cls(Path(string(r["manifest"])), r["durable"], Version._read(r["before"]), Version._read(r["after"]))

@dataclass(frozen=True, repr=False)
class SessionInfo(CompactValue):
    version: Version
    modified: bool
    history: tuple[int, int]
    id: SessionId
    state: str
    path: Path | None

    @classmethod
    def _read(cls, value: object) -> "SessionInfo":
        r = record(value)
        history = sequence(r["history_lengths"])
        if type(r["modified"]) is not bool or len(history) != 2:
            raise TypeError("Invalid session info")
        state = string(r["state"])
        if state not in {"ready", "busy", "faulted", "closing", "closed"}:
            raise ValueError("Unknown session state")
        return cls(Version._read(r["version"]), r["modified"], (integer(history[0]), integer(history[1])),
            SessionId._read(r["id"]), state, None if r["path"] is None else Path(string(r["path"])))

@dataclass(frozen=True, repr=False)
class WriteResult(CompactValue):
    source: Source
    ids: tuple[str, ...]

    @classmethod
    def _read(cls, value: object) -> "WriteResult":
        r = record(value)
        return cls(Source._read(r["source"]), tuple(string(v) for v in sequence(r["ids"])))
