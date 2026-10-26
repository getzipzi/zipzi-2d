//! A19: installed Python -> native Editor -> human refinement -> fresh Python.
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
    let (document, _assets) = super::fixture::animation_document()?;
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
    let asset = host
        .runtime
        .read()
        .document(session)?
        .get_mesh(&mesh)
        .unwrap()
        .texture_asset_id
        .clone();
    ensure!(history(&h) == (0, 0), "fixture has unexpected history");
    let mut args = vec![
        session.runtime_instance().to_string(),
        session.serial().to_string(),
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
import sys,zipzi
from dataclasses import replace
from importlib.util import find_spec
from zipzi import events,modeling as m
assert '/site-packages/zipzi/' in zipzi.__file__ and find_spec('zipzi_native') is None
with zipzi.connect() as runtime:
 s=runtime.sessions.attach(zipzi.SessionId(sys.argv[1],int(sys.argv[2])))
 before=s.info(); stream=s.subscribe()
 assert s.editor_context().value is not None
 with runtime.resources.prepare_image(1,1,bytes((240,40,50,255))) as image:
  with s.edit('Agent draft for Editor') as edit:
   edit.meshes.rename(sys.argv[3],'Agent draft')
   edit.resources.replace_image(sys.argv[4],image)
   clip=edit.modeling.records(m.ModelingQuery(motions=(sys.argv[5],))).value.motions[0]
   track=clip.tracks[0]
   end=replace(track.segments[0].end,value=.8)
   track=replace(track,segments=(replace(track.segments[0],end=end),))
   edit.apply(m.ModelingCommandReplaceMotion(clip=replace(clip,name='Agent motion',tracks=(track,))))
   receipt=edit.commit().wait()
   assert receipt.changed and receipt.before==before.version
  assert edit.receipt==receipt
 assert s.info().history==(1,0)
 assert s.meshes.get(sys.argv[3]).value.name=='Agent draft'
 assert s.resources.get(sys.argv[4]).value.pixels.rgba==bytes((240,40,50,255))
 published=[event for event in stream.poll() if isinstance(event,events.Published)]
 assert len(published)==1 and published[0].receipt.after==s.info().version
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
    let refined = host.runtime.read().session_info(session)?;
    args.push(human_value.to_string());
    args.push(refined.version.revision.to_string());
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
import sys,zipzi
from dataclasses import replace
from zipzi import modeling as m
with zipzi.connect() as runtime:
 s=runtime.sessions.attach(zipzi.SessionId(sys.argv[1],int(sys.argv[2])))
 assert s.info().history==(3,0) and s.info().version.revision==int(sys.argv[8])
 assert s.meshes.get(sys.argv[3]).value.name=='Human polish'
 clip=s.modeling.records(m.ModelingQuery(motions=(sys.argv[5],))).value.motions[0]
 assert abs(clip.tracks[0].segments[0].end.value-float(sys.argv[7]))<1e-5
 assert s.resources.get(sys.argv[4]).value.pixels.rgba==bytes((240,40,50,255))
 before=s.info().version
 with s.edit('Agent continues human refinement') as edit:
  edit.meshes.rename(sys.argv[3],'Agent continued',expected_name='Human polish')
  current=edit.modeling.records(m.ModelingQuery(motions=(sys.argv[5],))).value.motions[0]
  track=current.tracks[0]
  track=replace(track,initial=replace(track.initial,value=.2))
  edit.apply(m.ModelingCommandReplaceMotion(clip=replace(current,tracks=(track,))))
  receipt=edit.commit().wait()
  assert receipt.before==before and receipt.changed
 assert s.info().history==(4,0)
 after=s.modeling.records(m.ModelingQuery(motions=(sys.argv[5],))).value.motions[0]
 assert abs(after.tracks[0].initial.value-.2)<1e-5
 assert abs(after.tracks[0].segments[0].end.value-float(sys.argv[7]))<1e-5
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
    let image_usage = host.runtime.read().image_memory_usage();
    let image_entries = host.runtime.read().session(session)?.memory_image_entries();
    client(
        &mut h,
        python,
        endpoint.path(),
        r#"
import sys,zipzi
from zipzi import modeling as m
from zipzi.docs.editor_handoff import rename_mesh
with zipzi.connect() as runtime:
 s=runtime.sessions.attach(zipzi.SessionId(sys.argv[1],int(sys.argv[2])))
 assert s.info().history==(3,1)
 assert s.meshes.get(sys.argv[3]).value.name=='Human polish'
 clip=s.modeling.records(m.ModelingQuery(motions=(sys.argv[5],))).value.motions[0]
 assert clip.tracks[0].initial.value==0.
 assert abs(clip.tracks[0].segments[0].end.value-float(sys.argv[7]))<1e-5
 receipt=rename_mesh(s,sys.argv[3],'Human polish')
 assert not receipt.changed and s.info().history==(3,1)
 before=s.info()
 with runtime.resources.prepare_image(1,1,bytes((0,0,255,255))) as unused:
  edit=s.edit('cancel temporary resources')
  edit.resources.replace_image(sys.argv[4],unused)
  edit.cancel()
 assert s.info()==before
 assert s.resources.get(sys.argv[4]).value.pixels.rgba==bytes((240,40,50,255))
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
import runpy,sys
runtime,serial,mesh=sys.argv[1:4]
sys.argv=['editor_handoff','rename','--runtime',runtime,'--session',serial,'--mesh',mesh,'--name','Recipe continued']
runpy.run_module('zipzi.docs.editor_handoff',run_name='__main__')
"#,
        &args,
    )?;
    ensure!(
        history(&h) == (4, 0),
        "installed handoff example did not continue shared history"
    );
    let after_images = host.runtime.read().image_memory_usage();
    // Binary read replies remain in the bounded protocol recovery cache. Check
    // actual image buffers/registry roots, independently of cached blob replies.
    ensure!(
        after_images.rgba_bytes == image_usage.rgba_bytes
            && after_images.png_bytes == image_usage.png_bytes
            && after_images.preparing_bytes == 0
            && host.runtime.read().session(session)?.memory_image_entries() == image_entries
            && host.runtime.read().prepared_image_count() == 0,
        "cancelled image survived: before={image_usage:?}, after={after_images:?}"
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
    println!(
        "R6 A19: installed Python mixed batch -> resident memory texture/Metal -> human property input/Undo/Redo/Timeline drag -> fresh Python reads and continues -> atomic Undo -> installed recipe/no-op/cancel cleanup -> native animation passed"
    );
    Ok(())
}
