//! Installed public Python clients publish into the actual native Editor view.
use super::harness::{Harness, update};
use anyhow::{Context, ensure};
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use zipzi_editor_core::{Editor, EditorCommand};

fn client(
    h: &mut Harness,
    python: &Path,
    endpoint: &Path,
    script: &str,
    args: &[String],
) -> anyhow::Result<()> {
    let mut child = Command::new(python)
        .args(["-I", "-c", script])
        .args(args)
        .env_remove("PYTHONPATH")
        .env("ZIPZI_RUNTIME_DIR", endpoint)
        .env("ZIPZI_RUNTIME_PROFILE", "test")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    wait(h, &mut child)?;
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "Python client failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(())
}
fn wait(h: &mut Harness, child: &mut Child) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        h.cx.advance_clock(Duration::from_millis(2));
        h.cx.run_until_parked();
        if child.try_wait()?.is_some() {
            return Ok(());
        }
        if Instant::now() > deadline {
            let _ = child.kill();
            anyhow::bail!("Python client did not finish");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
pub(super) fn run() -> anyhow::Result<()> {
    let python = std::env::var_os("ZIPZI_CLIENT_PYTHON")
        .context("ZIPZI_CLIENT_PYTHON must point to an isolated installed client")?;
    let python = Path::new(&python);
    ensure!(python.is_absolute(), "client interpreter must be absolute");
    let endpoint = tempfile::Builder::new()
        .prefix("zzipc-")
        .tempdir_in("/tmp")?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(endpoint.path(), std::fs::Permissions::from_mode(0o700))?;
    let mut h = Harness::new()?;
    let host = crate::host::DesktopHost::new(false);
    let (document, _assets) = super::fixture::sample_document()?;
    let runtime = host.runtime.clone();
    h.cx.update(|cx| {
        cx.set_global(host.clone());
        let owner = zipzi_runtime::local::Endpoint::new(endpoint.path(), "test")?
            .claim()?
            .context("endpoint occupied")?;
        crate::ipc::start(owner, cx)?;
        Ok::<_, anyhow::Error>(())
    })?;
    update(&mut h, |view, window, cx| {
        view.host = Some(host.clone());
        view.replace_editor(Editor::from_document(document), cx);
        view.refresh_document(window, cx);
    })?;
    let (session, mesh, before, revision) = h.view.read_with(&h.cx, |view, _| {
        let document = view.editor.document().unwrap();
        (
            document.runtime_session_id(),
            view.editor.layers()[0].id.clone(),
            document.history_lengths(),
            document.properties_revision(),
        )
    });
    let event_cursor = runtime.read().subscribe_info(session)?.cursor;
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
import sys,zipzi
from zipzi import bindings,events,ops,parameters,scene
from importlib.metadata import version
from importlib.util import find_spec
assert version('zipzi-python2')=='0.1.0'
assert '/site-packages/zipzi/' in zipzi.__file__
assert find_spec('zipzi_native') is None
with zipzi.connect() as c:
 s=c.sessions.attach(zipzi.SessionId(sys.argv[1],int(sys.argv[2])))
 assert s.id in {info.id for info in c.sessions.list()}
 stream=s.subscribe()
 geometry=s.meshes.get(sys.argv[3]).geometry
 parameter='79700000-0000-4000-8000-000000000001'
 binding='79700000-0000-4000-8000-000000000002'
 with s.edit('external Python') as e:
  e.apply_many((ops.Rename(sys.argv[3],'Python changed this mesh'),
   ops.CreateParameter(parameters.ParameterSpec('External Angle',-30.,30.,0.,id='79700000-0000-4000-8000-000000000001',runtime_id='ParamExternal',decimal_places=3)),
   ops.CreateMeshBinding(bindings.MeshBindingSpec(sys.argv[3],(bindings.Axis(parameter,(-30.,30.)),),(
    bindings.Keyform((-30.,),geometry.positions),
    bindings.Keyform((30.,),tuple((x+10.,y) for x,y in geometry.positions))),id=binding))))
  candidate=e.bindings.get(binding)
  assert candidate.source.sequence==1
  sampled=e.modeling.sample(({parameter:15.},),expected=candidate.source)
  assert abs(sampled.frames[0].drawables[0].positions[0][0]-geometry.positions[0][0]-7.5)<1e-5
  assert e.parameters.get_many((parameter,)).items[0].source.sequence==1
  group='79700000-0000-4000-8000-000000000003'
  part='79700000-0000-4000-8000-000000000004'
  e.apply_many((scene.CreatePart(scene.PartSpec('External folder',id=part)),
   scene.CreateParameterGroup(scene.ParameterGroupSpec('External controls',id=group)),
   scene.MoveParameters((parameter,),group_id=group),
   scene.MoveToPart((sys.argv[3],),part_id=part)))
  assert e.scene.get(sys.argv[3]).value.part_id==part
 before=s.info()
 formal=s.modeling.sample(({parameter:-30.},{parameter:0.},{parameter:30.}))
 assert [round(f.drawables[0].positions[0][0]-geometry.positions[0][0],5) for f in formal.frames]==[0.,5.,10.]
 assert s.info()==before
 assert s.meshes.get(sys.argv[3]).name=='Python changed this mesh'
 assert s.meshes.get_many((sys.argv[3],))[0].name=='Python changed this mesh'
 with s.reference('mesh',sys.argv[3]) as reference:
  assert reference.resolve().version==s.info().version
  assert reference.scene_capabilities().patch.state=='available'
  with reference.label() as address:
   assert address.expected=='Python changed this mesh'
   with address.plan(address.expected) as plan: assert plan.report.no_op
 published=[e for e in stream.poll() if isinstance(e,events.Published)]
 assert len(published)==1 and published[0].receipt.before==stream.initial.version
 assert published[0].receipt.after==s.info().version
"#,
        &[
            session.runtime_instance().to_string(),
            session.serial().to_string(),
            mesh.clone(),
        ],
    )?;
    h.cx.run_until_parked();
    h.view.read_with(&h.cx, |view, _| {
        let document = view
            .editor
            .document()
            .context("external job left view busy")?;
        ensure!(
            document.properties_revision() > revision,
            "properties projection did not advance"
        );
        ensure!(
            view.editor
                .layers()
                .iter()
                .any(|l| l.id == mesh && l.name == "Python changed this mesh"),
            "layer projection not updated"
        );
        ensure!(
            document.history_lengths() == (before.0 + 1, 0),
            "duplicate/missing history"
        );
        ensure!(
            document
                .parameters()
                .iter()
                .any(|p| p.id == "79700000-0000-4000-8000-000000000001"
                    && p.name == "External Angle"
                    && p.minimum == -30.
                    && p.maximum == 30.
                    && p.decimal_places == 3
                    && p.group_id.as_deref() == Some("79700000-0000-4000-8000-000000000003")),
            "parameter projection did not update"
        );
        ensure!(
            document
                .part_tree()
                .iter()
                .any(|node| node.id == "79700000-0000-4000-8000-000000000004"
                    && node.name == "External folder"),
            "Part tree projection did not update"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    // A human preview remains selected while a second process samples formal
    // and temporary candidate poses. Neither read may publish or move the UI.
    update(&mut h, |view, _, cx| {
        view.editor
            .execute(EditorCommand::SetPreviewParameter {
                parameter_id: "79700000-0000-4000-8000-000000000001".into(),
                value: -15.,
            })
            .unwrap();
        view.redraw(cx);
    })?;
    let preview_revision = h.view.read_with(&h.cx, |view, _| {
        view.editor.document().unwrap().preview_revision()
    });
    let version = runtime.read().session_info(session)?.version;
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
import sys,zipzi
from zipzi import bindings
with zipzi.connect() as c:
 s=c.sessions.attach(zipzi.SessionId(sys.argv[1],int(sys.argv[2])))
 parameter='79700000-0000-4000-8000-000000000001'
 binding='79700000-0000-4000-8000-000000000002'
 geometry=s.meshes.get(sys.argv[3]).geometry
 before=s.info()
 stream=s.subscribe()
 sampled=s.modeling.sample(({parameter:15.},))
 assert abs(sampled.frames[0].drawables[0].positions[0][0]-geometry.positions[0][0]-7.5)<1e-5
 edit=s.edit('temporary CPU sample')
 try:
  edit.bindings.set_keyform(binding,bindings.Keyform((30.,),tuple((x+20.,y) for x,y in geometry.positions)))
  candidate=edit.modeling.sample(({parameter:15.},))
  assert abs(candidate.frames[0].drawables[0].positions[0][0]-geometry.positions[0][0]-15.)<1e-5
 finally: edit.cancel()
 assert s.info()==before
 assert stream.poll()==()
"#,
        &[
            session.runtime_instance().to_string(),
            session.serial().to_string(),
            mesh.clone(),
        ],
    )?;
    ensure!(
        runtime.read().session_info(session)?.version == version,
        "sampling changed formal version"
    );
    h.view.read_with(&h.cx, |view, _| {
        let document = view.editor.document().unwrap();
        ensure!(
            document.preview_revision() == preview_revision,
            "external CPU sample advanced UI preview"
        );
        ensure!(
            document
                .parameters()
                .iter()
                .any(|p| p.id == "79700000-0000-4000-8000-000000000001" && p.value == -15.),
            "external CPU sample replaced human preview pose"
        );
        ensure!(
            document.history_lengths() == (before.0 + 1, 0),
            "sampling changed shared history"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    // Human undo is immediately visible to another real Python process.
    update(&mut h, |view, _, cx| {
        view.editor.execute(EditorCommand::Undo).unwrap();
        view.redraw(cx);
    })?;
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
import json,sys,zipzi
from zipzi import events
with zipzi.connect() as c:
 s=c.sessions.attach(zipzi.SessionId(sys.argv[1],int(sys.argv[2])))
 assert s.meshes.get(sys.argv[3]).name!='Python changed this mesh'
 assert s.info().history[1]==1
 published=[e for e in c.events_since(events.Cursor.read(json.loads(sys.argv[4]))) if isinstance(e,events.Published)]
 assert len(published)==2
 assert published[0].receipt.after==published[1].receipt.before
 assert published[1].receipt.after==s.info().version
 try: s.parameters.get('79700000-0000-4000-8000-000000000001')
 except zipzi.SdkFailure as error: assert error.code=='MISSING_PARAMETER'
 else: raise AssertionError('UI Undo retained the new parameter')
 try: s.bindings.get('79700000-0000-4000-8000-000000000002')
 except zipzi.SdkFailure as error: assert error.code=='MISSING_BINDING'
 else: raise AssertionError('UI Undo retained the new binding')
"#,
        &[
            session.runtime_instance().to_string(),
            session.serial().to_string(),
            mesh.clone(),
            serde_json::to_string(&event_cursor)?,
        ],
    )?;
    // UI owns an in-flight Save As; the external entry cannot bypass its target.
    let target = endpoint.path().join("project");
    let task = h.cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |view, _| {
            view.editor
                .prepare_save(view.editor.active_id().unwrap(), &target, false)
        })
    })??;
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
import sys,uuid,zipzi
with zipzi.connect() as c:
 s=c.sessions.create(str(uuid.uuid4()))
 try: s.project.save(__import__('pathlib').Path(sys.argv[1])).wait()
 except zipzi.Busy: pass
 else: raise AssertionError('cross-session target reservation bypassed')
"#,
        &[target.to_string_lossy().into_owned()],
    )?;
    ensure!(
        !target.exists(),
        "rejected external save modified UI target"
    );
    let mut completion = task.run();
    completion.publish(&runtime);
    update(&mut h, |view, _, cx| {
        view.editor.finish_save(&mut completion).unwrap();
        view.redraw(cx);
    })?;
    ensure!(
        runtime.read().session_info(session)?.state == zipzi_runtime::SessionState::Ready,
        "save stayed busy"
    );
    // Extend the actual UI projection across every R4 scene/resource family.
    update(&mut h, |view, _, cx| {
        use zipzi_editor_core::{ObjectKind, ObjectRef};
        view.editor
            .execute(EditorCommand::SelectObjects(vec![ObjectRef::new(
                ObjectKind::Mesh,
                &mesh,
            )]))
            .unwrap();
        view.editor
            .execute(EditorCommand::Navigate(ObjectRef::new(
                ObjectKind::Mesh,
                &mesh,
            )))
            .unwrap();
        view.redraw(cx);
    })?;
    let texture_revision = h.view.read_with(&h.cx, |view, _| {
        view.editor.document().unwrap().mesh_texture_revision()
    });
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
import sys,time,zipzi
from dataclasses import replace
from zipzi import scene,ops,meshes,parameters,modeling as m
u=lambda n:f'79800000-0000-4000-8000-{n:012x}'
with zipzi.connect() as c:
 s=c.sessions.attach(zipzi.SessionId(sys.argv[1],int(sys.argv[2])))
 mesh=sys.argv[3]
 context=s.editor_context()
 assert context.context is not None and context.context.version==s.info().version
 assert mesh in context.context.selected_ids
 geometry=s.meshes.get(mesh).geometry
 with c.resources.prepare_image(2,2,bytes((45,80,130,255))*4) as image:
  with s.edit('R4 complete projection') as e:
   e.apply_many((scene.CreatePart(scene.PartSpec('R4 Part',id=u(1))),
    scene.CreateParameterGroup(scene.ParameterGroupSpec('R4 Controls',id=u(2))),
    ops.CreateParameter(parameters.ParameterSpec('R4 Blend',0.,1.,0.,id=u(3),kind=parameters.Kind.BLEND_SHAPE)),
    scene.MoveParameters((u(3),),group_id=u(2)),
    ops.Create(meshes.MeshSpec('R4 Second',geometry,id=u(4))),
    scene.MoveToPart((mesh,u(4)),part_id=u(1)),
    scene.CreateWarp('R4 Warp',scene.Placement(part_id=u(1)),(-10.,-10.,20.,20.),id=u(5)),
    scene.CreateRotation('R4 Rotation',scene.Placement(part_id=u(1),parent_id=u(5)),id=u(6)),
    scene.CreateGlue('R4 Glue',mesh,u(4),id=u(7)),
    scene.InsertGluePair(u(7),scene.GluePair(.5,.5,id=u(8),vertex_a=geometry.vertex_ids[0],vertex_b=geometry.vertex_ids[1])),
    scene.CreateOffscreen('R4 Offscreen',u(1),id=u(9)),
    scene.InsertOffscreenKeyform(u(9),scene.OffscreenForm(.75)),
    scene.SetOffscreenStaticKeyform(u(9),0),
    scene.CreateBlendTable(u(3),(scene.Sample(u(10),0.),scene.Sample(u(11),1.)),u(10),id=u(12)),
    scene.CreateMeshBlend(mesh,u(12),id=u(13))))
   asset=e.resources.create_image('R4 memory texture',image,id=u(14))
   value=e.modeling.records(m.ModelingQuery(meshes=(mesh,))).values.meshes[0]
   e.apply(m.ModelingCommandReplaceMesh(mesh=replace(value,name='R4 property name',texture_asset_id=m.SceneReferenceResolved(object_id=asset))))
   e.apply(m.ModelingCommandAttachment(intent=m.AttachmentEditCreate(id=u(15),path='extras/projection.bin')))
 deadline=time.monotonic()+5
 while True:
  context=s.editor_context()
  if context.context is not None and context.context.version==s.info().version:break
  assert time.monotonic()<deadline
  time.sleep(.01)
 assert mesh in context.context.selected_ids
 assert s.resources.get(u(14)).pixels.rgba==bytes((45,80,130,255))*4
 assert s.info().history==(1,0)
"#,
        &[
            session.runtime_instance().to_string(),
            session.serial().to_string(),
            mesh.clone(),
        ],
    )?;
    h.cx.run_until_parked();
    h.view.read_with(&h.cx, |view, _| {
        use zipzi_editor_core::{ObjectKind, ObjectRef, RelationKind};
        let document = view
            .editor
            .document()
            .context("projection document missing")?;
        let index = document.object_index();
        let id = |n: u32| format!("79800000-0000-4000-8000-{n:012x}");
        for (kind, n) in [
            (ObjectKind::Part, 1),
            (ObjectKind::ParameterGroup, 2),
            (ObjectKind::Parameter, 3),
            (ObjectKind::Mesh, 4),
            (ObjectKind::Deformer, 5),
            (ObjectKind::Deformer, 6),
            (ObjectKind::Glue, 7),
            (ObjectKind::GlueVertexPair, 8),
            (ObjectKind::Offscreen, 9),
            (ObjectKind::BlendKeyTable, 12),
            (ObjectKind::BlendBinding, 13),
            (ObjectKind::Texture, 14),
            (ObjectKind::Attachment, 15),
        ] {
            ensure!(
                index.get(&ObjectRef::new(kind, id(n))).is_some(),
                "missing R4 projection {kind:?} {n}"
            );
        }
        let reference = ObjectRef::new(ObjectKind::Mesh, &mesh);
        ensure!(
            index
                .get(&reference)
                .is_some_and(|v| v.name == "R4 property name"),
            "property source did not update"
        );
        ensure!(
            index
                .outgoing(&reference)
                .any(|r| r.kind == RelationKind::Resource && r.target.id == id(14)),
            "texture relation did not update"
        );
        ensure!(
            index
                .incoming(&reference)
                .any(|r| r.kind == RelationKind::GlueA),
            "glue relation did not update"
        );
        ensure!(
            view.editor.active_inspection().and_then(|i| i.object()) == Some(reference),
            "external changes lost property selection"
        );
        ensure!(
            document.mesh_texture_revision() > texture_revision,
            "memory texture projection did not advance"
        );
        Ok::<_, anyhow::Error>(())
    })?;
    // A real UI opacity gesture remains owned by the human. The public Python
    // candidate can compute, receives Busy, then rejects its obsolete base.
    let ready = endpoint.path().join("gesture-ready");
    let busy = endpoint.path().join("gesture-busy");
    let retry = endpoint.path().join("gesture-retry");
    let finish = endpoint.path().join("gesture-finished");
    let mut child = Command::new(python)
        .args([
            "-I",
            "-c",
            r#"
import sys,time,zipzi
from pathlib import Path
from zipzi import ops,modeling as m
ready,busy,retry,finish=map(Path,sys.argv[4:])
def wait(path):
 deadline=time.monotonic()+15
 while not path.exists():
  assert time.monotonic()<deadline
  time.sleep(.005)
with zipzi.connect() as c:
 s=c.sessions.attach(zipzi.SessionId(sys.argv[1],int(sys.argv[2])))
 e=s.edit('obsolete external candidate')
 e.apply(ops.Rename(sys.argv[3],'must never publish'))
 ready.write_text('candidate prepared')
 wait(retry)
 try:e.commit()
 except zipzi.Busy:pass
 else:raise AssertionError('human gesture was not protected')
 busy.write_text('Busy preserves candidate')
 wait(finish)
 try:e.commit().wait()
 except zipzi.SdkFailure as error:assert error.code=='STALE_VERSION',error.code
 else:raise AssertionError('obsolete candidate committed')
 assert s.meshes.get(sys.argv[3]).name=='R4 property name'
 value=s.modeling.records(m.ModelingQuery(meshes=(sys.argv[3],))).values.meshes[0]
 assert abs(value.appearance.opacity-.61)<1e-6
"#,
        ])
        .args([
            session.runtime_instance().to_string(),
            session.serial().to_string(),
            mesh.clone(),
            ready.to_string_lossy().into_owned(),
            busy.to_string_lossy().into_owned(),
            retry.to_string_lossy().into_owned(),
            finish.to_string_lossy().into_owned(),
        ])
        .env_remove("PYTHONPATH")
        .env("ZIPZI_RUNTIME_DIR", endpoint.path())
        .env("ZIPZI_RUNTIME_PROFILE", "test")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let wait_file = |h: &mut Harness, path: &Path| -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !path.exists() {
            h.cx.advance_clock(Duration::from_millis(2));
            h.cx.run_until_parked();
            ensure!(
                Instant::now() < deadline,
                "gesture client did not reach {}",
                path.display()
            );
            std::thread::sleep(Duration::from_millis(2));
        }
        Ok(())
    };
    wait_file(&mut h, &ready)?;
    update(&mut h, |view, _, cx| {
        view.editor
            .execute(EditorCommand::Select(mesh.clone()))
            .unwrap();
        view.editor
            .execute(EditorCommand::PreviewOpacity(0.61))
            .unwrap();
        view.redraw(cx);
    })?;
    std::fs::write(&retry, "UI interaction active")?;
    wait_file(&mut h, &busy)?;
    ensure!(
        h.view.read_with(&h.cx, |view, _| view
            .editor
            .document()
            .unwrap()
            .has_pending_edit()),
        "Busy cancelled the human gesture"
    );
    update(&mut h, |view, _, cx| {
        view.editor.execute(EditorCommand::FinishEdit).unwrap();
        view.redraw(cx);
    })?;
    std::fs::write(&finish, "UI gesture published")?;
    wait(&mut h, &mut child)?;
    let output = child.wait_with_output()?;
    ensure!(
        output.status.success(),
        "gesture client failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    // Render through Metal after the external publication/UI history cycle.
    let image = h.capture()?;
    ensure!(
        image.width() > 0 && image.height() > 0,
        "native render missing"
    );
    println!(
        "R4 native IPC: installed zipzi-python2 -> mesh/parameter/binding/scene/group -> SDK reference/address -> candidate/formal CPU sweep -> human preview unchanged -> cursor events/UI Undo -> path reservation -> Metal render passed"
    );
    Ok(())
}
