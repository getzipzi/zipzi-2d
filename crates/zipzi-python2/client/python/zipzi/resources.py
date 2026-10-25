"""Prepared images own independent leases; pixel snapshots are immutable values."""
from __future__ import annotations
from typing import TYPE_CHECKING
from ._owned import Owned
from ._values import integer, record
if TYPE_CHECKING:
    from .backends import Backend

from ._generated.resource_values import ImagePixels as ImagePixels, Snapshot as Snapshot, Usage as Usage
from ._generated.plan_intents import ImageRename as _RenameIntent
RenameIntent = _RenameIntent

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
