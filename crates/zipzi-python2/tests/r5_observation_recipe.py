"""Actual GPU pixels, texture identities and exact frozen formal/candidate/Preview sources."""
from dataclasses import replace
import hashlib
from pathlib import Path
import zipzi
from zipzi import animation as a, bindings, meshes, modeling as m, observation as o, parameters

def uid(n:int)->str:return f"79660000-0000-4000-8000-{n:012x}"
VIEW=o.View(width=64,height=32,roi=(0.,0.,2.,1.))
GEOMETRY=meshes.Geometry((10,20,30,40),((0.,0.),(1.,0.),(1.,1.),(0.,1.)),
    ((0.,0.),(1.,0.),(1.,1.),(0.,1.)),((10,20,30),(10,30,40)))
def fixture(runtime:zipzi.Runtime)->zipzi.Session:
    session=runtime.sessions.create(uid(1))
    with runtime.resources.prepare_image(1,1,bytes((255,0,0,255))) as image:
        with session.edit("Render fixture") as edit:
            edit.resources.create_image("Red",image,id=uid(2))
            edit.meshes.create(meshes.MeshSpec("Quad",GEOMETRY,id=uid(3),texture_asset_id=uid(2)))
            edit.parameters.create(parameters.ParameterSpec("P",0.,1.,0.,id=uid(4),runtime_id="P"))
            edit.bindings.create(bindings.MeshBindingSpec(uid(3),(bindings.Axis(uid(4),(0.,1.)),),(
                bindings.Keyform((0.,),GEOMETRY.positions),bindings.Keyform((1.,),tuple((x+1.,y) for x,y in GEOMETRY.positions))),id=uid(5)))
            edit.apply(m.commands.ModelingCommandMotion(intent=m.motion.MotionEditCreate(id=uid(10),name="Move",registration_id=uid(11),destination=m.motion.MotionContainerRoot())))
            clip=edit.modeling.records(m.ModelingQuery(motions=(uid(10),))).value.motions[0]
            clip=replace(clip,duration=1.,fade_in=0.,fade_out=0.,tracks=(m.motion.MotionTrack(id=uid(12),target=m.motion.MotionTrackTargetParameter(parameter_id=uid(4)),
                initial=m.motion.MotionKey(id=uid(13),time=0.,value=0.),segments=(m.motion.MotionSegmentLinear(end=m.motion.MotionKey(id=uid(14),time=1.,value=1.)),),extensions={}),))
            edit.apply(m.commands.ModelingCommandReplaceMotion(clip=clip))
            edit.apply(m.commands.ModelingCommandMotion(intent=m.motion.MotionEditCreate(id=uid(20),name="Transparent",registration_id=uid(21),destination=m.motion.MotionContainerRoot())))
            opacity=edit.modeling.records(m.ModelingQuery(motions=(uid(20),))).value.motions[0]
            opacity=replace(opacity,duration=1.,fade_in=0.,fade_out=0.,tracks=(m.motion.MotionTrack(id=uid(22),target=m.motion.MotionTrackTargetModel(runtime_id="Opacity"),
                initial=m.motion.MotionKey(id=uid(23),time=0.,value=0.),segments=(),extensions={}),))
            edit.apply(m.commands.ModelingCommandReplaceMotion(clip=opacity))
    return session

def replace_image(runtime:zipzi.Runtime,edit:zipzi.Edit,rgba:bytes)->None:
    with runtime.resources.prepare_image(1,1,rgba) as image:edit.resources.replace_image(uid(2),image)

def pixel(image:o.Image,x:int=16,y:int=16)->bytes:
    index=(y*image.width+x)*4;return image.rgba[index:index+4]

def run(runtime:zipzi.Runtime,folder:Path)->dict[str,object]:
    session=fixture(runtime);before=session.info()
    assert "observation" in runtime.capabilities().domains
    preview=session.preview()
    job=session.modeling.observe(({}, {uid(4):1.},{}),VIEW)
    with session.edit("Green") as edit:replace_image(runtime,edit,bytes((0,255,0,255)))
    observed=job.wait();assert observed.source.version==before.version
    images=observed.value.frames
    assert pixel(images[0])==bytes((255,0,0,255)) and pixel(images[1],48)==bytes((255,0,0,255))
    assert pixel(images[1])==bytes(4) and images[0].rgba==images[2].rgba
    assert images[0].capture_id==images[1].capture_id and images[0].textures==images[1].textures
    assert images[0].render_digest==images[2].render_digest!=images[1].render_digest
    assert images[0].visible_roi==(0.,0.,2.,1.) and images[0].scale==32. and images[0].offset==(0.,32.)
    assert images[0].png[:8]==b"\x89PNG\r\n\x1a\n" and len(images[0].rgba)==64*32*4
    capabilities=runtime.capabilities();assert capabilities.gpu is True and capabilities.gpu_error is None
    assert all(frame.adapter_name and frame.adapter_backend for frame in images)
    edit=session.edit("Blue candidate")
    replace_image(runtime,edit,bytes((0,0,255,255)))
    source=edit.resources.get(uid(2)).source
    candidate_preview=edit.preview(expected=source)
    blue=edit.modeling.observe(({},),VIEW,expected=source)
    replace_image(runtime,edit,bytes((255,255,0,255)));edit.cancel()
    candidate=blue.wait();assert candidate.source==source and pixel(candidate.value.frames[0])==bytes((0,0,255,255))
    candidate_frame=candidate_preview.seek(.25,geometry=False,view=VIEW).wait()
    assert candidate_frame.source==source and pixel(candidate_frame.value.frames[0].observation)==bytes((0,0,255,255))
    candidate_preview.close()
    assert candidate.value.frames[0].textures[0].sha256!=images[0].textures[0].sha256
    after=session.info()
    preview.apply(a.ScheduleMotion(id=uid(10),time=0.))
    moving=preview.sample((0.,.25,.5,.25),geometry=False,view=VIEW).wait()
    assert moving.source==preview.source and moving.source.version==before.version and moving.value.view==VIEW
    positions=[]
    for frame in moving.value.frames:
        assert frame.geometry is None and frame.observation is not None
        image=frame.observation;local=frame.motion.motion_positions[0].local_time;positions.append(local)
        assert abs(image.parameters[uid(4)]-local)<1e-6
        assert pixel(image,int((local+.5)*32))==bytes((255,0,0,255))
        assert image.textures==images[0].textures
    preview.apply(a.Reset());preview.apply(a.ScheduleMotion(id=uid(20),time=0.))
    transparent=preview.seek(.25,geometry=False,view=VIEW).wait().value.frames[0]
    assert transparent.motion.model_opacity==0. and transparent.observation.rgba==bytes(64*32*4)
    preview.close()
    assert runtime.resources.usage().previews==0 and session.info()==after
    green=session.modeling.observe(({},),VIEW).wait()
    assert pixel(green.value.frames[0])==bytes((0,255,0,255))
    # File organization is client-side and works after Runtime release.
    from zipzi import inspection
    report=inspection.save(observed,folder/"formal")
    animated=inspection.save(moving,folder/"animation")
    assert report.images[0].read_bytes()==images[0].png and len(animated.images)==4
    assert report.manifest.is_file() and animated.manifest.is_file()
    return dict(adapter_backend=images[0].adapter_backend,red_rgba_sha256=hashlib.sha256(images[0].rgba).hexdigest(),png_sha256=hashlib.sha256(images[0].png).hexdigest(),
        frozen_candidate=True,frozen_candidate_preview=True,preview_times=positions,texture_hash=images[0].textures[0].sha256,model_opacity_zero=True,artifacts=(len(report.images),len(animated.images)))
