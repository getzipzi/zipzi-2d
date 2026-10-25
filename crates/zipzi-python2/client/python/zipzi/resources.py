"""Prepared images own independent leases; pixel snapshots are immutable values."""
from __future__ import annotations
from typing import TYPE_CHECKING
from ._owned import Owned
from ._values import Source,integer, record
from pathlib import Path
from ._jobs import Job
from .buffers import Buffer, MAX_BUFFER_BYTES
if TYPE_CHECKING:
    from .backends import Backend
else:
    Backend = object

from ._generated.resource_values import ImagePixels as ImagePixels, Image as Image, Usage as Usage

class PreparedImage(Owned):
    def __init__(self, backend: Backend):
        super().__init__(backend, "image")
        self._width = self._height = 0

    @classmethod
    def _from_info(cls,backend:Backend,value:object)->PreparedImage:
        image=cls(backend);raw=record(value);image._token=raw["image"];image._width=integer(raw["width"]);image._height=integer(raw["height"]);return image

    @classmethod
    def _prepare(cls, backend: Backend, width: int, height: int, rgba: bytes) -> PreparedImage:
        image = cls(backend)
        def load(value: object) -> PreparedImage:
            r = record(value)
            image._token = r["image"]
            image._width, image._height = integer(r["width"]), integer(r["height"])
            return image
        return image._request("prepare", lambda: backend.prepare_image(width, height, rgba, image), load)

    @classmethod
    def _prepare_buffer(cls, backend: Backend, width: int, height: int, buffer: Buffer) -> PreparedImage:
        buffer._check(backend)
        image = cls(backend)
        def load(value: object) -> PreparedImage:
            r = record(value)
            image._token = r["image"]
            image._width, image._height = integer(r["width"]), integer(r["height"])
            return image
        return image._request("prepare", lambda: backend.prepare_image_buffer(width, height, buffer._token, image), load)

    @property
    def width(self) -> int:
        'Cached image width in pixels; no IO.'
        return self._width

    @property
    def height(self) -> int:
        'Cached image height in pixels; no IO.'
        return self._height

    def pixels(self) -> ImagePixels:
        'Read immutable RGBA8 pixels without joining a document. The result owns bytes; the image lease remains independent. Recover a lost read through its original Call.'
        return self._request("pixels", lambda: self._backend.prepared_pixels(self._token, self), ImagePixels._read)

    def __enter__(self) -> PreparedImage:
        super().__enter__()
        return self

class ImageLease(PreparedImage):
    """Explicit host lease that pins captured pixels across replacement/history eviction."""
    source: Source
    @classmethod
    def _capture(cls,backend:Backend,session:object,edit:object | None,id:str)->ImageLease:
        image=cls(backend)
        def decode(raw:object)->ImageLease:
            value=record(raw);image._token=value["image"];image._width=integer(value["width"]);image._height=integer(value["height"]);image.source=Source._read(value["source"]);return image
        return image._request("capture",lambda:backend.capture_image(session,edit,id,image),decode)
    def __enter__(self)->ImageLease:
        super().__enter__();return self

class Resources:
    def __init__(self, backend: Backend):
        self._backend = backend

    def load_png(self,path:Path)->Job[PreparedImage]:
        'Submit decoding a host-absolute PNG into a PreparedImage Job. Wait for the accepted job and close the resulting lease; decoder failures do not poison an unrelated Edit.'
        from ._lifecycle import Job,absolute
        path=absolute(path)
        return Job[PreparedImage]._start(self._backend,lambda owner:self._backend.prepare_image_job(str(path),None,owner),"image",lambda raw:PreparedImage._from_info(self._backend,raw))
    def prepare_png(self,buffer:Buffer)->Job[PreparedImage]:
        'Submit decoding PNG bytes from an existing same-client immutable Buffer. The Job returns a PreparedImage lease, independent of document publication.'
        buffer._check(self._backend)
        return Job[PreparedImage]._start(self._backend,lambda owner:self._backend.prepare_image_job(None,buffer._token,owner),"image",lambda raw:PreparedImage._from_info(self._backend,raw))

    def prepare_image(self, width: int, height: int, rgba: bytes | bytearray | memoryview | Buffer) -> PreparedImage:
        'Own RGBA8 bytes or an immutable Buffer and prepare a width-by-height image lease. Preparation does not edit a document. Close the lease after candidate acceptance; SDK/history owners retain their references.'
        if any(type(v) is not int or not 0 <= v <= 0xffffffff for v in (width, height)):
            raise ValueError("Dimensions must be u32 integers")
        if isinstance(rgba, Buffer):
            return PreparedImage._prepare_buffer(self._backend, width, height, rgba)
        if not isinstance(rgba, (bytes, bytearray, memoryview)):
            raise TypeError("RGBA must be bytes or a buffer")
        size = rgba.nbytes if isinstance(rgba, memoryview) else len(rgba)
        if size > MAX_BUFFER_BYTES:
            raise ValueError("Image payload exceeds 256 MiB")
        return PreparedImage._prepare(self._backend, width, height, bytes(rgba))

    def prepare_buffer(self, data: bytes | bytearray | memoryview) -> Buffer:
        'Own bytes in a reusable immutable host Buffer, with bounded upload and original-request recovery. Preparation does not edit a document; explicitly close the Buffer after use.'
        return Buffer._prepare(self._backend, data)

    def usage(self) -> Usage:
        # A statistics query has its own recovery owner, just like other reads.
        'Query owned resource usage and actual shared image buffer counters. These are scoped Runtime estimates, not process RSS or a promise of later admission.'
        from ._lifecycle import known
        return known(lambda: Usage._read(self._backend.statistics(self)), lambda: Usage._read(self._backend.recover(self)))

from ._generated.resource_ops import CreateImage as CreateImage, ReplaceImage as ReplaceImage, RenameImage as RenameImage

from ._generated.discovery import ObjectSummary as Summary

__all__ = ['PreparedImage', 'ImageLease', 'CreateImage', 'ReplaceImage', 'RenameImage', 'ImagePixels', 'Image', 'Usage', 'Summary']
