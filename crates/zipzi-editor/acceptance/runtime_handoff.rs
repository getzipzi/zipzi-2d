//! A19/A20: installed Python -> native Editor -> human refinement -> persistence/export.
use super::harness::{Harness, command, update};
use super::runtime_ipc::{client, setup};
use anyhow::{Context as _, ensure};
use gpui_kit::{point, px, test::TestWindowExt};
use std::path::Path;
use zipzi_editor_core::{
    AnimationPreviewMode, CommandId, EditorCommand, TimelineChannelId, TimelineDisplayMode,
    TimelineRowId,
};

fn history(h: &Harness) -> (usize, usize) {
    h.view
        .read_with(&h.cx, |v, _| v.editor.document().unwrap().history_lengths())
}

pub(super) fn run() -> anyhow::Result<()> {
    let python = std::env::var_os("ZIPZI_CLIENT_PYTHON")
        .context("ZIPZI_CLIENT_PYTHON must point to an isolated installed client")?;
    let python = Path::new(&python);
    ensure!(python.is_absolute(), "client interpreter must be absolute");
    // The shared animation fixture deliberately references missing test.wav to
    // test unsupported-audio diagnostics. Use real input for the export path.
    let mut wav = b"RIFF".to_vec();
    wav.extend_from_slice(&1636_u32.to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt \x10\0\0\0\x01\0\x01\0");
    wav.extend_from_slice(&8000_u32.to_le_bytes());
    wav.extend_from_slice(&16000_u32.to_le_bytes());
    wav.extend_from_slice(b"\x02\0\x10\0data");
    wav.extend_from_slice(&1600_u32.to_le_bytes());
    wav.resize(1644, 0);
    let (document, _assets) = super::fixture::animation_document_with_sound_bytes(Some(&wav))?;
    let (mut h, host, endpoint) = setup(document)?;
    let (session, mesh, clip, parameter) = h.view.read_with(&h.cx, |v, _| {
        let d = v.editor.document().unwrap();
        let mesh = &d.layers()[0].id;
        (
            d.runtime_session_id(),
            mesh.clone(),
            d.motion_resources()[0].selection.clip_id.clone(),
            d.parameters()[0].id.clone(),
        )
    });
    let source = crate::test_fixture::published_snapshot(&host.runtime, session)?;
    let asset = source
        .document()
        .mesh(&mesh)
        .unwrap()
        .texture_asset_id
        .clone();
    ensure!(history(&h) == (0, 0), "fixture has unexpected history");
    let mut args = vec![
        session.to_string(),
        mesh.clone(),
        asset.to_string(),
        clip.clone(),
        parameter.clone(),
    ];
    update(&mut h, |v, _, cx| {
        v.editor
            .execute(EditorCommand::Select(mesh.clone()))
            .unwrap();
        v.redraw(cx);
    })?;
    let before = h.capture()?;
    let before_canvas = h.canvas_image(&before)?;
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
from dataclasses import replace
from importlib.util import find_spec
assert '/site-packages/zipzi/' in zipzi.__file__ and find_spec('zipzi_native') is None
with zipzi.connect() as runtime:
    s=runtime.sessions.attach(sys.argv[1])
    mesh, asset, motion=sys.argv[2:5]
    cursor=s.version
    assert s.editor_context() is not None
    with s.draft('Agent draft for Editor') as e:
        e.apply(rename(mesh,'Agent draft'))
        replace_image(runtime,e,asset,bytes((240,40,50,255)))
        clip=v.MotionClip.from_wire(record(e,v.ObjectKind.MOTION,motion))
        track=clip.tracks[0]
        segment=replace(track.segments[0],end=replace(track.segments[0].end,value=.8))
        e.apply(v.CommandPutMotion(value=replace(clip,name='Agent motion',tracks=(replace(track,segments=(segment,)),))))
        receipt=e.publish()
        assert receipt['kind']=='commit' and receipt['version']['revision']==cursor.revision+1
    assert history(s)==(1,0)
    assert mesh_record(s,mesh).name=='Agent draft'
    assert read_pixels(s,asset)==bytes((240,40,50,255))
    events=s.events(cursor)
    assert len(events)==1 and events[0]['version']==s.info()['version']
"#,
        &args,
    )?;
    // The process has exited. Formal content, memory pixels and history stay resident.
    ensure!(
        history(&h) == (1, 0),
        "agent batch was not one history step"
    );
    h.view.read_with(&h.cx, |v, _| -> anyhow::Result<()> {
        let d = v
            .editor
            .document()
            .context("view stayed busy after flush")?;
        ensure!(
            d.layers()[0].name == "Agent draft",
            "mesh projection did not refresh"
        );
        ensure!(
            d.motion_definition(&clip).unwrap().name == "Agent motion",
            "motion source did not refresh"
        );
        ensure!(
            d.motion_resources()
                .iter()
                .any(|m| m.name == "Agent motion"),
            "motion catalog did not refresh"
        );
        Ok(())
    })?;
    let drafted = h.capture()?;
    ensure!(
        h.canvas_image(&drafted)?.as_raw() != before_canvas.as_raw(),
        "memory texture replacement did not change the native Metal canvas"
    );

    // Submit a human property edit using the production text field and Enter.
    h.cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.click("property-input-name", cx);
        window.press("cmd-a", cx);
        window.input("Human polish", cx);
        window.press("enter", cx);
        window.blur(cx);
        window.render_frame(cx);
    })?;
    ensure!(
        history(&h) == (2, 0),
        "human property input did not commit once"
    );
    command(&mut h, CommandId::Undo)?;
    ensure!(
        history(&h) == (1, 1),
        "human Undo did not share the SDK history"
    );
    command(&mut h, CommandId::Redo)?;
    ensure!(
        history(&h) == (2, 0),
        "human Redo did not restore refinement"
    );

    // Edit an existing key through the actual Timeline number drag.
    h.cx.update_window(h.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
    })?;
    update(&mut h, |v, _, cx| -> anyhow::Result<()> {
        let id = v.editor.active_id().unwrap();
        v.editor
            .set_animation_preview_mode(id, AnimationPreviewMode::Editing)?;
        v.editor.seek_timeline(id, 2.)?;
        v.editor.update_timeline_view(id, |state| {
            state.scroll = 0.;
            state.search.clear();
            state.mode = TimelineDisplayMode::Overview;
            state.snap = false;
        })?;
        v.redraw(cx);
        Ok(())
    })??;
    super::timeline::drain(&mut h)?;
    let row = TimelineRowId::Channel(TimelineChannelId::Parameter(parameter.clone()));
    let field = format!("timeline-number-RowValue({row:?})");
    let origin = h.cx.update_window(h.window, |_, window, cx| {
        window.blur(cx);
        window.render_frame(cx);
        window.within("timeline-rows").find(field).bounds().center()
    })?;
    super::timeline::pointer(&mut h, origin, 0)?;
    super::timeline::pointer(&mut h, origin + point(px(-32.), px(0.)), 1)?;
    super::timeline::pointer(&mut h, origin + point(px(-32.), px(0.)), 2)?;
    super::timeline::drain(&mut h)?;
    ensure!(
        history(&h) == (3, 0),
        "human Timeline drag did not commit once"
    );
    let human_value = h.view.read_with(&h.cx, |v, _| {
        v.editor
            .document()
            .unwrap()
            .motion_definition(&clip)
            .unwrap()
            .tracks[0]
            .segments[0]
            .end()
            .value
    });
    ensure!(
        (human_value - 0.8).abs() > 1e-4,
        "Timeline drag did not change the authored key"
    );
    let refined = host
        .runtime
        .read()
        .session_read(host.runtime.client_id(), session)?
        .version();
    args.push(human_value.to_string());
    args.push(refined.revision.to_string());
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
from dataclasses import replace
with zipzi.connect() as runtime:
    s=runtime.sessions.attach(sys.argv[1])
    mesh, asset, motion=sys.argv[2:5]
    assert history(s)==(3,0) and s.version.revision==int(sys.argv[7])
    assert mesh_record(s,mesh).name=='Human polish'
    clip=v.MotionClip.from_wire(record(s,v.ObjectKind.MOTION,motion))
    assert abs(clip.tracks[0].segments[0].end.value-float(sys.argv[6]))<1e-5
    assert read_pixels(s,asset)==bytes((240,40,50,255))
    before=s.version
    with s.draft('Agent continues human refinement') as e:
        assert mesh_record(e,mesh).name=='Human polish'
        track=clip.tracks[0]
        track=replace(track,initial=replace(track.initial,value=.2))
        e.apply(rename(mesh,'Agent continued'),v.CommandPutMotion(value=replace(clip,tracks=(track,))))
    assert s.version.revision==before.revision+1 and history(s)==(4,0)
    after=v.MotionClip.from_wire(record(s,v.ObjectKind.MOTION,motion))
    assert abs(after.tracks[0].initial.value-.2)<1e-5
    assert abs(after.tracks[0].segments[0].end.value-float(sys.argv[6]))<1e-5
