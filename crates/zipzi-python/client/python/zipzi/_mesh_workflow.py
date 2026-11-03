"""Document-aware mesh generation and bounded CPU quality inspection."""
from __future__ import annotations
from collections import Counter
from dataclasses import dataclass, replace
import math
from collections.abc import Callable, Generator
from typing import TypeVar, cast
from ._calls import Call
from ._errors import OutcomeUnknown
from ._results import Snapshot
from .resources import Image
from . import modeling as m
from .meshes import Geometry
from ._errors import SdkFailure
from ._lifecycle import Session, Edit
from ._values import Source, WriteResult
from .mesh_generation import MeshSettings

T = TypeVar('T')


def _run_workflow(flow: Generator[Callable[[], object], object, T]) -> T:
    """Recover the suspended request, then continue without replaying prior IO."""
    pending: Callable[[], object] | None = None

    def advance(value: object) -> T:
        try:
            action = flow.send(value)
        except StopIteration as done:
            return cast(T, done.value)
        return drive(action)

    def drive(action: Callable[[], object]) -> T:
        nonlocal pending
        while True:
            try:
                value = action()
            except OutcomeUnknown as error:
                pending = error._resume
                raise
            try:
                action = flow.send(value)
            except StopIteration as done:
                return cast(T, done.value)

    def start() -> T:
        return drive(next(flow))

    def resume() -> T:
        nonlocal pending
        assert pending is not None
        try:
            value = pending()
        except OutcomeUnknown as error:
            pending = error._resume
            raise
        return advance(value)

    return Call('mesh_generation.workflow', start, lambda value: cast(T, value), resume=resume).run()


@dataclass(frozen=True)
class MeshDraft:
    """Frozen generated geometry in the original mesh's authoring coordinates.

    Generation does not edit the document. apply() checks the captured Source and
    lets the host validate complete binding/BlendShape/Glue replacements. It never
    silently drops dependencies. Inspect the Edit candidate before committing.

    Fields:
    source: Exact document Source captured before generation; later edits invalidate the draft.
    mesh: Original canonical mesh, including texture, parent and drawing metadata.
    geometry: Generated geometry mapped into the original mesh authoring space.
    space: Model, rotation-local or Warp-parameter space for the original mesh.
    """
    __module__ = "zipzi.mesh_generation"
    source: Source
    mesh: m.mesh.Mesh
    geometry: Geometry
    space: m.sampling.GeometrySpace

    def apply(self, edit: Edit, *, vertex_mapping: dict[int, int | None],
             binding: m.bindings.MeshBinding | None = None,
             blend_bindings: tuple[m.bindings.BlendShapeBinding, ...] = (),
             glues: tuple[m.scene.Glue, ...] = ()) -> WriteResult:
        """Apply replacement to an existing Edit; publication remains explicit.

        Supply the complete existing dependency records with resampled shapes.
        Omitted dependencies are rejected by the host. vertex_mapping explicitly
        maps every old ID to a new ID or None; new IDs do not imply correspondence.
        """
        return _run_workflow(self._apply(edit, vertex_mapping, binding, blend_bindings, glues))

    def _apply(self, edit: Edit, vertex_mapping: dict[int, int | None],
               binding: m.bindings.MeshBinding | None,
               blend_bindings: tuple[m.bindings.BlendShapeBinding, ...],
               glues: tuple[m.scene.Glue, ...]) -> Generator[Callable[[], object], object, WriteResult]:
        yield lambda: edit._session.meshes.get(self.mesh.id, expected=self.source)
        current = cast(Snapshot[m.OwnedModelingRecords], (yield lambda: edit.modeling.records(m.ModelingQuery(meshes=(self.mesh.id,)))))
        if current.value.meshes[0] != self.mesh:
            raise SdkFailure('STALE_SOURCE', 'Mesh changed in this candidate after generation')
        g = self.geometry
        meshing = None if g.meshing is None else m.mesh.MeshMeshing(
            boundary=g.meshing.boundary, constraints=tuple(m.mesh.MeshConstraint(
                endpoints=c.endpoints, kind=c.kind) for c in g.meshing.constraints))
        mesh = replace(self.mesh, vertex_ids=g.vertex_ids,
            base_positions=tuple(m.common.Vec2(*p) for p in g.positions),
            uvs=tuple(m.common.Vec2(*p) for p in g.uvs), triangles=g.triangles, meshing=meshing)
        snapshot = m.sampling.GeometrySnapshot(version=self.source.version,
            mesh_id=self.mesh.id, vertex_ids=self.mesh.vertex_ids,
            positions=self.mesh.base_positions, uvs=self.mesh.uvs,
            triangles=self.mesh.triangles, meshing=self.mesh.meshing, space=self.space)
        operation = m.commands.ModelingCommandReplaceTopology(source=snapshot,
            replacement=m.mesh.TopologyReplacement(mesh=mesh, binding=binding,
                blend_bindings=blend_bindings, glues=glues, vertex_mapping=vertex_mapping))
        return cast(WriteResult, (yield lambda: edit.apply(operation)))


