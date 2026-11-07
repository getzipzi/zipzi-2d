"""Read-only response shapes. Runtime values are immutable mappings and tuples.

Query overloads select these shapes automatically. Use Snapshot.records(Type)
to decode object payloads into generated values with attribute access.
"""

from collections.abc import Mapping
from typing import Literal, ReadOnly, TypedDict


class Version(TypedDict):
    """Serialized publication identity; use values.Version.from_wire for a value."""

    session: ReadOnly[str]
    generation: ReadOnly[int]
    revision: ReadOnly[int]


class ObjectAddress(TypedDict):
    """Stable object kind and ID within the snapshot's source."""

    kind: ReadOnly[str]
    id: ReadOnly[str]


class ObjectRecord(TypedDict):
    """Object payload; decode with the generated type matching address.kind."""

    address: ReadOnly[ObjectAddress]
    value: ReadOnly[Mapping[str, object]]


class Objects(TypedDict):
    """Selected records in request order, including duplicate addresses."""

    records: ReadOnly[tuple[ObjectRecord, ...]]


class Collection(Objects):
    """One page. Continue using next_offset and the original Snapshot.source."""

    next_offset: ReadOnly[int | None]


class Summary(TypedDict):
    """Document identity, setup and counts; contains no geometry or image bytes."""

    id: ReadOnly[str]
    setup: ReadOnly[Mapping[str, object]]
    meshes: ReadOnly[int]
    parameters: ReadOnly[int]
    assets: ReadOnly[int]


class Metadata(TypedDict):
    """Document roots; decode each mapping with its generated value's from_wire.

    The source guards read/modify/write of EditorState, DisplayInfo and Model3Settings.
    Like other queries, the response is bounded and contains no geometry or image bytes.
    """

    editor_state: ReadOnly[Mapping[str, object]]
    display_info: ReadOnly[Mapping[str, object]]
    model3_settings: ReadOnly[Mapping[str, object]]
    hierarchy_order: ReadOnly[Mapping[str, object]]
    missing_attachments: ReadOnly[tuple[str, ...]]


class CapabilityReason(TypedDict):
    """Stable reason why an operation is unavailable or unsupported."""

    code: ReadOnly[str]


class CapabilityUnsupported(TypedDict):
    """Operation is unsupported for this collection."""

    Unsupported: ReadOnly[CapabilityReason]


class CapabilityUnavailable(TypedDict):
    """Operation is unavailable in the captured state."""

    Unavailable: ReadOnly[CapabilityReason]


type Capability = Literal["Available"] | CapabilityUnsupported | CapabilityUnavailable


class CollectionCapabilities(TypedDict):
    """Inspect each state's reason before attempting a collection change."""

    create: ReadOnly[Capability]
    move_into: ReadOnly[Capability]
    reorder: ReadOnly[Capability]


class HistoryLimits(TypedDict):
    """Configured history retention targets."""

    max_steps: ReadOnly[int]
    target_bytes: ReadOnly[int]


class History(TypedDict):
    """Undo/Redo counts and retained history accounting."""

    undo_steps: ReadOnly[int]
    redo_steps: ReadOnly[int]
    estimated_bytes: ReadOnly[int]
    over_target: ReadOnly[bool]
    limits: ReadOnly[HistoryLimits]


class Fingerprint(TypedDict):
    """Exact on-disk identity for save preconditions."""

    sha256: ReadOnly[str]
    length: ReadOnly[int]


class SessionOverview(TypedDict):
    """Resident document metadata. Duplicate or absent paths are possible."""

    session: ReadOnly[str]
    version: ReadOnly[Version]
    document_id: ReadOnly[str]
    path: ReadOnly[str | None]
    dirty: ReadOnly[bool]
    saving: ReadOnly[bool]


class SessionInfo(TypedDict):
    """Attached document state, including content and undo history."""

    session: ReadOnly[str]
    version: ReadOnly[Version]
    content: ReadOnly[str]
    document_id: ReadOnly[str]
    path: ReadOnly[str | None]
    fingerprint: ReadOnly[Fingerprint | None]
    dirty: ReadOnly[bool]
    history: ReadOnly[History]


class SaveReceipt(TypedDict):
    """A completed save. Check durable and warnings even when published is true.

    path is absolute on the host; content identifies captured resident content.
    Reopening creates a new identity; compare object data to verify the file.
    Publication means the destination was replaced, while durable reports storage syncing.
    """

    kind: ReadOnly[Literal["saved"]]
    content: ReadOnly[str]
    path: ReadOnly[str]
    fingerprint: ReadOnly[Fingerprint]
    published: ReadOnly[bool]
    durable: ReadOnly[bool]
    warnings: ReadOnly[tuple[str, ...]]


__all__ = [
    "Version",
    "ObjectAddress",
    "ObjectRecord",
    "Objects",
    "Collection",
    "Summary",
    "Metadata",
    "Capability",
    "CapabilityReason",
    "CapabilityUnsupported",
    "CapabilityUnavailable",
    "CollectionCapabilities",
    "HistoryLimits",
    "History",
    "Fingerprint",
    "SessionOverview",
    "SessionInfo",
    "SaveReceipt",
]
