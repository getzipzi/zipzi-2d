"""Handwritten selected values for P1. No native imports or implicit queries."""
from collections.abc import Mapping, Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import cast

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

@dataclass(frozen=True)
class Version:
    session_id: int
    generation: int
    revision: int

    @classmethod
    def read(cls, value: object) -> "Version":
        r = record(value)
        return cls(*(integer(r[k]) for k in ("session_id", "generation", "revision")))

@dataclass(frozen=True)
class Source:
    runtime: str
    session: int
    version: Version
    edit: int | None = None
    sequence: int = 0

    @classmethod
    def read(cls, value: object) -> "Source":
        r = record(value)
        session = record(r["session"])
        if r["kind"] == "formal":
            return cls(string(session["runtime"]), integer(session["serial"]), Version.read(r["version"]))
        if r["kind"] != "edit":
            raise ValueError("Unknown source kind")
        edit = record(r["edit"])
        if session["runtime"] != edit["runtime"]:
            raise ValueError("Source has mixed Runtime identities")
        return cls(string(session["runtime"]), integer(session["serial"]), Version.read(r["base"]),
            integer(edit["serial"]), integer(r["sequence"]))

@dataclass(frozen=True)
class Receipt:
    before: Version
    after: Version
    changed: bool
    label: str
    object_ids: tuple[str, ...]

    @classmethod
    def read(cls, value: object) -> "Receipt":
        r = record(value)
        if type(r["changed"]) is not bool:
            raise TypeError("Expected changed flag")
        return cls(Version.read(r["before"]), Version.read(r["after"]), r["changed"],
            string(r["label"]), tuple(string(v) for v in sequence(r["object_ids"])))

@dataclass(frozen=True)
class SaveReceipt:
    manifest: Path
    durable: bool
    before: Version
    after: Version

    @classmethod
    def read(cls, value: object) -> "SaveReceipt":
        r = record(value)
        if type(r["durable"]) is not bool:
            raise TypeError("Expected durability flag")
        return cls(Path(string(r["manifest"])), r["durable"], Version.read(r["before"]), Version.read(r["after"]))

@dataclass(frozen=True)
class SessionInfo:
    version: Version
    modified: bool
    history: tuple[int, int]

    @classmethod
    def read(cls, value: object) -> "SessionInfo":
        r = record(value)
        history = sequence(r["history_lengths"])
        if type(r["modified"]) is not bool or len(history) != 2:
            raise TypeError("Invalid session info")
        return cls(Version.read(r["version"]), r["modified"], (integer(history[0]), integer(history[1])))

@dataclass(frozen=True)
class WriteResult:
    source: Source
    ids: tuple[str, ...]

    @classmethod
    def read(cls, value: object) -> "WriteResult":
        r = record(value)
        return cls(Source.read(r["source"]), tuple(string(v) for v in sequence(r["ids"])))
