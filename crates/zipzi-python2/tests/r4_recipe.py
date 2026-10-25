"""Public installed R4 acceptance; never modifies import paths or loads source clients."""
from dataclasses import replace
from pathlib import Path
import hashlib
import json
import zipzi
from zipzi import modeling as m, meshes, bindings, parameters, scene

def uid(n: int) -> str:
    return f"79900000-0000-4000-8000-{n:012x}"

def run(runtime: zipzi.Runtime, root: Path, folder: Path) -> dict[str, object]:
    folder.mkdir()
    session=runtime.sessions.create(uid(1))
    assert session.editor_context().context is None
    # A separate earlier recipe may leave collectible resource-store entries.
    # Sweep them before establishing this resident session's 100-round baseline.
    with runtime.resources.prepare_image(1,1,bytes((255,9,7,255))) as image:
        warmup=session.edit("resource baseline sweep")
        warmup.resources.create_image("warmup",image,id=uid(2))
        warmup.cancel()
    baseline=runtime.resources.usage()
    before=session.info()
    for n in range(100):
        with runtime.resources.prepare_image(1,1,bytes((n,11,13,255))) as image:
            edit=session.edit("resident cancellation")
            edit.resources.create_image("draft",image,id=uid(2))
            edit.cancel()
        usage=runtime.resources.usage()
        assert usage.rgba_bytes==baseline.rgba_bytes and usage.resource_entries==baseline.resource_entries,(baseline,usage,n)
    assert session.info()==before
    with runtime.resources.prepare_image(1,1,bytes((0,11,13,255))) as image:
        with session.edit("retain history") as edit:edit.resources.create_image("image",image,id=uid(2))
    pinned=session.resources.snapshot(uid(2))
    for n in range(1,101):
        with runtime.resources.prepare_image(1,1,bytes((n,11,13,255))) as image:
            with session.edit("replace") as edit:edit.resources.replace_image(uid(2),image)
        usage=runtime.resources.usage()
        assert usage.resource_entries<=baseline.resource_entries+52
        assert usage.rgba_bytes<=baseline.rgba_bytes+52*4
    assert session.info().history==(50,0)
    assert pinned.pixels().rgba==bytes((0,11,13,255))
    pinned.close()
    after=runtime.resources.usage()
    assert after.rgba_bytes==baseline.rgba_bytes+51*4
    assert after.resource_entries==baseline.resource_entries+51
    # A real PSD becomes an unsaved document with in-memory artwork.
    imported=runtime.sessions.import_(root/"crates/zipzi-psd/tests/fixtures/layered.psd",m.ImportFormat.PSD).wait()
    records=imported.modeling.records(m.ModelingQuery())
    asset=records.values.ids["assets"][0]
    pixels=imported.resources.get(asset).pixels.rgba
    with imported.edit("headless authoring") as edit:
        edit.apply(scene.CreatePart(scene.PartSpec("Headless part",id=uid(3))))
        for n,runtime_id in ((20,"R4Param"),(21,"R4X"),(22,"R4Y")):
            edit.parameters.create(parameters.ParameterSpec(runtime_id,-1.,1.,0.,id=uid(n),runtime_id=runtime_id))
        for n,runtime_id in ((23,"R4Pose0"),(24,"R4Pose1")):
            edit.apply(m.ModelingCommandCreatePart(part=m.Part(id=uid(n),runtime_id=runtime_id,name=runtime_id,parent_id="",enabled=True,draw_order=0.)))
    saved=imported.project.save(folder/"project").wait()
    reopened=imported.project.open(saved.manifest).wait()
    assert reopened.after.generation>reopened.before.generation
    assert imported.resources.get(asset).pixels.rgba==pixels
    assert imported.project.diagnose().wait().diagnostics==()
    for format,name in ((m.ExportFormat.CMO3,"model.cmo3"),(m.ExportFormat.CAN3,"model.can3"),(m.ExportFormat.CUBISM,"cubism"),(m.ExportFormat.PACKAGE,"package"),(m.ExportFormat.CDI3,"model.cdi3.json")):
        report=imported.project.export(folder/name,format,allow_loss=True).wait()
        assert report.published and (folder/name).exists()
    imported.project.replace(folder/"model.cmo3",m.ImportFormat.CUBISM).wait()
    assert imported.modeling.records(m.ModelingQuery()).values.ids["assets"]
    imported.project.replace(folder/"package/model.model3.json",m.ImportFormat.MODEL3).wait()
    imported.project.replace(folder/"package/model.moc3",m.ImportFormat.MOC3,textures=(m.TextureSlot(slot=0,path=str(folder/"package/textures/0.png")),)).wait()
    assert imported.modeling.records(m.ModelingQuery()).values.ids["meshes"]
    # All existing sidecar export routes use content imported through Model3.
    motion={"Version":3,"Meta":{"Duration":1.,"Fps":30.,"Loop":False,"AreBeziersRestricted":True,"CurveCount":1,"TotalSegmentCount":1,"TotalPointCount":2,"UserDataCount":0,"TotalUserDataSize":0},"Curves":[{"Target":"Model","Id":"Opacity","Segments":[0.,1.,0,1.,.5]}]}
    expression={"Type":"Live2D Expression","Parameters":[{"Id":"R4Param","Value":.5,"Blend":"Add"}]}
    physics={'Version': 3, 'Meta': {'PhysicsSettingCount': 1, 'TotalInputCount': 1, 'TotalOutputCount': 1, 'VertexCount': 2, 'EffectiveForces': {'Gravity': {'X': 0, 'Y': -1}, 'Wind': {'X': 0, 'Y': 0}}, 'PhysicsDictionary': [{'Id': 'PhysicsProbe', 'Name': 'Probe'}], 'Fps': 0}, 'PhysicsSettings': [{'Id': 'PhysicsProbe', 'Input': [{'Source': {'Target': 'Parameter', 'Id': 'R4X'}, 'Weight': 100, 'Type': 'X', 'Reflect': False}], 'Output': [{'Destination': {'Target': 'Parameter', 'Id': 'R4Y'}, 'VertexIndex': 1, 'Scale': 1, 'Weight': 100, 'Type': 'Angle', 'Reflect': False}], 'Vertices': [{'Position': {'X': 0, 'Y': 0}, 'Mobility': 1, 'Delay': 1, 'Acceleration': 1, 'Radius': 0}, {'Position': {'X': 0, 'Y': 1}, 'Mobility': 0.8, 'Delay': 0.9, 'Acceleration': 1.5, 'Radius': 1}], 'Normalization': {'Position': {'Minimum': -1, 'Default': 0, 'Maximum': 1}, 'Angle': {'Minimum': -1, 'Default': 0, 'Maximum': 1}}}]}
    pose={'Type': 'Live2D Pose', 'FadeInTime': 0.5, 'Groups': [[{'Id': 'R4Pose0'}, {'Id': 'R4Pose1'}]]}
    for name,value in (("input.motion3.json",motion),("input.exp3.json",expression),("input.physics3.json",physics),("input.pose3.json",pose)):(folder/name).write_text(json.dumps(value))
    model={"Version":3,"FileReferences":{"Moc":"package/model.moc3","Textures":["package/textures/0.png"],"Motions":{"Idle":[{"File":"input.motion3.json"}]},"Expressions":[{"Name":"Test","File":"input.exp3.json"}],"Physics":"input.physics3.json","Pose":"input.pose3.json"}}
    (folder/"pipeline.model3.json").write_text(json.dumps(model))
    imported.project.replace(folder/"pipeline.model3.json",m.ImportFormat.MODEL3).wait()
    io_ids=imported.modeling.records(m.ModelingQuery()).values.ids
    for format,name,identity in ((m.ExportFormat.MOTION3,"output.motion3.json",io_ids["motions"][0]),(m.ExportFormat.EXPRESSION3,"output.exp3.json",io_ids["expressions"][0]),(m.ExportFormat.PHYSICS3,"output.physics3.json",None),(m.ExportFormat.POSE3,"output.pose3.json",None)):
        before=imported.info()
        report=imported.project.export(folder/name,format,object_id=identity).wait()
        assert report.published and json.loads((folder/name).read_text())
        assert imported.info()==before
    # SDK rectangle-grid boundary: existing bindings survive an explicit full
    # topology replacement. Both transports return actual numeric arrays.
    n=256
    positions=tuple((float(x),float(y)) for y in range(n) for x in range(n))
    ids=tuple(range(n*n))
    triangles=tuple(t for y in range(n-1) for x in range(n-1) for t in ((y*n+x,y*n+x+1,(y+1)*n+x+1),(y*n+x,(y+1)*n+x+1,(y+1)*n+x)))
    corners=((0.,0.),(255.,0.),(255.,255.),(0.,255.))
    rectangle=meshes.Geometry((0,1,2,3),corners,((0.,0.),(1.,0.),(1.,1.),(0.,1.)),((0,1,2),(0,2,3)))
    large=runtime.sessions.create(uid(4))
    with large.edit("SDK limit geometry") as edit:
        mesh=edit.meshes.create(meshes.MeshSpec("Limit",rectangle,id=uid(5)))
        parameter=edit.parameters.create(parameters.ParameterSpec("P",0.,1.,0.,id=uid(6)))
        binding=edit.bindings.create(bindings.MeshBindingSpec(mesh,(bindings.Axis(parameter,(0.,1.)),),(bindings.Keyform((0.,),corners),bindings.Keyform((1.,),tuple((x+2.,y) for x,y in corners))),id=uid(7)))
        edit.apply(m.ModelingCommandRemeshRectangleGrid(id=mesh,columns=255,rows=255))
        generated=edit.meshes.get(mesh).geometry
        ids,triangles=generated.vertex_ids,generated.triangles
        assert generated.positions==positions
        assert edit.bindings.get(binding).keyforms[0].positions==positions
        sampled=edit.modeling.sample(({parameter:.5},))
        assert sampled.frames[0].drawables[0].positions[-1]==(256.,255.)
    before=large.info()
    try:
        with large.edit("outside SDK grid limit") as edit:edit.apply(m.ModelingCommandRemeshRectangleGrid(id=mesh,columns=256,rows=256))
    except zipzi.SdkFailure as error:assert error.code=="INVALID_RECTANGLE_GRID"
    else:raise AssertionError("SDK grid limit was ignored")
    assert large.info()==before
    selected=large.modeling.records(m.ModelingQuery(meshes=(mesh,),bindings=(binding,)))
    mesh_value=selected.values.meshes[0]
    assert mesh_value is not None
    source=m.GeometrySnapshot(version=selected.source.version,mesh_id=mesh,vertex_ids=mesh_value.vertex_ids,positions=mesh_value.base_positions,uvs=mesh_value.uvs,triangles=mesh_value.triangles,space=m.GeometrySpaceModel())
    replacement=m.TopologyReplacement(mesh=replace(mesh_value,triangles=tuple((c,b,a) for a,b,c in triangles)),binding=selected.values.bindings[0],blend_bindings=(),glues=(),vertex_mapping={v:v for v in ids})
    with large.edit("bound topology") as edit:edit.apply(m.ModelingCommandReplaceTopology(source=source,replacement=replacement))
    sampled=large.modeling.sample_async(({parameter:0.},{parameter:1.})).wait()
    assert sampled.frames[1].drawables[0].positions[-1]==(257.,255.)
    large.undo().wait()
    assert large.meshes.get(mesh).geometry.triangles==triangles
    large.redo().wait()
    diagnostics=tuple((d.kind.value,d.mesh_id,d.triangle_index) for d in sampled.diagnostics)
    return dict(vertices=len(ids),triangles=len(triangles),geometry_sha256=hashlib.sha256(json.dumps(sampled.frames[1].drawables[0].positions,separators=(",",":")).encode()).hexdigest(),diagnostics=diagnostics,history=large.info().history,cancel_iterations=100,replacement_iterations=100,retained_history_rgba_bytes=after.rgba_bytes-baseline.rgba_bytes,formats=9)