"#,
        &args,
    )?;
    ensure!(
        history(&h) == (4, 0),
        "agent continuation duplicated/missed publication"
    );
    // One Undo rolls back the whole second batch, preserving both human edits.
    command(&mut h, CommandId::Undo)?;
    ensure!(history(&h) == (3, 1), "continuation Undo was not atomic");
    let image_usage = host.runtime.read().transfer_budget().usage();
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
with zipzi.connect() as runtime:
    s=runtime.sessions.attach(sys.argv[1])
    mesh, asset, motion=sys.argv[2:5]
    assert history(s)==(3,1)
    assert mesh_record(s,mesh).name=='Human polish'
    clip=v.MotionClip.from_wire(record(s,v.ObjectKind.MOTION,motion))
    assert clip.tracks[0].initial.value==0.
    assert abs(clip.tracks[0].segments[0].end.value-float(sys.argv[6]))<1e-5
    before=s.info()
    with s.draft('Same mesh name') as e:
        e.apply(rename(mesh,'Human polish'))
        assert e.publish()['kind']=='unchanged'
    e=s.draft('cancel temporary resources')
    try:
        replace_image(runtime,e,asset,bytes((0,0,255,255)))
    finally:
        e.cancel()
    assert s.info()==before
    assert read_pixels(s,asset)==bytes((240,40,50,255))
