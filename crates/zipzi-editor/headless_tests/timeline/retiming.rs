use super::super::setup_with_document;
use super::super::slide_number_input::{press, release, slide};
use gpui_kit::{AppContext as _, TestAppContext, point, px, test::TestWindowExt};
use zipzi_editor_core::{CommandId, TimelineDisplayMode};

#[gpui_kit::test]
fn timeline_retime_final_release_escape_and_numeric_blur_are_atomic(cx: &mut TestAppContext) {
    let (document, assets) = crate::test_fixture::retiming_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    let duration = "timeline-number-RetimeDuration";
    cx.update_window(h.window, |_, w, c| {
        w.click("workspace-animation", c);
        h.view.update(c, |view, c| {
            let editor = &mut view.editor;
            let document = editor.active_id().unwrap();
            let selection = editor
                .document()
                .unwrap()
                .motion_resources()
                .iter()
                .find(|motion| motion.name == "Retime")
                .unwrap()
                .selection
                .clone();
            editor.select_motion(document, selection).unwrap();
            view.redraw(c);
        });
        w.render_frame(c);
        super::tool(w, "timeline-retime", c);
        assert!(w.find("timeline-retime-controls").visible());
        assert!(w.try_find("timeline-retime-right").is_none());

        h.view.update(c, |view, c| {
            let editor = &mut view.editor;
            let document = editor.active_id().unwrap();
            let projection = editor.timeline_projection().unwrap();
            let clip = editor
                .document()
                .unwrap()
                .motion_definition(&projection.clip_id)
                .unwrap();
            let track = &projection.tracks[&clip.tracks[0].id];
            editor
                .update_timeline_view(document, |state| {
                    state.keys = track.keys[..3]
                        .iter()
                        .map(|key| key.reference.clone())
                        .collect();
                    state.events.insert(clip.events[0].id.clone());
                    state.channels.extend(
                        projection
                            .rows
                            .iter()
                            .filter(|row| row.track_id.as_ref() == Some(&track.id))
                            .map(|row| row.id.clone()),
                    );
                })
                .unwrap();
            view.redraw(c);
        });
        w.render_frame(c);
    })
    .unwrap();
    let original = h.view.read_with(cx, |view, _| {
        let editor = &view.editor;
        editor
            .document()
            .unwrap()
            .motion_definition(&editor.timeline_projection().unwrap().clip_id)
            .unwrap()
    });
    for escaped in [false, true] {
        cx.update_window(h.window, |_, w, c| {
            let origin = w.find("timeline-retime-right").bounds().center();
            let graph = w.find("timeline-graph").bounds();
            let step = graph.size.width / 60.;
            let valid = origin - point(4. * step, px(0.));
            let invalid = origin + point(4. * step, px(0.));
            press(w, origin, c);
            slide(w, valid, false, c);
            let editor = &h.view.read(c).editor;
            assert_eq!(
                editor.timeline_edit().unwrap().candidate.tracks[0].segments[1]
                    .end()
                    .time,
                32. / 30.
            );
            if escaped {
                w.press("escape", c);
            }
            release(w, if escaped { valid } else { invalid }, c);
            w.render_frame(c);
            let editor = &h.view.read(c).editor;
            assert!(editor.timeline_edit().is_none());
            assert_eq!(
                editor
                    .document()
                    .unwrap()
                    .motion_definition(&original.id)
                    .unwrap(),
                original
            );
            assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
            if !escaped {
                assert!(w.find("timeline-retime-error").visible());
            }
        })
        .unwrap();
    }
    cx.update_window(h.window, |_, w, c| {
        super::tool(w, "timeline-view", c);
        assert_eq!(
            h.view.read(c).editor.timeline_view().unwrap().mode,
            TimelineDisplayMode::Curves
        );
        w.click("timeline-retime-fix-end", c);
        w.render_frame(c);
        w.click(duration, c);
        w.render_frame(c);
        w.press("secondary-a", c);
        w.input("8", c);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, w, c| {
        assert!(h.view.read(c).editor.timeline_edit().is_some());
        w.press("secondary-a", c);
        w.input("99999", c);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, w, c| {
        w.click("timeline-graph", c);
        w.render_frame(c);
        let editor = &h.view.read(c).editor;
        assert!(editor.timeline_edit().is_none());
        assert_eq!(
            editor
                .document()
                .unwrap()
                .motion_definition(&original.id)
                .unwrap(),
            original
        );
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    })
    .unwrap();

    cx.update_window(h.window, |_, w, c| {
        h.view.update(c, |view, c| {
            let editor = &mut view.editor;
            let document = editor.active_id().unwrap();
            let p = editor.timeline_projection().unwrap();
            let track = &p.tracks[&original.tracks[0].id];
            editor
                .update_timeline_view(document, |state| {
                    state.keys = track.keys[..3]
                        .iter()
                        .map(|key| key.reference.clone())
                        .collect();
                })
                .unwrap();
            view.redraw(c);
        });
        w.render_frame(c);
        w.click(duration, c);
        w.render_frame(c);
        w.press("secondary-a", c);
        w.input("8", c);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, w, c| {
        // Focusing a transport button completes the valid duration input.
        w.click("timeline-previous-frame", c);
        w.render_frame(c);
        let editor = &h.view.read(c).editor;
        let result = editor
            .document()
            .unwrap()
            .motion_definition(&original.id)
            .unwrap();
        assert_eq!(result.tracks[0].initial.time, 28. / 30.);
        assert_eq!(result.tracks[0].segments[0].end().time, 32. / 30.);
        assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
        h.view
            .update(c, |view, c| view.run_command(CommandId::Undo, w, c));
        assert_eq!(
            h.view
                .read(c)
                .editor
                .document()
                .unwrap()
                .motion_definition(&original.id)
                .unwrap(),
            original
        );
        h.view
            .update(c, |view, c| view.run_command(CommandId::Redo, w, c));
        assert_eq!(
            h.view
                .read(c)
                .editor
                .document()
                .unwrap()
                .motion_definition(&original.id)
                .unwrap(),
            result
        );
    })
    .unwrap();
}
