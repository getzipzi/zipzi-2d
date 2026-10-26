"""Independent model-space numerics on formal, candidate and animated sources."""
from dataclasses import replace
from pathlib import Path
import zipzi
from zipzi import animation as a, bindings, meshes, modeling as m, parameters, scene, spatial as s

def uid(n: int) -> str:
    return f"79650000-0000-4000-8000-{n:012x}"

GEOMETRY=meshes.Geometry((10,20,30),((0.,0.),(1.,0.),(0.,1.)),
    ((0.,0.),(1.,0.),(0.,1.)),((10,20,30),))

def fixture(runtime: zipzi.Runtime) -> zipzi.Session:
    session=runtime.sessions.create(uid(1))
    with session.edit("Spatial fixture") as edit:
        edit.parameters.create(parameters.ParameterSpec("P",-1.,1.,-1.,id=uid(9),runtime_id="P"))
        for n,name in ((2,"Moving"),(3,"Top"),(7,"Degenerate"),(8,"Hidden")):
            geometry=replace(GEOMETRY,positions=((0.,0.),(.5,0.),(1.,0.))) if n==7 else GEOMETRY
            edit.meshes.create(meshes.MeshSpec(name,geometry,id=uid(n)))
        edit.apply_many((scene.CreatePart(scene.PartSpec("Root",id=uid(4))),
            scene.CreatePart(scene.PartSpec("Leaf",id=uid(5),parent_id=uid(4))),
            scene.CreatePart(scene.PartSpec("Empty",id=uid(6))),
            scene.MoveToPart((uid(2),uid(3),uid(7),uid(8)),part_id=uid(5)),
            scene.SetSceneEnabled(uid(8),True,False)))
        edit.bindings.create(bindings.MeshBindingSpec(uid(2),(bindings.Axis(uid(9),(-1.,1.)),),(
            bindings.Keyform((-1.,),GEOMETRY.positions),
            bindings.Keyform((1.,),tuple((x+10.,y) for x,y in GEOMETRY.positions))),id=uid(15)))
        edit.apply(m.ModelingCommandMotion(intent=m.MotionEditCreate(id=uid(10),name="Clip",
            registration_id=uid(11),destination=m.MotionContainerRoot())))
        clip=edit.modeling.records(m.ModelingQuery(motions=(uid(10),))).value.motions[0]
        clip=replace(clip,duration=1.,fade_in=0.,fade_out=0.,tracks=(m.MotionTrack(id=uid(12),
            target=m.MotionTrackTargetParameter(parameter_id=uid(9)),initial=m.MotionKey(id=uid(13),time=0.,value=-1.),
            segments=(m.MotionSegmentLinear(end=m.MotionKey(id=uid(14),time=1.,value=1.)),),extensions={}),))
        edit.apply(m.ModelingCommandReplaceMotion(clip=clip))
    return session

def bounds(result: s.Result) -> s.ObjectBounds:
    assert isinstance(result,s.BoundsResult)
    return result.value

def hits(result: s.Result) -> s.GeometryQuery:
    assert isinstance(result,s.HitsResult)
    return result.value

