"""Prepared images own independent leases; pixel snapshots are immutable values."""
from __future__ import annotations
from dataclasses import dataclass, field
from typing import TYPE_CHECKING, Literal
from ._owned import Owned
from ._values import Source, integer, record, sequence, string
if TYPE_CHECKING:
    from .backends import Backend

@dataclass(frozen=True)
class ImagePixels:
    width: int
    height: int
    rgba: bytes

    @classmethod
    def read(cls, value: object) -> ImagePixels:
        r = record(value)
        raw = r["rgba"]
        pixels = raw if isinstance(raw, bytes) else bytes(integer(v) for v in sequence(raw))
        return cls(integer(r["width"]), integer(r["height"]), pixels)

@dataclass(frozen=True)
class Snapshot:
    source: Source
    id: str
    name: str
    pixels: ImagePixels

    @classmethod
    def read(cls, value: object) -> Snapshot:
        r = record(value)
        return cls(Source.read(r["source"]), string(r["id"]), string(r["name"]), ImagePixels.read(r["pixels"]))

@dataclass(frozen=True)
class RenameIntent:
    id: str
    expected_name: str
    name: str
    kind: Literal["image_rename"] = field(default="image_rename", init=False)

@dataclass(frozen=True)
class Usage:
    """Runtime-wide table counts and actual tracked Rust buffers, not process RSS."""
    prepared_images: int
    plans: int
    rgba_bytes: int
    png_bytes: int
    blob_bytes: int
    preparing_bytes: int

    @classmethod
    def read(cls, value: object) -> Usage:
        r = record(value)
        memory = record(r["image_memory"])
        return cls(integer(r["prepared_images"]), integer(r["plans"]),
            *(integer(memory[k]) for k in ("rgba_bytes", "png_bytes", "blob_bytes", "preparing_bytes")))

class PreparedImage(Owned):
    def __init__(self, backend: Backend):
        super().__init__(backend, "image")
        self._width = self._height = 0

    @classmethod
    def prepare(cls, backend: Backend, width: int, height: int, rgba: bytes) -> PreparedImage:
        image = cls(backend)
        def load(value: object) -> PreparedImage:
            r = record(value)
            image._token = r["image"]
            image._width, image._height = integer(r["width"]), integer(r["height"])
            return image
        return image._request("prepare", lambda: backend.prepare_image(width, height, rgba, image), load)

    @property
    def width(self) -> int:
        return self._width

    @property
    def height(self) -> int:
        return self._height

    def pixels(self) -> ImagePixels:
        return self._request("pixels", lambda: self._backend.prepared_pixels(self._token, self), ImagePixels.read)

    def __enter__(self) -> PreparedImage:
        super().__enter__()
        return self

class Resources:
    def __init__(self, backend: Backend):
        self._backend = backend

    def prepare_image(self, width: int, height: int, rgba: bytes | bytearray | memoryview) -> PreparedImage:
        if any(type(v) is not int or not 0 <= v <= 0xffffffff for v in (width, height)):
            raise ValueError("Dimensions must be u32 integers")
        if not isinstance(rgba, (bytes, bytearray, memoryview)):
            raise TypeError("RGBA must be bytes or a buffer")
        size = rgba.nbytes if isinstance(rgba, memoryview) else len(rgba)
        if size > 1 << 20:
            raise ValueError("Initial pixel transport is limited to 1 MiB")
        return PreparedImage.prepare(self._backend, width, height, bytes(rgba))

    def usage(self) -> Usage:
        # A statistics query has its own recovery owner, just like other reads.
        from ._lifecycle import known
        return known(lambda: Usage.read(self._backend.statistics(self)), lambda: Usage.read(self._backend.recover(self)))
