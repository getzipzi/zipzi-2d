//! Synthetic production UI scenario, independent of optional licensed assets.
use super::super::harness::{Harness, command, update};
use super::common::{clip, drain, pointer};
use anyhow::{Context as _, ensure};
use gpui_kit::{point, px, test::TestWindowExt};
use zipzi_core::document::{MotionCurve, MotionFrameStep};
use zipzi_editor_core::{AnimationPreviewMode, CommandId, Document, Editor, TimelineRowId};

pub(super) fn run() -> anyhow::Result<()> {
    let (document, _assets) = crate::test_fixture::retiming_document()?;
    let mut h = Harness::with_size(1320., 800.)?;
    update(&mut h, |view, window, cx| {
        view.replace_editor(Editor::from_document(document), cx);
        view.animation_clock_running = true;
        view.refresh_document(window, cx);
    })?;
    command(&mut h, CommandId::AnimationWorkspace)?;
    command(&mut h, CommandId::FitView)?;
    update(&mut h, |view, _, cx| -> anyhow::Result<()> {
        let editor = &mut view.editor;
        let document = editor.active_id().context("synthetic document missing")?;
        let selection = editor
            .document()
            .unwrap()
            .motion_resources()
            .iter()
            .find(|resource| resource.name == "Retime")
            .context("synthetic motion missing")?
            .selection
            .clone();
        editor.select_motion(document, selection)?;
        editor.set_animation_preview_mode(document, AnimationPreviewMode::Result)?;
        let expression = editor.document().unwrap().expression_resources()[0]
            .id
            .clone();
        editor.select_expression(document, expression)?;
        view.redraw(cx);
        Ok(())
    })??;
    drain(&mut h)?;
    let original = clip(&h);
    let strip = h.cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        window.find("timeline-loop-strip").bounds()
    })?;
    let strip_at = |frame: f32| {
        point(
            strip.left() + strip.size.width * (frame / 60.),
            strip.center().y,
        )
    };
    pointer(&mut h, strip_at(41.), 0)?;
    pointer(&mut h, strip_at(20.), 1)?;
    pointer(&mut h, strip_at(20.), 2)?;
    let range = h
        .view
        .read_with(&h.cx, |v, _| v.editor.timeline_view().unwrap().loop_range())
        .context("range drag did not set endpoints")?;
    ensure!(range.enabled && range.start == 20. / 30. && range.end == 41. / 30.);

    update(&mut h, |view, _, cx| -> anyhow::Result<()> {
        let editor = &mut view.editor;
        let p = editor.timeline_projection().unwrap();
        let ids = [&original.tracks[0].id, &original.tracks[1].id];
        editor.update_timeline_view(editor.active_id().unwrap(), |state| {
            state.keys = ids
                .iter()
                .flat_map(|id| &p.tracks[*id].keys[..3])
                .map(|key| key.reference.clone())
                .collect();
            state.events = original
                .events
                .iter()
                .map(|event| event.id.clone())
                .collect();
            state.channels = p
                .rows
                .iter()
                .filter(|row| {
                    row.id == TimelineRowId::Events
                        || row.track_id.as_ref().is_some_and(|id| ids.contains(&id))
                })
                .map(|row| row.id.clone())
                .collect();
        })?;
        view.redraw(cx);
        Ok(())
    })??;
    let (origin, width) = h.cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("timeline-retime", cx);
        window.render_frame(cx);
        (
            window.find("timeline-retime-right").bounds().center(),
            window.find("timeline-graph").bounds().size.width,
        )
    })?;
    let dest = origin - point(width * (4. / 60.), px(0.));
    pointer(&mut h, origin, 0)?;
    pointer(&mut h, dest, 1)?;
    drain(&mut h)?;
    ensure!(
        h.view
            .read_with(&h.cx, |v, _| v.editor.timeline_edit().is_some())
    );
    pointer(&mut h, dest, 2)?;
    drain(&mut h)?;
    let retimed = clip(&h);
    for track in &retimed.tracks[..2] {
        ensure!(
            track.keys().map(|key| key.time).collect::<Vec<_>>()
                == [24. / 30., 28. / 30., 32. / 30., 40. / 30.]
        );
    }
    ensure!(retimed.tracks[2] == original.tracks[2]);
    ensure!(retimed.events[0].id == original.events[0].id);
    ensure!(retimed.events[0].time == 1. && retimed.events[0].value == "cue");
    let MotionCurve::Bezier {
        control1, control2, ..
    } = retimed.tracks[0].segments[0].curve
    else {
        anyhow::bail!("retimed manual curve lost its interpolation");
    };
    ensure!(control1.value == 0.1 && control2.value == 0.4);
    ensure!((control1.time - (24. + 4. / 3.) / 30.).abs() < 1e-6);
    ensure!(retimed.tracks[1].segments[0].frame_step == MotionFrameStep::EveryFrames { frames: 3 });
    ensure!(
        h.view
            .read_with(&h.cx, |v, _| v.editor.document().unwrap().history_lengths())
            == (1, 0)
    );
    command(&mut h, CommandId::Undo)?;
    ensure!(clip(&h) == original);
    command(&mut h, CommandId::Redo)?;
    ensure!(clip(&h) == retimed);

    h.cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("animation-play-pause", cx);
    })?;
    drain(&mut h)?;
    let start_pixels = h.capture()?;
    update(&mut h, |view, _, cx| {
        view.editor.advance_animation_clock_precise(0.4);
        view.redraw(cx);
    })?;
    drain(&mut h)?;
    ensure!(
        h.view.read_with(&h.cx, |v, _| v
            .editor
            .animation()
            .unwrap()
            .range_events()
            .count())
            == 1
    );
    let cue_pixels = h.capture()?;
    ensure!(
        cue_pixels != start_pixels,
        "retimed playback did not update the Metal canvas"
    );
    update(&mut h, |view, _, cx| {
        view.editor.advance_animation_clock_precise(1.0);
        view.redraw(cx);
    })?;
    drain(&mut h)?;
    let (position, wrapped) = h.view.read_with(&h.cx, |v, _| {
        let frame = v.animation_frame().unwrap();
        (
            frame.state.identity.range_progress.unwrap(),
            frame.state.snapshot.clone(),
        )
    });
    ensure!(position.cycle == 1 && position.local < 1.);
    h.cx.update_window(h.window, |_, window, cx| {
        window.click("animation-play-pause", cx)
    })?;
    update(&mut h, |view, _, cx| -> anyhow::Result<()> {
        view.editor
            .seek_timeline(view.editor.active_id().unwrap(), position.local)?;
        view.redraw(cx);
        Ok(())
    })??;
    drain(&mut h)?;
    h.view.read_with(&h.cx, |v, _| -> anyhow::Result<()> {
        let a = v.editor.animation().unwrap();
        let direct = a.presented.as_ref().unwrap();
        ensure!(wrapped.parameters == direct.parameters && wrapped.physics == direct.physics);
        ensure!(a.range_events().count() == 0);
        ensure!(v.editor.timeline_view().unwrap().loop_range() == Some(range));
        Ok(())
    })?;

    let temp = tempfile::tempdir()?;
    let path = temp.path().join("retimed.zipzi");
    let save = update(&mut h, |v, _, _| {
        v.editor
            .prepare_save(v.editor.active_id().unwrap(), &path, false)
    })??;
    let mut completion = save.run();
    update(&mut h, |v, _, _| v.editor.finish_save(&mut completion))??;
    let reopened = Document::open(&path)?;
    ensure!(reopened.motion_definition(&retimed.id).as_deref() == Some(&retimed));
    let mut fresh = Editor::from_document(reopened);
    fresh.set_workspace(zipzi_editor_core::WorkspaceKind::Animation)?;
    let selection = fresh
        .document()
        .unwrap()
        .motion_resources()
        .iter()
        .find(|resource| resource.name == "Retime")
        .unwrap()
        .selection
        .clone();
    fresh.select_motion(fresh.active_id().unwrap(), selection)?;
    ensure!(fresh.timeline_view().unwrap().loop_range().is_none());
    println!(
        "PASS Timeline synthetic range and retiming acceptance (Metal playback, history, save/reopen)"
    );
    Ok(())
}
