use super::super::slide_number_input::{press, release, slide};
use super::super::{document_fixture, setup_with_document};
use editor_core::TimelineRowId;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, point, px};

#[gpui_kit::test]
fn event_editing_resize_and_view_controls_preserve_history_boundaries(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        let before = window.find("animation-timeline").bounds().size.height;
        let area = harness.view.read(cx).dock.as_ref().unwrap().area.clone();
        area.update(cx, |area, cx| {
            use gpui_kit::component::dock::DockPlacement;
            let size = area.dock_size(DockPlacement::Bottom).unwrap();
            area.set_dock_size(DockPlacement::Bottom, size + px(45.), window, cx);
        });
        window.render_frame(cx);
        assert!(window.find("animation-timeline").bounds().size.height > before);
        window.click("timeline-seconds", cx);
        window.click("timeline-fit", cx);
        window.click("timeline-snap", cx);
        window.render_frame(cx);
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
        window.click("timeline-add-event", cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        assert_eq!(projection.events.len(), 1);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_view()
                .unwrap()
                .channels
                .first(),
            Some(&TimelineRowId::Events)
        );
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (1, 0)
        );
        window.click("timeline-delete", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, _window, cx| {
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_projection()
                .unwrap()
                .events
                .is_empty()
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
fn event_text_enter_commits_once_and_same_time_events_remain_distinct(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        window.click("timeline-add-event", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.click("timeline-event-value", cx);
        window.input("hello", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor.timeline_projection().unwrap().events[0].value,
            "hello"
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.timeline_edit().is_none());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.scroll(
            "timeline-inspector",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-10000.))),
            cx,
        );
        window.click("timeline-add-event", cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        assert_eq!(projection.events.len(), 2);
        assert_eq!(projection.events[0].time, projection.events[1].time);
        assert_ne!(projection.events[0].id, projection.events[1].id);
        window.click("timeline-event-list", cx);
        window.render_frame(cx);
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
    })
    .unwrap();
}

#[gpui_kit::test]
fn explicit_time_scale_previews_and_enter_commits_all_times_together(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        window.click("timeline-properties", cx);
        window.render_frame(cx);
        window.scroll(
            "timeline-inspector",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-10000.))),
            cx,
        );
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.click("timeline-number-Scale", cx);
        window.press("cmd-a", cx);
        window.input("2", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let clip = view.editor.timeline_edit().unwrap().candidate.clip();
        assert_eq!(clip.duration, 4.);
        assert_eq!(clip.tracks[0].segments[0].end().time, 4.);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.timeline_edit().is_none());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
        assert_eq!(view.editor.timeline_projection().unwrap().duration, 4.);
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.scroll(
            "timeline-inspector",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(10000.))),
            cx,
        );
        window.scroll(
            "timeline-inspector",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-90.))),
            cx,
        );
        window.click("timeline-fade-default-FadeIn", cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .motion_definition(&projection.clip_id)
                .unwrap()
                .fade_in,
            None
        );
        window.click("timeline-fade-custom-FadeIn", cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        assert_eq!(projection.duration, 4.);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .motion_definition(&projection.clip_id)
                .unwrap()
                .fade_in,
            Some(0.)
        );
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
    })
    .unwrap();
}

#[gpui_kit::test]
fn committing_on_blur_keeps_the_next_field_focused_and_uses_the_new_asset(cx: &mut TestAppContext) {
    use gpui_kit::component::{WindowExt, input::AnyInputState};
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    let key = cx
        .update_window(harness.window, |_, window, cx| {
            window.activate_window();
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
            key
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("timeline-number-KeyValue({key:?})"), cx);
        window.press("cmd-a", cx);
        window.input("0.25", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("timeline-number-KeyTime({key:?})"), cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.timeline_edit().is_none());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.press("cmd-a", cx);
        window.input("0.5", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let projection = view.editor.timeline_projection().unwrap();
        assert_eq!(projection.key(&key).unwrap().time, 0.5);
        assert_eq!(projection.key(&key).unwrap().value, 0.25);
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.timeline_edit().is_none());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.click(format!("timeline-number-KeyValue({key:?})"), cx);
        let Some(AnyInputState::Input(input)) = window.focused_input(cx) else {
            panic!("key value input not focused")
        };
        assert_eq!(input.read(cx).value().as_ref(), "0.2");
        window.blur(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let origin = window
            .find(format!("timeline-number-KeyValue({key:?})"))
            .bounds()
            .center();
        // The scrub must start at the precise 0.25, rather than the displayed 0.2.
        press(window, origin, cx);
        slide(window, origin + point(px(20.), px(0.)), false, cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.timeline_edit().is_some());
        let projection = view.editor.timeline_projection().unwrap();
        let actual = projection.key(&key).unwrap().value;
        assert!((actual - 0.3).abs() < 0.00001, "scrub value = {actual}");
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.press("escape", cx);
        window.render_frame(cx);
        let view = harness.view.read(cx);
        assert_eq!(
            view.editor
                .timeline_projection()
                .unwrap()
                .key(&key)
                .unwrap()
                .value,
            0.25
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
    })
    .unwrap();
}

#[gpui_kit::test]
fn invalid_inspector_text_does_not_commit_the_last_valid_intermediate_value(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    let (key, original) = cx
        .update_window(harness.window, |_, window, cx| {
            window.activate_window();
            window.click("workspace-animation", cx);
            window.render_frame(cx);
            window.click(("animation-motion", 0usize), cx);
            window.render_frame(cx);
            let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
            let key_data = &projection.tracks.values().next().unwrap().keys[0];
            let key = key_data.reference.clone();
            let original = key_data.value;
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
            (key, original)
        })
        .unwrap();
    for invalid in ["-", "999999"] {
        cx.update_window(harness.window, |_, window, cx| {
            window.click(format!("timeline-number-KeyValue({key:?})"), cx);
            window.press("cmd-a", cx);
            window.input("0.25", cx);
        })
        .unwrap();
        cx.update_window(harness.window, |_, window, cx| {
            window.press("cmd-a", cx);
            window.input(invalid, cx);
        })
        .unwrap();
        cx.update_window(harness.window, |_, window, cx| {
            window.press("enter", cx);
        })
        .unwrap();
        cx.run_until_parked();
        harness.view.read_with(cx, |view, _| {
            assert!(view.editor.timeline_edit().is_none());
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
            assert_eq!(
                view.editor
                    .timeline_projection()
                    .unwrap()
                    .key(&key)
                    .unwrap()
                    .value,
                original
            );
        });
    }
}