def generate_for_mesh(session: Session, mesh_id: str, *, settings: MeshSettings | None = None) -> MeshDraft:
    """Read the mesh/texture on one Source and generate an uncommitted draft.

    Requires a connected Editor and an affine UV-to-authoring map. Validates all
    source vertices, including deformed meshes; rejects non-affine mappings rather
    than guessing. The draft retains original metadata and authoring space.
    """
    return _run_workflow(_generate_for_mesh(session, mesh_id, settings))


def _generate_for_mesh(session: Session, mesh_id: str, settings: MeshSettings | None) -> Generator[Callable[[], object], object, MeshDraft]:
    from .mesh_generation import generate
    selected = cast(Snapshot[m.OwnedModelingRecords], (yield lambda: session.modeling.records(m.ModelingQuery(meshes=(mesh_id,)))))
    mesh = selected.value.meshes[0]
    if mesh is None:
        raise SdkFailure('MISSING_MESH', mesh_id)
    if not isinstance(mesh.texture_asset_id, m.scene.SceneReferenceResolved):
        raise SdkFailure('UNRESOLVED_TEXTURE', 'Mesh generation requires a resolved image asset')
    slots = dict(zip(mesh.vertex_ids, range(len(mesh.vertex_ids))))
    basis = None
    for triangle in mesh.triangles:
        a, b, c = (slots[v] for v in triangle)
        u0, v0 = mesh.uvs[a]
        du1, dv1 = mesh.uvs[b].x-u0, mesh.uvs[b].y-v0
        du2, dv2 = mesh.uvs[c].x-u0, mesh.uvs[c].y-v0
        det = du1*dv2-dv1*du2
        if abs(det) > 1e-12:
            basis = (a, b, c, u0, v0, du1, dv1, du2, dv2, det)
            break
    if basis is None:
        raise SdkFailure('NON_AFFINE_MAPPING', 'Mesh has no nondegenerate UV triangle')
    a, b, c, u0, v0, du1, dv1, du2, dv2, det = basis
    def mapped(uv: tuple[float, float]) -> tuple[float, float]:
        du, dv = uv[0]-u0, uv[1]-v0
        t, q = (du*dv2-dv*du2)/det, (du1*dv-dv1*du)/det
        point = tuple(mesh.base_positions[a][i]+t*(mesh.base_positions[b][i]-mesh.base_positions[a][i])+q*(mesh.base_positions[c][i]-mesh.base_positions[a][i]) for i in range(2))
        return point[0], point[1]
    extent = max(1., *(max(p[i] for p in mesh.base_positions)-min(p[i] for p in mesh.base_positions) for i in range(2)))
    if any(math.dist(mapped(uv), pos) > extent*1e-5 for uv, pos in zip(mesh.uvs, mesh.base_positions)):
        raise SdkFailure('NON_AFFINE_MAPPING', 'Existing mesh requires non-affine UV resampling')
    space = m.sampling.GeometrySpaceModel()
    if mesh.deformer_id:
        parents = cast(Snapshot[m.OwnedModelingRecords], (yield lambda: session.modeling.records(m.ModelingQuery(transforms=(mesh.deformer_id,)))))
        if parents.source != selected.source:
            raise SdkFailure('STALE_SOURCE', 'Mesh changed while reading its parent')
        parent = parents.value.transforms[0]
        if parent is None:
            raise SdkFailure('MISSING_TRANSFORM', mesh.deformer_id)
        space = (m.sampling.GeometrySpaceWarpParameter(value=mesh.deformer_id)
                 if isinstance(parent.data, m.scene.TransformDataWarp)
                 else m.sampling.GeometrySpaceRotationLocal(value=mesh.deformer_id))
    asset_id = mesh.texture_asset_id.object_id
    image = cast(Snapshot[Image], (yield lambda: session.resources.get(asset_id, expected=selected.source)))
    pixels = image.value.pixels
    geometry = cast(Geometry, (yield lambda: generate(session._runtime, pixels.width, pixels.height, pixels.rgba[3::4], settings=settings)))
    geometry = Geometry(geometry.vertex_ids, tuple(mapped(uv) for uv in geometry.uvs), geometry.uvs, geometry.triangles, geometry.meshing)
    return MeshDraft(selected.source, mesh, geometry, space)


@dataclass(frozen=True)
class GeometryQuality:
    """CPU quality of one geometry, independent of any document or GPU.

    Angles use supplied positions; coverage samples source UV pixel centers with
    alpha >= alpha_threshold. Ignores texture filtering, masks and occlusion.
    pixel_step > 1 denotes approximate coverage. No animation is evaluated.

    Fields:
    vertex_count: Number of supplied mesh vertices.
    triangle_count: Number of supplied triangles.
    min_angle_degrees: Smallest triangle angle; zero for empty or degenerate geometry.
    skinny_triangles: Count of triangles with a minimum angle below ten degrees.
    degenerate_triangles: Count of triangles with zero edges or double area below 1e-12.
    mixed_winding_triangles: Smaller of the positive and negative winding counts; global reflection is allowed.
    nonmanifold_edges: Edges incident to more than two triangles.
    pixel_step: Source pixel sampling stride; greater than one denotes approximation.
    opaque_pixels: Sampled source pixels whose alpha meets the threshold.
    uncovered_opaque_pixels: Sampled painted pixels outside the UV triangle union.
    covered_pixels: Sampled source pixels inside the UV triangle union.
    transparent_covered_pixels: Covered samples below the alpha threshold.
    uncovered_opaque_fraction: Uncovered opaque samples divided by opaque samples; None if none are painted.
    """
    __module__ = "zipzi.mesh_generation"
    vertex_count: int
    triangle_count: int
    min_angle_degrees: float
    skinny_triangles: int
    degenerate_triangles: int
    mixed_winding_triangles: int
    nonmanifold_edges: int
    pixel_step: int
    opaque_pixels: int
    uncovered_opaque_pixels: int
    covered_pixels: int
    transparent_covered_pixels: int
    uncovered_opaque_fraction: float | None


