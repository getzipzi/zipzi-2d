"""Optional Editor-host mesh generation; no document writes or native dependency."""
from __future__ import annotations
from dataclasses import dataclass, asdict, field
from typing import TYPE_CHECKING, Literal
from .meshes import Geometry
from ._calls import known
from ._errors import SdkFailure
if TYPE_CHECKING:
    from ._lifecycle import Runtime

@dataclass(frozen=True)
class FillParameters:
    edge_ratio: float = 2.
    gradation: float = 1.
    jitter: float = .45
    angle: float = 0.
    max_rows: int = 12

@dataclass(frozen=True)
class FillSettings:
    poisson: FillParameters = field(default_factory=FillParameters)
    quadtree: FillParameters = field(default_factory=FillParameters)
    fractal: FillParameters = field(default_factory=lambda: FillParameters(gradation=2.))
    paving: FillParameters = field(default_factory=FillParameters)

@dataclass(frozen=True)
class MeshSettings:
    edge_mode: Literal['single', 'double', 'triple'] = 'single'
    edge_spacing: float = 6.
    edge_width: float = 10.
    outer_margin: float = 1.
    fill_algorithm: Literal['graded_poisson', 'simple_triangles', 'adaptive_quadtree', 'triangle_fractal', 'contour_paving'] = 'graded_poisson'
    interior_spacing: float = 40.
    fill_parameters: FillSettings = field(default_factory=FillSettings)
    suppress_boundary_diagonals: bool = False
    alpha_threshold: int = 8
    wrap: float = 0.
    unit_scale: float = 1.
    detail: float = 1.
    clip_to_image: bool = True
    max_vertices: int = 65534


def generate(runtime: Runtime, width: int, height: int, alpha: bytes = b'', *,
             settings: MeshSettings | None = None,
             stage: Literal['edges', 'full', 'interior'] = 'full',
             geometry: Geometry | None = None) -> Geometry:
    """Generate owned geometry through a connected Editor, without editing a document.

    Alpha is top-down, one byte per pixel. Output positions use bottom-up source
    pixels, not model coordinates. Interior preserves the input geometry's mapping.
    Initial synchronous service limit: 4194304 pixels and 65534 output vertices.
    Use ordinary Edit operations to publish the result; generation creates no history.
    Socket timeouts use the standard same-request recovery contract.
    """
    from .backends.connected import ConnectedBackend
    backend = runtime._backend
    if not isinstance(backend, ConnectedBackend):
        raise SdkFailure('UNSUPPORTED_OPERATION', 'Mesh generation requires a connected Editor host')
    if type(width) is not int or type(height) is not int or not 0 < width <= 16384 or not 0 < height <= 16384 or width * height > 4194304:
        raise ValueError('Expected positive dimensions, at most 4194304 source pixels')
    if not isinstance(alpha, bytes):
        raise TypeError('alpha must be owned bytes')
    if stage not in ('edges', 'full', 'interior'):
        raise ValueError('Invalid generation stage')
    if stage == 'interior':
        if geometry is None or alpha:
            raise ValueError('Interior requires geometry and empty alpha')
    elif geometry is not None or len(alpha) != width * height:
        raise ValueError('Edges/full requires width*height alpha bytes and no geometry')
    source = None
    if geometry is not None:
        source = dict(vertex_ids=geometry.vertex_ids,
            positions=[dict(x=x, y=y) for x,y in geometry.positions],
            uvs=[dict(x=x, y=y) for x,y in geometry.uvs], triangles=geometry.triangles,
            meshing=None if geometry.meshing is None else dict(boundary=geometry.meshing.boundary,
                constraints=[dict(endpoints=c.endpoints, kind=c.kind.value) for c in geometry.meshing.constraints]))
    request: dict[str, object] = dict(op='editor_mesh_generate', input=dict(width=width,
        height=height, alpha_hex=alpha.hex(), stage=stage, geometry=source, settings=asdict(settings or MeshSettings())))
    return known(lambda: Geometry._read(backend._request(request, runtime, 'mesh_generation.generate')),
                 lambda: Geometry._read(backend.recover(runtime)))

__all__ = ['MeshSettings', 'FillSettings', 'FillParameters', 'generate']
