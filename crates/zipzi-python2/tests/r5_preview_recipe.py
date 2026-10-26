"""Public CPU Preview numerics and lifetime; no Editor or GPU is required."""
from contextlib import ExitStack
from dataclasses import replace
from pathlib import Path
import zipzi
from zipzi import animation as a, modeling as m, meshes, parameters, bindings, scene

def uid(n: int) -> str:
    return f"79640000-0000-4000-8000-{n:012x}"

GEOMETRY=meshes.Geometry((10,20,30),((0.,0.),(1.,0.),(0.,1.)),
    ((0.,0.),(1.,0.),(0.,1.)),((10,20,30),))

def fixture(runtime: zipzi.Runtime) -> zipzi.Session:
    session=runtime.sessions.create(uid(1))
    with session.edit("Preview fixture") as edit:
        edit.parameters.create(parameters.ParameterSpec("P",-1.,1.,0.,id=uid(2),runtime_id="P"))
        edit.meshes.create(meshes.MeshSpec("Mesh",GEOMETRY,id=uid(3)))
        edit.bindings.create(bindings.MeshBindingSpec(uid(3),(bindings.Axis(uid(2),(-1.,1.)),),(
            bindings.Keyform((-1.,),GEOMETRY.positions),
            bindings.Keyform((1.,),tuple((x+10.,y) for x,y in GEOMETRY.positions))),id=uid(4)))
        edit.apply(scene.CreatePart(scene.PartSpec("Part",id=uid(5))))
        edit.apply_many((
            m.ModelingCommandMotion(intent=m.MotionEditCreate(id=uid(10),name="Clip",registration_id=uid(11),destination=m.MotionContainerRoot())),
            m.ModelingCommandExpression(intent=m.ExpressionEditCreate(id=uid(20),name="Smile")),
            m.ModelingCommandExpression(intent=m.ExpressionEditCreateEntry(owner=uid(20),id=uid(21))),
            m.ModelingCommandPose(intent=m.PoseEditCreateGroup(pose_id=uid(30),id=uid(31),name="Pose")),
            m.ModelingCommandPose(intent=m.PoseEditCreateEntry(group_id=uid(31),id=uid(32))),
        ))
        records=edit.modeling.records(m.ModelingQuery(motions=(uid(10),),expressions=(uid(20),),pose=True)).value
        clip=replace(records.motions[0],duration=1.,fade_in=0.,fade_out=0.,tracks=(m.MotionTrack(id=uid(12),
            target=m.MotionTrackTargetParameter(parameter_id=uid(2)),initial=m.MotionKey(id=uid(13),time=0.,value=-1.),
            segments=(m.MotionSegmentLinear(end=m.MotionKey(id=uid(14),time=1.,value=1.)),),extensions={}),))
        edit.apply(m.ModelingCommandReplaceMotion(clip=clip))
        expression=replace(records.expressions[0],fade_in=0.,fade_out=0.,entries=(replace(records.expressions[0].entries[0],
            target=m.ExpressionTargetResolved(parameter_id=uid(2)),value=.75,blend=m.ExpressionBlend.OVERWRITE),))
        edit.apply(m.ModelingCommandReplaceExpression(expression=expression))
        edit.apply(m.ModelingCommandPose(intent=m.PoseEditPatch(id=uid(32),expected=m.PoseFieldsEntry(part=m.PosePartRefUnspecified()),
            fields=m.PoseFieldsEntry(part=m.PosePartRefResolved(part_id=uid(5))))))
    return session

def run(runtime: zipzi.Runtime, folder: Path) -> dict[str, object]:
    session=fixture(runtime)
    before=session.info()
    assert "animation_preview" in runtime.capabilities().domains
    with ExitStack() as owned:
        movie=owned.enter_context(session.preview())
        manual=owned.enter_context(session.preview(expected=movie.source))
        assert movie.id!=manual.id and movie.source==manual.source
        movie.apply(a.ScheduleMotion(id=uid(10),time=0.))
        manual.apply(a.SetParameter(id=uid(2),value=.5))
        manual.apply(a.SetPhysics(enabled=False,stabilized_start=False))
        result=movie.sample((0.,.25,.5,.25),geometry=True).wait()
        assert result.source==movie.source
        x=tuple(frame.geometry.drawables[0].positions[0].x for frame in result.value.frames)
        # Activations start on the first canonical update at/after the schedule.
        # Inspect the SDK local coordinate rather than equating it with the clock.
        local=tuple(frame.motion.motion_positions[0].local_time for frame in result.value.frames)
        assert all(abs(actual-expected)<1e-5 for actual,expected in zip(local,(0.,14/60,29/60,14/60))),local
        assert all(abs(actual-10*t)<1e-5 for actual,t in zip(x,local)),x
        assert all(frame.preview_id==movie.id and frame.sequence==result.value.sequence for frame in result.value.frames)
        assert all(frame.motion.part_opacities[uid(5)]==1. for frame in result.value.frames)
        frozen_manual=manual.seek(.5).wait()
        assert abs(frozen_manual.value.frames[0].geometry.drawables[0].positions[0].x-7.5)<1e-5
        assert movie.info().value.time==.25
        movie.apply(a.Reset())
        movie.apply(a.ScheduleExpression(id=uid(20),time=0.))
        expression=movie.seek(.5).wait()
        assert expression.value.frames[0].motion.parameters[uid(2)]==.75
        assert abs(expression.value.frames[0].geometry.drawables[0].positions[0].x-8.75)<1e-5
        assert manual.info().value.time==.5
        assert session.info()==before
        # A candidate captures its sequence before later writes/cancellation.
        edit=session.edit("Frozen candidate")
        candidate=owned.enter_context(edit.preview())
        captured=candidate.source
        edit.parameters.create(parameters.ParameterSpec("Only candidate",-1.,1.,0.,id=uid(40)))
        later=owned.enter_context(edit.preview())
        assert later.source.sequence>captured.sequence
        edit.cancel()
        assert uid(40) not in candidate.seek(0.,geometry=False).wait().value.frames[0].motion.parameters
        assert uid(40) in later.seek(0.,geometry=False).wait().value.frames[0].motion.parameters
        assert session.info()==before
        try: manual.apply(a.SetParameter(id=uid(999),value=0.))
        except zipzi.SdkFailure: pass
        else: raise AssertionError("Missing preview parameter accepted")
        assert manual.seek(.5,geometry=False).wait().value.frames[0].geometry is None
        # Formal publication leaves captured views on their old source.
        with session.edit("New default") as edit:
            edit.parameters.patch(uid(2),parameters.ParameterPatch(default_value=-.5))
        fresh=owned.enter_context(session.preview())
        assert fresh.source.version!=manual.source.version
        assert fresh.seek(0.,geometry=False).wait().value.frames[0].motion.parameters[uid(2)]==-.5
        assert manual.seek(0.,geometry=False).wait().value.frames[0].motion.parameters[uid(2)]==.5
        incremental=manual.advance_to(.5,geometry=False).wait().value.frames[0]
        assert incremental.motion.parameters[uid(2)]==.5
    assert runtime.resources.usage().previews==0
    # Repeated release returns the actual Preview table to baseline in one session.
    for _ in range(100):
        with session.preview() as preview: assert preview.info().value.time==0.
    assert runtime.resources.usage().previews==0
    return dict(motion_x=x,expression_x=8.75,manual_x=7.5,independent_views=True,candidate_sequence=True,releases=100)
