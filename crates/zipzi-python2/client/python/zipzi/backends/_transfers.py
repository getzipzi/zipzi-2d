"""Recoverable bounded byte transfers; each subrequest keeps the public owner."""
from __future__ import annotations
import hashlib
from typing import TYPE_CHECKING, Literal, Protocol
from .._errors import OutcomeUnknown, SdkFailure, PendingRequest, unwrap
from .._results import Batch
from .._values import Source
from .._values import integer, record, string
from .._generated import binary as typed_binary
if TYPE_CHECKING:
    pass

class TransferBackend(Protocol):
    def _request(self, value: dict[str,object], owner: object, operation: str) -> object: ...
    def _recover_wire(self, owner: object) -> object: ...
    def _binary_request(self, value: dict[str,object], payload: bytes, owner: object, operation: str) -> object: ...
    def pending(self, owner: object) -> PendingRequest | None: ...
    def recover(self, owner: object) -> object: ...

CHUNK = 1 << 20

class Transfer:
    def __init__(self, backend: TransferBackend, owner: object,
        kind: Literal["image", "buffer", "pixels", "snapshot", "read", "authoring", "records", "samples", "meshes", "bindings", "images", "preview"], *, data: bytes = b"",
        width: int = 0, height: int = 0, selector: dict[str, object] | None = None,
        blob: object = None, offset: int = 0, length: int = 0, edit: object = None):
        self.backend, self.owner, self.kind = backend, owner, kind
        self.data, self.width, self.height = data, width, height
        self.selector = selector
        self.blob = blob
        self.edit = edit
        self.offset, self.end = offset, offset + length
        self.parts: list[bytes] = []
        self.value: object = None
        self.phase = "create" if kind in {"image", "buffer", "authoring"} else "read" if kind == "read" else "select"
        self.fault: BaseException | None = None

    def _invoke(self, operation: dict[str, object], payload: bytes | None = None) -> object:
        if self.backend.pending(self.owner) is not None:
            return self.backend._recover_wire(self.owner)
        label = "resources.transfer." + str(operation["op"])
        if payload is None:
            return self.backend._request(operation, self.owner, label)
        return self.backend._binary_request(operation, payload, self.owner, label)

    def _cleanup_recover(self) -> None:
        try:
            self.backend.recover(self.owner)
        except BaseException as error:
            if error is not self.fault:
                raise

    def drive(self) -> object:
        while True:
            try:
                if self.phase == "create":
                    if self.kind == "authoring":
                        self.blob = record(self._invoke(dict(op="start_authoring_upload", edit=self.edit, length=len(self.data))))["blob"]
                    else:
                        self.blob = self._invoke(dict(op="create_blob", length=len(self.data)))
                    self.phase = "append"
                elif self.phase == "append":
                    if self.offset == len(self.data):
                        self.phase = "seal"
                        continue
                    chunk = self.data[self.offset:self.offset + CHUNK]
                    self.offset = integer(self._invoke(dict(op="append_blob_binary", blob=self.blob,
                        offset=self.offset, length=len(chunk), payload_sha256=hashlib.sha256(chunk).hexdigest()), chunk))
                elif self.phase == "seal":
                    size = integer(self._invoke(dict(op="seal_blob", blob=self.blob)))
                    self.data = b""
                    if self.kind == "buffer":
                        return self.blob
                    self.end = size
                    self.phase = "prepare"
                elif self.phase == "prepare":
                    if self.kind == "authoring":
                        self.value = self._invoke(dict(op="apply_authoring_blob", edit=self.edit, blob=self.blob))
                    else:
                        self.value = self._invoke(dict(op="prepare_image_blob", width=self.width, height=self.height, blob=self.blob))
                    self.phase = "release"
                elif self.phase == "select":
                    assert self.selector is not None
                    self.value = self._invoke(self.selector)
                    if self.kind in {"samples","preview"} and self.selector["op"]=="job":
                        result=record(self.value)["result"]
                        if result is None: return None
                        self.value=unwrap(result)
                    if self.value is None: return None
                    value = record(self.value)
                    if "blob" not in value:
                        return self.value
                    self.blob = value["blob"]
                    if self.kind in {"records","samples","meshes","bindings","images","preview"}:
                        self.end = integer(value["length"])
                    else:
                        pixels = record(value["pixels"]) if self.kind == "snapshot" else value
                        self.end = integer(pixels["width"]) * integer(pixels["height"]) * 4
                    if not 0 <= self.end <= 256 << 20:
                        raise SdkFailure("BUDGET_EXCEEDED", "Immutable result exceeds transfer budget")
                    self.phase = "read"
                elif self.phase == "read":
                    if self.offset == self.end:
                        content = b"".join(self.parts)
                        self.parts.clear()
                        if self.kind == "read":
                            return content
                        value = dict(record(self.value))
                        value.pop("blob", None)
                        if self.kind in {"records","samples","meshes","bindings","images","preview"}:
                            if self.kind=="preview":
                                self.value=dict(source=value["source"],samples=typed_binary.decode_preview(content,string(value["signature"])))
                            elif self.kind=="images":
                                values=typed_binary.decode_images(content,string(value["signature"]))
                                self.value=Batch(Source._read(value["source"]),tuple(item.value for item in values))
                            elif self.kind=="bindings":
                                self.value=dict(source=value["source"],bindings=typed_binary.decode_bindings(content,string(value["signature"])))
                            elif self.kind=="meshes":
                                values=typed_binary.decode_meshes(content,string(value["signature"]))
                                self.value=values[0] if value["single"] else Batch(Source._read(value["source"]), tuple(item.value for item in values))
                            else:
                                self.value = (dict(source=value["source"], records=typed_binary.decode_records(content,string(value["signature"]))) if self.kind=="records" else dict(source=value["source"],state=value["state"],samples=typed_binary.decode_samples(content,string(value["signature"]))))
                            self.phase = "release"
                            continue
                        if self.kind == "snapshot":
                            pixels = dict(record(value["pixels"]))
                            pixels["rgba"] = content
                            value["pixels"] = pixels
                        else:
                            value["rgba"] = content
                        self.value = value
                        self.phase = "release"
                        continue
                    count = min(CHUNK, self.end - self.offset)
                    data = self._invoke(dict(op="read_blob_binary", blob=self.blob, offset=self.offset, length=count), b"")
                    if not isinstance(data, bytes) or len(data) != count:
                        raise ValueError("Incomplete immutable buffer read")
                    self.parts.append(data)
                    self.offset += count
                elif self.phase == "release":
                    try:
                        self._invoke(dict(op="release_blob", blob=self.blob))
                    except OutcomeUnknown as error:
                        if self.fault is None:
                            raise
                        error._resume = self._cleanup_recover
                        if isinstance(self.fault, SdkFailure):
                            self.fault.cleanup = error
                        else:
                            self.fault.__context__ = error
                        self.fault.add_note("Buffer cleanup has a recoverable pending request in error.cleanup")
                        raise self.fault from error
                    except SdkFailure as cleanup:
                        if self.fault is None:
                            raise
                        self.fault.add_note(f"Buffer cleanup failed: {cleanup}")
                    self.blob = None
                    if self.fault is not None and self.kind=="authoring":
                        self.phase="release_edit"
                        continue
                    if self.fault is not None:
                        raise self.fault
                    return self.value
                elif self.phase=="release_edit":
                    try:
                        self._invoke(dict(op="release_edit",edit=self.edit))
                    except OutcomeUnknown as cleanup:
                        cleanup._resume=self._cleanup_recover
                        if isinstance(self.fault,SdkFailure):self.fault.cleanup=cleanup
                        assert self.fault is not None
                        raise self.fault from cleanup
                    except SdkFailure as cleanup:
                        assert self.fault is not None
                        self.fault.add_note(f"Candidate cleanup failed: {cleanup}")
                    assert self.fault is not None
                    raise self.fault
                else:
                    raise RuntimeError("Unknown transfer phase")
            except OutcomeUnknown:
                raise
            except (SdkFailure, TypeError, ValueError, OverflowError) as error:
                if self.phase != "release" and self.blob is not None and self.kind != "read":
                    self.fault = error
                    self.data = b""
                    self.parts.clear()
                    self.phase = "release"
                    continue
                raise
