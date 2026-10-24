use super::super::slide_number_input::{press, release, slide};
use super::super::{document_fixture, setup_with_document};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, point, px};
use zipzi_editor_core::{CommandId, TimelineKeyRef};

#[gpui_kit::test]
fn empty_motion_inline_value_creates_one_key_at_the_head_and_can_cancel(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click("animation-new-motion", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let origin = cx
        .update_window(harness.window, |_, window, cx| {
            let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
            assert!(projection.tracks.is_empty());
            let row = projection
                .rows
                .iter()
                .find(|row| {
                    matches!(
                        row.channel,
                        Some(zipzi_editor_core::TimelineChannelId::Parameter(_))
                    )
                })
                .unwrap();
            let field = format!("timeline-number-RowValue({:?})", row.id);
            harness.view.update(cx, |view, cx| {
                let id = view.editor.active_id().unwrap();
                view.editor.seek_timeline(id, 0.5).unwrap();
                view.redraw(cx);
            });
            window.render_frame(cx);
            let origin = window.find(field).bounds().center();
            press(window, origin, cx);
            slide(window, origin + point(px(80.), px(0.)), false, cx);
            origin
        })
        .unwrap();
    harness.view.read_with(cx, |view, _| {
        let candidate = view.editor.timeline_edit().unwrap().candidate.clip();
        assert_eq!(candidate.tracks.len(), 1);
        assert_eq!(candidate.tracks[0].initial.time, 0.5);
        assert!(candidate.tracks[0].segments.is_empty());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_projection()
                .unwrap()
                .tracks
                .is_empty()
        );
        slide(window, origin + point(px(120.), px(0.)), false, cx);
        release(window, origin, cx);
        assert!(harness.view.read(cx).editor.timeline_edit().is_none());
        press(window, origin, cx);
        slide(window, origin + point(px(80.), px(0.)), false, cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        release(window, origin, cx);
        window.render_frame(cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
        assert_eq!(view.editor.timeline_projection().unwrap().tracks.len(), 1);
    });
}

#[gpui_kit::test]
fn keyboard_copy_paste_delete_is_scoped_and_number_text_has_priority(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    let (key, _pasted) = cx
        .update_window(harness.window, |_, window, cx| {
            window.click("workspace-animation", cx);
            window.render_frame(cx);
            window.click(("animation-motion", 0usize), cx);
            window.render_frame(cx);
            let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
            let key = projection.tracks.values().next().unwrap().keys[0]
                .reference
                .clone();
            let origin = harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .timeline
                .read(cx)
                .key_position(&key)
                .unwrap()
                + point(px(2.), px(0.));
            press(window, origin, cx);
            release(window, origin, cx);
            window.render_frame(cx);
            crate::keybindings::reset(cx).unwrap();
            window.press("cmd-c", cx);
            for _ in 0..30 {
                window.press("right", cx);
            }
            window.press("cmd-v", cx);
            window.render_frame(cx);
            let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
            let track = &projection.tracks[&key.track_id];
            assert_eq!(track.keys.len(), 3);
            assert!((track.keys[1].time - 0.5).abs() < 1e-5);
            let pasted: TimelineKeyRef = track.keys[1].reference.clone();
            (key, pasted)
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        let field = "property-input-selection_value";
        window.click(field, cx);
        window.press("cmd-a", cx);
        window.input("0.25", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        assert!(harness.view.read(cx).editor.timeline_edit().is_some());
        window.press("backspace", cx); // text, not a key deletion
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_projection()
                .unwrap()
                .tracks[&key.track_id]
                .keys
                .len(),
            3
        );
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(harness.view.read(cx).editor.timeline_edit().is_none());
        window.press("backspace", cx);
        window.render_frame(cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_projection()
                .unwrap()
                .tracks[&key.track_id]
                .keys
                .len(),
            2
        );
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (2, 0)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn marquee_batch_drag_preserves_spacing_and_has_one_undo_entry(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        harness.view.update(cx, |view, cx| {
            let projection = view.editor.timeline_projection().unwrap();
            let channel = projection.tracks.values().next().unwrap().channel.clone();
            let d = view.editor.active_id().unwrap();
            for time in [0.5, 0.75] {
                view.editor.seek_timeline(d, time).unwrap();
                view.editor
                    .execute_timeline(
                        d,
                        zipzi_editor_core::TimelineCommand::InsertKey {
                            channel: channel.clone(),
                            track_id: None,
                            selected_key: None,
                        },
                    )
                    .unwrap();
            }
            view.redraw(cx);
        });
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        let track = projection.tracks.values().next().unwrap();
        let a = track.keys[1].reference.clone();
        let b = track.keys[2].reference.clone();
        let start = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .key_position(&a)
            .unwrap()
            - point(px(12.), px(9.));
        let end = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .key_position(&b)
            .unwrap()
            + point(px(12.), px(9.));
        press(window, start, cx);
        slide(window, end, false, cx);
        release(window, end, cx);
        window.render_frame(cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_view()
                .unwrap()
                .keys
                .len(),
            2
        );
        let origin = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .key_position(&a)
            .unwrap();
        let end = origin
            + point(
                window.find("timeline-graph").bounds().size.width / 8.,
                px(0.),
            );
        press(window, origin, cx);
        slide(window, end, false, cx);
        release(window, end, cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        assert_eq!(projection.key(&a).unwrap().time, 0.75);
        assert_eq!(projection.key(&b).unwrap().time, 1.);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (3, 0)
        );
        harness
            .view
            .update(cx, |view, cx| view.run_command(CommandId::Undo, window, cx));
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        assert_eq!(projection.key(&a).unwrap().time, 0.5);
        assert_eq!(projection.key(&b).unwrap().time, 0.75);
    })
    .unwrap();
}

#[gpui_kit::test]
fn modal_cancels_a_candidate_and_late_pointer_events_do_not_restart_it(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        let key = projection.tracks.values().next().unwrap().keys[0]
            .reference
            .clone();
        let origin = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .key_position(&key)
            .unwrap()
            + point(px(2.), px(0.));
        let end = origin
            + point(
                window.find("timeline-graph").bounds().size.width / 4.,
                px(0.),
            );
        press(window, origin, cx);
        slide(window, end, false, cx);
        assert!(harness.view.read(cx).editor.timeline_edit().is_some());
        harness.view.update(cx, |view, cx| {
            view.modal = true;
            view.redraw(cx);
        });
        window.render_frame(cx);
        assert!(harness.view.read(cx).editor.timeline_edit().is_none());
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_projection()
                .unwrap()
                .key(&key)
                .unwrap()
                .time,
            0.
        );
        harness.view.update(cx, |view, cx| {
            view.modal = false;
            view.redraw(cx);
        });
        window.render_frame(cx);
        slide(window, end, false, cx);
        release(window, end, cx);
        assert!(harness.view.read(cx).editor.timeline_edit().is_none());
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 0)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn rebound_timeline_commands_replace_defaults_and_ignore_repeated_writes(cx: &mut TestAppContext) {
    use gpui_kit::InputEvent as _;
    use zipzi_editor_core::{ShortcutOverride, ShortcutOverrides};
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        let key = projection.tracks.values().next().unwrap().keys[0]
            .reference
            .clone();
        let position = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .key_position(&key)
            .unwrap()
            + point(px(2.), px(0.));
        press(window, position, cx);
        release(window, position, cx);
        window.render_frame(cx);
        let store = cx.global::<crate::settings::Settings>().store.clone();
        store
            .write(
                "keybindings.json",
                &ShortcutOverrides {
                    bindings: [
                        (CommandId::TimelineCopy, "v"),
                        (CommandId::TimelinePaste, "p"),
                        (CommandId::TimelineNextFrame, "l"),
                        (CommandId::TimelineDelete, "shift-backspace"),
                    ]
                    .into_iter()
                    .map(|(command, key)| ShortcutOverride {
                        command,
                        keys: vec![key.into()],
                        platform: None,
                    })
                    .collect(),
                },
            )
            .unwrap();
        crate::keybindings::reload(cx);
        assert!(cx.global::<crate::settings::Settings>().notices.is_empty());
        let before = harness.view.read(cx).editor.animation().unwrap().edit_time;
        window.press("cmd-c", cx);
        assert!(
            !harness.view.read(cx).editor.command_available(
                harness
                    .view
                    .read(cx)
                    .editor
                    .command(CommandId::TimelinePaste)
            )
        );
        window.press("right", cx);
        assert_eq!(
            harness.view.read(cx).editor.animation().unwrap().edit_time,
            before
        );
        window.press("v", cx);
        for _ in 0..30 {
            window.press("l", cx);
        }
        window.press("p", cx);
        window.render_frame(cx);
        let keys = |cx: &gpui_kit::App| {
            harness
                .view
                .read(cx)
                .editor
                .timeline_projection()
                .unwrap()
                .tracks[&key.track_id]
                .keys
                .len()
        };
        assert_eq!(keys(cx), 3);
        let history = harness
            .view
            .read(cx)
            .editor
            .document()
            .unwrap()
            .history_lengths();
        window.dispatch_event(
            gpui_kit::KeyDownEvent {
                keystroke: gpui_kit::Keystroke::parse("p").unwrap(),
                is_held: true,
                prefer_character_input: false,
            }
            .to_platform_input(),
            cx,
        );
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            history
        );
        assert_eq!(keys(cx), 3);
        window.press("backspace", cx);
        assert_eq!(keys(cx), 3);
        window.press("shift-backspace", cx);
        assert_eq!(keys(cx), 2);
    })
    .unwrap();
}