"#,
        &args,
    )?;
    ensure!(
        history(&h) == (3, 1),
        "no-op/cancel changed human history or redo"
    );
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
with zipzi.connect() as runtime:
    s=runtime.sessions.attach(sys.argv[1])
    with s.draft('Continue the Editor project') as e:
        e.apply(rename(sys.argv[2],'Recipe continued'))
"#,
        &args,
    )?;
    ensure!(
        history(&h) == (4, 0),
        "second client did not continue shared history"
    );
    let after_images = host.runtime.read().transfer_budget().usage();
    ensure!(
        after_images.rgba_bytes == image_usage.rgba_bytes
            && after_images.png_bytes == image_usage.png_bytes
            && after_images.preparing_bytes == 0,
        "cancelled image survived"
    );
    update(&mut h, |v, _, cx| {
        let document = v.editor.active_id().unwrap();
        v.editor.seek_timeline(document, 1.).unwrap();
        v.editor.toggle_animation().unwrap();
        v.editor.advance_animation_clock(1.25);
        v.editor.pause_animation();
        v.redraw(cx);
    })?;
    let final_frame = h.capture()?;
    ensure!(
        !h.canvas_image(&final_frame)?.as_raw().is_empty(),
        "final animation Metal frame missing"
    );
    h.view.read_with(&h.cx, |v, _| -> anyhow::Result<()> {
        let d = v.editor.document().unwrap();
        ensure!(
            d.layers()
                .iter()
                .any(|layer| layer.id == mesh && layer.name == "Recipe continued"),
            "final UI source is stale"
        );
        ensure!(
            d.runtime_session_id() == session,
            "handoff changed the target Session"
        );
        ensure!(
            d.motion_definition(&clip).unwrap().tracks[0].segments[0]
                .end()
                .value
                == human_value,
            "continuation lost the human key value"
        );
        ensure!(
            v.editor.animation().unwrap().presented.is_some(),
            "animation was not presented"
        );
        Ok(())
    })?;
    // Finish the same human/agent document through the public installed IO API.
    let persisted = tempfile::tempdir()?;
    args.push(persisted.path().to_string_lossy().into_owned());
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
import wave
with zipzi.connect() as runtime:
    s=runtime.sessions.attach(sys.argv[1])
    mesh, asset, motion=sys.argv[2:5]
    folder=Path(sys.argv[8])
    before=record(s,v.ObjectKind.MOTION,motion)
    previous=s.version
    with s.save(folder/'project.zipzi') as job:
        saved=job.wait()
    assert saved['durable'] and not s.info()['dirty']
    with s.open(Path(saved['path'])) as job:
        reopened=job.wait()
    assert reopened['receipt']['version']['generation']>previous.generation and s.id==sys.argv[1]
    assert mesh_record(s,mesh).name=='Recipe continued'
    assert read_pixels(s,asset)==bytes((240,40,50,255))
    after=record(s,v.ObjectKind.MOTION,motion)
    assert after==before and history(s)==(0,0)
    with s.export(folder/'package',v.ExportFormat.RUNTIME,allow_loss=True) as job:
        assert job.wait()['receipt']['published']
    package=folder/'package'
    model=json.loads((package/'model.model3.json').read_text())
    registrations=[row for rows in model['FileReferences']['Motions'].values() for row in rows]
    assert any(abs(json.loads((package/row['File']).read_text())['Curves'][0]['Segments'][-1]
        -float(sys.argv[6]))<1e-5 for row in registrations)
    with wave.open(str(package/'test.wav'),'rb') as sound:
        assert sound.getframerate()==8000 and sound.readframes(800)==bytes(1600)
"#,
        &args,
    )?;
    h.capture()?;
    ensure!(
        h.view
            .read_with(&h.cx, |v, _| v.editor.document().is_some_and(|d| {
                d.runtime_session_id() == session
                    && d.layers()
                        .iter()
                        .any(|layer| layer.id == mesh && layer.name == "Recipe continued")
                    && !d.modified()
            })),
        "saved/reopened human refinement did not return to the Editor"
    );
    println!(
        "R6/R7 A19/A20: installed Python mixed batch -> resident memory texture/Metal -> human property input/Undo/Redo/Timeline drag -> fresh Python continuation -> atomic Undo/no-op/cancel cleanup -> native animation -> durable save/reopen -> motion and audio in runtime-package export passed"
    );
    Ok(())
}
