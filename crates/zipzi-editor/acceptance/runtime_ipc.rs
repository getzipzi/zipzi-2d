//! Installed public Python clients publish into the actual native Editor view.
use super::harness::{Harness, update};
use anyhow::{Context, ensure};
use std::{
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use zipzi_editor_core::{Editor, EditorCommand};

const PYTHON_HELPERS: &str = r#"
import json, sys
from pathlib import Path
import zipzi
from zipzi import values as v

def address(kind, id):
    return v.ObjectAddress(kind=kind, id=id)

def record(owner, kind, id):
    return owner.query(v.QueryObjects(addresses=(address(kind, id),))).value['records'][0]['value']

def rename(id, name):
    return v.CommandRename(target=address(v.ObjectKind.MESH, id), name=name)

def mesh_record(owner, id):
    return v.Mesh.from_wire(record(owner, v.ObjectKind.MESH, id))

def history(session):
    h = session.info()['history']
    return h['undo_steps'], h['redo_steps']

def replace_image(runtime, draft, id, rgba, width=1, height=1):
    with runtime.blob(len(rgba)) as image:
        image.append(0, rgba)
        image.seal()
        draft.replace_image_blob(id, image, width=width, height=height)

def read_pixels(session, id):
    with session.image_blob(id) as image:
        return image.read(0, image.length)
def make_part(id, name):
    return v.Part(id=id, name=name, runtime_id='Part'+id.replace('-',''), parent_id='', enabled=True, draw_order=0.)

def make_group(id, name):
    return v.CdiParameterGroup(id=id,name=name,runtime_id='Group'+id.replace('-',''),extensions={})

def move_to_part(ids, part):
    return v.CommandOrganization(edit=v.OrganizationEditMove(ids=ids,
        destination=v.OrganizationCollectionPartMembers(part_id=part)))

def sample_x(session, mesh, values, source=None):
    request=v.SampleRequest(input=v.SampleInputParameters(values=values))
    with session.samples(request,source=source) as job:
        job.wait()
        with job.blob() as blob:
            frames=json.loads(blob.read(0,blob.length))['frames']
    return [next(d for d in frame['geometry']['drawables'] if d['id']==mesh)['positions'][0]['x']
            for frame in frames]
"#;

pub(super) fn client(
    h: &mut Harness,
    python: &Path,
    endpoint: &Path,
    script: &str,
    args: &[String],
) -> anyhow::Result<()> {
    let mut child = Command::new(python)
        .args(["-I", "-c", &format!("{PYTHON_HELPERS}\n{script}")])
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
    let (document, _assets) = super::fixture::sample_document()?;
    let (mut h, host, endpoint) = setup(document)?;
    let runtime = host.runtime.clone();
    let (session, mesh, before, revision) = h.view.read_with(&h.cx, |view, _| {
        let document = view.editor.document().unwrap();
        (
            document.runtime_session_id(),
            view.editor.layers()[0].id.clone(),
            document.history_lengths(),
            document.properties_revision(),
        )
    });
    let event_cursor = runtime
        .read()
        .subscribe(runtime.client_id(), session)?
        .cursor;
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
from importlib.util import find_spec
assert '/site-packages/zipzi/' in zipzi.__file__
assert find_spec('zipzi_native') is None
parameter='79700000-0000-4000-8000-000000000001'
binding='79700000-0000-4000-8000-000000000002'
group='79700000-0000-4000-8000-000000000003'
part='79700000-0000-4000-8000-000000000004'
with zipzi.connect() as c:
    s=c.sessions.attach(sys.argv[1])
    mesh=sys.argv[2]
    assert s.id in {info['session'] for info in c.sessions.list()}
    cursor=v.Version.from_wire(s.subscribe()['cursor'])
    positions=mesh_record(s,mesh).base_positions
    base=positions[0].x
    with s.draft('external Python') as e:
        e.apply(rename(mesh,'Python changed this mesh'),
            v.CommandCreateParameter(spec=v.ParameterCreateSpec(id=parameter, name='External Angle',
                minimum=-30.,maximum=30.,default_value=0.,runtime_id='ParamExternal',decimal_places=3)),
            v.CommandCreateBinding(spec=v.MeshBindingCreateSpec(id=binding, mesh_id=mesh,
                axes=(v.MeshBindingAxisSpec(parameter_id=parameter,keys=(-30.,30.)),),
                keyforms=(v.MeshKeyformSpec(keys=(-30.,),positions=positions),
                    v.MeshKeyformSpec(keys=(30.,),positions=tuple(v.Vec2(x=p.x+10.,y=p.y) for p in positions))))))
        assert e.source.sequence==1
        assert record(e,v.ObjectKind.MESH_BINDING,binding)['mesh_id']==mesh
        assert abs(sample_x(s,mesh,({parameter:15.},),source=v.QuerySourceDraft(value=e.source))[0]-base-7.5)<1e-5
        e.apply(
            v.CommandCreatePart(value=make_part(part,'External folder')),
            v.CommandOrganization(edit=v.OrganizationEditCreateGroup(group=make_group(group,'External controls'))),
            v.CommandOrganization(edit=v.OrganizationEditSetParameterGroup(parameter_id=parameter,group_id=group)),
            move_to_part((mesh,),part))
        assert mesh_record(e,mesh).part_id==part
    before=s.info()
    assert [round(x-base,5) for x in sample_x(s,mesh,tuple({parameter:x} for x in (-30.,0.,30.)))]==[0.,5.,10.]
    assert s.info()==before
    assert mesh_record(s,mesh).name=='Python changed this mesh'
    with s.draft('Confirm unchanged name') as e:
        e.preview(rename(mesh,'Python changed this mesh'))
        assert e.publish()['kind']=='unchanged'
    published=s.events(cursor)
    assert len(published)==1 and published[0]['kind']=='commit'
    assert published[0]['version']==s.info()['version']
"#,
        &[session.to_string(), mesh.clone()],
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
    let version = runtime
        .read()
        .session_read(runtime.client_id(), session)?
        .version();
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
parameter='79700000-0000-4000-8000-000000000001'
binding='79700000-0000-4000-8000-000000000002'
with zipzi.connect() as c:
    s=c.sessions.attach(sys.argv[1])
    mesh=sys.argv[2]
    positions=mesh_record(s,mesh).base_positions
    base=positions[0].x
    before=s.info()
    cursor=s.version
    assert abs(sample_x(s,mesh,({parameter:15.},))[0]-base-7.5)<1e-5
    e=s.draft('temporary CPU sample')
    try:
        e.apply(v.CommandMeshKeyform(binding=binding,value=v.MeshKeyformSpec(
            keys=(30.,),positions=tuple(v.Vec2(x=p.x+20.,y=p.y) for p in positions))))
        assert abs(sample_x(s,mesh,({parameter:15.},),source=v.QuerySourceDraft(value=e.source))[0]-base-15.)<1e-5
    finally:
        e.cancel()
    assert s.info()==before
    assert s.events(cursor)==()
"#,
        &[session.to_string(), mesh.clone()],
    )?;
    ensure!(
        runtime
            .read()
            .session_read(runtime.client_id(), session)?
            .version()
            == version,
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
with zipzi.connect() as c:
    s=c.sessions.attach(sys.argv[1])
    assert mesh_record(s,sys.argv[2]).name!='Python changed this mesh'
    assert history(s)==(0,1)
    published=s.events(v.Version.from_wire(json.loads(sys.argv[3])))
    assert [event['kind'] for event in published]==['commit','undo']
    assert published[0]['version']['revision']+1==published[1]['version']['revision']
    assert published[-1]['version']==s.info()['version']
    for kind,id in ((v.ObjectKind.PARAMETER,'79700000-0000-4000-8000-000000000001'),
                    (v.ObjectKind.MESH_BINDING,'79700000-0000-4000-8000-000000000002')):
        try: record(s,kind,id)
        except zipzi.SdkFailure as error: assert error.code=='MISSING_OBJECT'
        else: raise AssertionError('UI Undo retained '+id)
"#,
        &[
            session.to_string(),
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
with zipzi.connect() as c:
    s=c.sessions.create()
    try:
        s.save(Path(sys.argv[1]))
    except zipzi.Busy:
        pass
    else:
        raise AssertionError('cross-session target reservation bypassed')
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
        runtime
            .read()
            .session_catalog(runtime.client_id())?
            .iter()
            .any(|info| info.session == session && !info.saving),
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
import time
from dataclasses import replace
u=lambda n:f'79800000-0000-4000-8000-{n:012x}'
def scene(n,name,object,part=None,parent=None):
    return v.CommandScene(edit=v.SceneEditCreate(id=u(n),name=name,object=object,part_id=part,parent_id=parent))
def content(edit):
    return v.CommandSceneContent(edit=edit)
def blend(edit):
    return v.CommandBlendShape(edit=edit)
def ref(id):
    return v.SceneReferenceResolved(object_id=id)

with zipzi.connect() as c:
    s=c.sessions.attach(sys.argv[1])
    mesh=sys.argv[2]
    context=s.editor_context()
    assert context is not None and context['version']==s.info()['version']
    assert mesh in context['selected_ids']
    value=mesh_record(s,mesh)
    geometry=v.MeshGeometry(vertex_ids=value.vertex_ids,positions=value.base_positions,uvs=value.uvs,triangles=value.triangles)
    pixels=bytes((45,80,130,255))*4
    with s.draft('Complete scene projection') as e:
        e.apply(v.CommandCreatePart(value=make_part(u(1),'External Part')),
            v.CommandOrganization(edit=v.OrganizationEditCreateGroup(group=make_group(u(2),'External controls'))),
            v.CommandCreateParameter(spec=v.ParameterCreateSpec(id=u(3),name='External Blend',
                minimum=0.,maximum=1.,default_value=0.,kind=v.ParameterKind.BLEND_SHAPE)),
            v.CommandOrganization(edit=v.OrganizationEditSetParameterGroup(parameter_id=u(3),group_id=u(2))),
            v.CommandCreateMesh(spec=v.MeshCreateSpec(name='Second',geometry=geometry,id=u(4))),
            move_to_part((mesh,u(4)),u(1)),
            scene(5,'Warp',v.SceneCreateWarp(bounds=(-10.,-10.,20.,20.)),u(1)),
            scene(6,'Rotation',v.SceneCreateRotation(),u(1),u(5)),
            scene(7,'Glue',v.SceneCreateGlue(a=ref(mesh),b=ref(u(4)))),
            content(v.SceneContentEditInsertGluePair(glue_id=u(7),pair=v.GlueVertexPair(
                id=u(8),weight_a=.5,weight_b=.5,vertex_a=value.vertex_ids[0],vertex_b=value.vertex_ids[1]))),
            scene(9,'Offscreen',v.SceneCreateOffscreen(),u(1)),
            content(v.SceneContentEditInsertOffscreenKeyform(offscreen_id=u(9),value=v.OffscreenKeyform(opacity=.75))),
            content(v.SceneContentEditSetOffscreenMapping(offscreen_id=u(9),updates=(
                v.OffscreenMappingPatch(slot=v.PartKeyformSlotStatic(),keyform=0),))),
            blend(v.BlendAuthoringEditCreateTable(table=v.BlendShapeKeyTable(id=u(12),parameter_id=u(3),
                samples=(v.BindingSample(id=u(10),value=0.),v.BindingSample(id=u(11),value=1.)),base_sample_id=u(10)))),
            blend(v.BlendAuthoringEditCreateBinding(id=u(13),target_id=mesh,target_kind=v.BlendShapeTargetKind.MESH,
                key_table_id=u(12))),
            v.CommandCreateAsset(id=u(14),name='Memory texture'),
            v.CommandAttachment(edit=v.AttachmentEditCreate(id=u(15),path='extras/projection.bin')))
        replace_image(c,e,u(14),pixels,2,2)
        e.apply(v.CommandPutMesh(value=replace(mesh_record(e,mesh),
            name='R4 property name',texture_asset_id=ref(u(14)))))
    deadline=time.monotonic()+5
    while True:
        context=s.editor_context()
        if context is not None and context['version']==s.info()['version']:
            break
        assert time.monotonic()<deadline
        time.sleep(.01)
    assert mesh in context['selected_ids']
    assert read_pixels(s,u(14))==pixels
    assert history(s)==(1,0)
"#,
        &[session.to_string(), mesh.clone()],
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
    // A foreign publication wins against an unpublished human candidate. Both
    // clients observe the new version, and neither stale candidate can publish.
    let ready = endpoint.path().join("gesture-ready");
    let published = endpoint.path().join("gesture-published");
    let retry = endpoint.path().join("gesture-retry");
    let finish = endpoint.path().join("gesture-finished");
    let script = r#"
import time
ready,published,retry,finish=map(Path,sys.argv[3:])
def wait(path):
    deadline=time.monotonic()+15
    while not path.exists():
        assert time.monotonic()<deadline
        time.sleep(.005)
with zipzi.connect() as c:
    s=c.sessions.attach(sys.argv[1])
    mesh=sys.argv[2]
    stale=s.draft('obsolete external candidate')
    stale.apply(rename(mesh,'must never publish'))
    ready.write_text('candidate prepared')
    wait(retry)
    assert mesh_record(s,mesh).appearance.opacity==1.
    with s.draft('external edit during gesture') as winner:
        winner.apply(rename(mesh,'External winner'))
    published.write_text('external publication installed')
    wait(finish)
    try: stale.publish()
    except zipzi.SdkFailure as error: assert error.code=='STALE_SOURCE',error.code
    else: raise AssertionError('obsolete candidate committed')
    value=mesh_record(s,mesh)
    assert value.name=='External winner'
    assert abs(value.appearance.opacity-.61)<1e-6
"#;
    let mut child = Command::new(python)
        .args(["-I", "-c", &format!("{PYTHON_HELPERS}\n{script}")])
        .args([
            session.to_string(),
            mesh.clone(),
            ready.to_string_lossy().into_owned(),
            published.to_string_lossy().into_owned(),
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
    wait_file(&mut h, &published)?;
    ensure!(
        h.view.read_with(&h.cx, |view, _| {
            let document = view.editor.document().unwrap();
            !document.has_pending_edit()
                && document
                    .layers()
                    .iter()
                    .any(|layer| layer.id == mesh && layer.name == "External winner")
        }),
        "foreign publication did not invalidate the stale gesture"
    );
    update(&mut h, |view, _, cx| {
        view.editor
            .execute(EditorCommand::PreviewOpacity(0.61))
            .unwrap();
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
        "R4 native IPC: installed zipzi -> mesh/parameter/binding/scene/group -> typed Draft preview -> candidate/formal CPU sweep -> human preview unchanged -> cursor events/UI Undo -> path reservation -> Metal render passed"
    );
    Ok(())
}

/// Real desktop IPC and native view, shared by R4 and the R6 handoff case.
pub(super) fn setup(
    document: zipzi_editor_core::Document,
) -> anyhow::Result<(Harness, crate::host::DesktopHost, tempfile::TempDir)> {
    let endpoint = tempfile::Builder::new()
        .prefix("zzipc-")
        .tempdir_in("/tmp")?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(endpoint.path(), std::fs::Permissions::from_mode(0o700))?;
    let mut h = Harness::new()?;
    let host =
        h.cx.update(|cx| crate::host::DesktopHost::with_projects(false, Default::default(), cx));
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
    Ok((h, host, endpoint))
}