def inspect_geometry(geometry: Geometry, width: int, height: int, alpha: bytes, *, alpha_threshold: int = 8) -> GeometryQuality:
    """Inspect generated/mapped geometry without an Edit, Runtime or GPU.

    Coverage uses a grid with step ceil(max(width,height)/256), matching observation
    sampling. At most 4194304 source pixels, 131068 triangles and 8 million triangle
    scanlines are accepted. Malformed/nonfinite geometry raises ValueError.
    """
    if type(width) is not int or type(height) is not int or min(width, height) <= 0 or width*height > 4194304:
        raise ValueError('Expected positive dimensions with at most 4194304 pixels')
    if not isinstance(alpha, bytes) or len(alpha) != width*height:
        raise ValueError('Expected width*height owned alpha bytes')
    if type(alpha_threshold) is not int or not 0 <= alpha_threshold <= 255:
        raise ValueError('alpha_threshold must be between 0 and 255')
    g = geometry
    if len(g.vertex_ids) != len(set(g.vertex_ids)) or len(g.vertex_ids) != len(g.positions) or len(g.vertex_ids) != len(g.uvs) or len(g.triangles)>131068:
        raise ValueError('Invalid geometry arrays')
    if any(not math.isfinite(v) for point in (*g.positions, *g.uvs) for v in point):
        raise ValueError('Nonfinite geometry')
    slots = dict(zip(g.vertex_ids, range(len(g.vertex_ids))))
    step = max(1, math.ceil(max(width, height)/256))
    cols, rows = math.ceil(width/step), math.ceil(height/step)
    covered = bytearray(cols*rows)
    edges: Counter[tuple[int, ...]] = Counter()
    angles: list[float] = []
    positive = negative = degenerate = scanlines = 0
    for triangle in g.triangles:
        if any(v not in slots for v in triangle):raise ValueError('Triangle references missing vertex')
        indices = [slots[v] for v in triangle]
        p = [g.positions[i] for i in indices]
        a,b,c = p
        area = (b[0]-a[0])*(c[1]-a[1])-(b[1]-a[1])*(c[0]-a[0])
        positive += area>0; negative += area<0
        lengths = [math.dist(p[i],p[(i+1)%3]) for i in range(3)]
        if abs(area) < 1e-12 or min(lengths)==0:
            degenerate += 1; angles.append(0.)
        else:
            angles.append(min(math.degrees(math.acos(max(-1.,min(1.,(lengths[i]**2+lengths[(i+1)%3]**2-lengths[(i+2)%3]**2)/(2*lengths[i]*lengths[(i+1)%3]))))) for i in range(3)))
        for i in range(3):edges[tuple(sorted((triangle[i],triangle[(i+1)%3])))]+=1
        uv = [(g.uvs[i][0]*width, (1-g.uvs[i][1])*height) for i in indices]
        lo = max(0, math.ceil((min(p[1] for p in uv)-.5)/step))
        hi = min(rows, math.floor((max(p[1] for p in uv)-.5)/step)+1)
        scanlines += max(0,hi-lo)
        if scanlines>8000000:raise ValueError('Coverage exceeds scanline budget')
        for row in range(lo,hi):
            y = row*step+.5
            xs: list[float] = []
            for i in range(3):
                a,b=uv[i],uv[(i+1)%3]
                if min(a[1],b[1])<=y<max(a[1],b[1]):xs.append(a[0]+(y-a[1])*(b[0]-a[0])/(b[1]-a[1]))
            if len(xs)>=2:
                left=max(0,math.ceil((min(xs)-.5-1e-7)/step));right=min(cols,math.floor((max(xs)-.5+1e-7)/step)+1)
                if right>left:covered[row*cols+left:row*cols+right]=b'\1'*(right-left)
    opaque=missed=waste=0
    for row in range(rows):
        for col in range(cols):
            painted=alpha[row*step*width+col*step]>=alpha_threshold
            hit=covered[row*cols+col]
            opaque+=painted;missed+=painted and not hit;waste+=not painted and bool(hit)
    return GeometryQuality(len(g.vertex_ids),len(g.triangles),min(angles,default=0.),sum(a<10 for a in angles),degenerate,min(positive,negative),sum(n>2 for n in edges.values()),step,opaque,missed,sum(covered),waste,missed/opaque if opaque else None)