def run(runtime: zipzi.Runtime, folder: Path) -> dict[str,object]:
    session=fixture(runtime)
    before=session.info()
    assert "spatial" in runtime.capabilities().domains
    queries=(s.Bounds(targets=(s.MeshTarget(value=uid(2)),)),
        s.Bounds(targets=(s.PartTarget(value=uid(4)),s.MeshTarget(value=uid(2)))),
        s.Hits(point=(.25,.25),details=True,max_candidates=1),
        s.Hits(point=(.25,.25),include_hidden=True,details=True),
        s.Bounds(targets=(s.MeshTarget(value=uid(8)),)),
        s.Bounds(targets=(s.MeshTarget(value=uid(8)),),include_hidden=True),
        s.Bounds(targets=(s.PartTarget(value=uid(6)),)),s.Hits(point=(100.,100.)))
    poses=({uid(9):-1.},{uid(9):0.},{uid(9):1.},{uid(9):-1.},{uid(9):5.})
    job=session.modeling.spatial(poses,queries)
    # The accepted read retains the old authoring object table as well as geometry.
    with session.edit("Later label") as edit: edit.meshes.rename(uid(3),"New Top")
    sampled=job.wait()
    assert sampled.source.version==before.version
    batch=sampled.value
    assert batch.requested==poses and batch.queries==queries and len(batch.frames)==5
    x=tuple(bounds(frame.results[0]).model_bounds[0] for frame in batch.frames)
    assert x==(0.,5.,10.,0.,10.),x
    assert batch.frames[-1].parameters[uid(9)]==1.  # Effective clamped value.
    first=batch.frames[0]
    assert bounds(first.results[1]).mesh_ids==(uid(2),uid(3),uid(7),uid(8))
    limited=hits(first.results[2]); hit=limited.hits[0]; triangle=hit.triangles[0]
    assert limited.total==2 and limited.truncated and hit.mesh_id==uid(3) and hit.name=="Top"
    assert hit.part_path==(uid(4),uid(5)) and hit.part_names==("Root","Leaf")
    assert triangle.vertex_indices==(0,1,2) and triangle.vertex_ids==(10,20,30)
    assert triangle.barycentric==(.5,.25,.25) and triangle.uv==(.25,.25)
    hidden=hits(first.results[3]); assert hidden.total==3
    assert tuple(h.render_order for h in hidden.hits)==tuple(sorted((h.render_order for h in hidden.hits),reverse=True))
    assert not next(h for h in hidden.hits if h.mesh_id==uid(8)).enabled
    assert bounds(first.results[4]).model_bounds is None and bounds(first.results[4]).empty_reason=="filtered_out"
    assert bounds(first.results[5]).model_bounds==(0.,0.,1.,1.)
    assert bounds(first.results[6]).empty_reason=="no_descendant_mesh"
    assert hits(first.results[7]).total==0
    assert session.modeling.spatial((),queries).wait().value.frames==()
    assert session.modeling.spatial(({},),()).wait().value.frames[0].results==()
    # A failed read leaves the candidate usable; a queued read survives its cancellation.
    edit=session.edit("Frozen query")
    captured=edit.apply(scene.SetSceneEnabled(uid(2),True,False)).source
    try:edit.modeling.spatial(({},),(s.Bounds(targets=(s.MeshTarget(value=uid(99)),)),)).wait()
    except zipzi.SdkFailure as error:assert error.code=="MISSING_MESH"
    else:raise AssertionError("Missing target accepted")
    candidate=edit.modeling.spatial(({},),queries,expected=captured)
    edit.meshes.rename(uid(3),"Candidate Top")
    edit.cancel()
    frozen=candidate.wait()
    assert frozen.source==captured and bounds(frozen.value.frames[0].results[0]).empty_reason=="filtered_out"
    assert hits(frozen.value.frames[0].results[2]).hits[0].name=="New Top"
    after=session.info()
    with session.preview() as preview:
        preview.apply(a.ScheduleMotion(id=uid(10),time=0.))
        sampled=preview.sample((0.,.25,.5,.25),geometry=False,queries=queries[:1]).wait()
        assert sampled.source==preview.source and sampled.value.queries==queries[:1]
        positions=tuple(bounds(frame.spatial[0]).model_bounds[0] for frame in sampled.value.frames)
        local=tuple(frame.motion.motion_positions[0].local_time for frame in sampled.value.frames)
        assert all(abs(actual-10*t)<1e-5 for actual,t in zip(positions,local)),positions
        assert all(frame.geometry is None and frame.preview_id==preview.id for frame in sampled.value.frames)
        assert preview.info().value.time==.25
    assert session.info()==after and runtime.resources.usage().previews==0
    return dict(bounds_x=x,triangle_ids=triangle.vertex_ids,barycentric=triangle.barycentric,
        hit_order=(uid(3),uid(2)),hidden_total=hidden.total,preview_x=positions,frozen_candidate=True)
