use super::super::slide_number_input::{press, release, slide};
use super::super::{document_fixture, setup_with_document};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, point, px};
use zipzi_editor_core::TimelineRowId;

#[gpui_kit::test]
fn overlapping_events_can_be_selected_and_edited_individually(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        for _ in 0..2 {
            crate::headless_tests::timeline::tool(window, "timeline-add-event", cx);
            window.render_frame(cx);
        }
        window.click("timeline-row-Events", cx);
        window.render_frame(cx);
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_view()
                .unwrap()
                .events
                .is_empty()
        );
    })
    .unwrap();
    for (index, text) in [(0usize, "first"), (1usize, "second")] {
        cx.update_window(harness.window, |_, window, cx| {
            window.click("property-navigation", cx);
            window.render_frame(cx);
            window.click(("property-navigation-choice", index), cx);
            window.render_frame(cx);
            let view = harness.view.read(cx);
            let projection = view.editor.timeline_projection().unwrap();
            assert_eq!(
                view.editor
                    .timeline_view()
                    .unwrap()
                    .events
                    .iter()
                    .collect::<Vec<_>>(),
                vec![&projection.events[index].id]
            );
            assert_eq!(
                view.editor.document().unwrap().history_lengths(),
                (2 + index, 0)
            );
            window.click("property-input-event_value", cx);
            window.input(text, cx);
            window.press("enter", cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            let view = harness.view.read(cx);
            let projection = view.editor.timeline_projection().unwrap();
            assert_eq!(projection.events[index].value, text);
            assert_eq!(projection.events[0].time, projection.events[1].time);
            assert_eq!(
                view.editor.document().unwrap().history_lengths(),
                (3 + index, 0)
            );
            window.click("timeline-row-Events", cx);
            window.render_frame(cx);
        })
        .unwrap();
    }
    harness.view.read_with(cx, |view, _| {
        let projection = view.editor.timeline_projection().unwrap();
        assert_eq!(projection.events[0].value, "first");
        assert_eq!(projection.events[1].value, "second");
    });
}

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
        crate::headless_tests::timeline::tool(window, "timeline-seconds", cx);
        crate::headless_tests::timeline::tool(window, "timeline-fit", cx);
        crate::headless_tests::timeline::tool(window, "timeline-snap", cx);
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
        crate::headless_tests::timeline::tool(window, "timeline-add-event", cx);
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
        crate::keybindings::reset(cx).unwrap();
        window.hover("timeline-graph", cx);
        window.press("backspace", cx);
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
        crate::headless_tests::timeline::tool(window, "timeline-add-event", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.click("property-input-event_value", cx);
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
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.timeline_edit().is_none());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.scroll(
            "object-properties",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-10000.))),
            cx,
        );
        crate::headless_tests::timeline::tool(window, "timeline-add-event", cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        assert_eq!(projection.events.len(), 2);
        assert_eq!(projection.events[0].time, projection.events[1].time);
        assert_ne!(projection.events[0].id, projection.events[1].id);
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
fn motion_fade_toggles_use_shared_properties(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        window.scroll(
            "object-properties",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(10000.))),
            cx,
        );
        window.scroll(
            "object-properties",
            gpui_kit::ScrollDelta::Pixels(point(px(0.), px(-90.))),
            cx,
        );
        window.click("property-custom-fade_in", cx);
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
        window.click("property-custom-fade_in", cx);
        window.render_frame(cx);
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        assert_eq!(projection.duration, 2.);
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
            (2, 0)
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
        window.click("property-input-selection_value", cx);
        window.press("cmd-a", cx);
        window.input("0.25", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.click("property-input-selection_time", cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert!(
            view.editor.timeline_edit().is_none(),
            "the previous property must commit before focusing the next one"
        );
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
        window.click("property-input-selection_value", cx);
        let Some(AnyInputState::Input(input)) = window.focused_input(cx) else {
            panic!("key value input not focused")
        };
        assert_eq!(input.read(cx).value().as_ref(), "0.25");
        window.blur(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let origin = window
            .find("property-input-selection_value")
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
            window.click("property-input-selection_value", cx);
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

fn selected_key_fixture(
    cx: &mut TestAppContext,
) -> (
    super::super::Harness,
    zipzi_editor_core::TimelineKeyRef,
    f32,
) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    let result = cx
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
    (harness, result.0, result.1)
}

fn settle_input(
    harness: &super::super::Harness,
    how: &str,
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::App,
) {
    match how {
        "enter" => window.press("enter", cx),
        "blur" => window.blur(cx),
        "save" => harness.view.update(cx, |view, cx| {
            assert!(view.prepare_property_inputs(view.editor.active_id().unwrap(), window, cx));
        }),
        _ => unreachable!(),
    }
    window.render_frame(cx);
}

#[gpui_kit::test]
fn returning_live_text_to_baseline_discards_the_candidate(cx: &mut TestAppContext) {
    let (harness, key, original) = selected_key_fixture(cx);
    for how in ["enter", "blur", "save"] {
        cx.update_window(harness.window, |_, window, cx| {
            window.click("property-input-selection_value", cx);
            window.press("cmd-a", cx);
            window.input("0.25", cx);
            window.render_frame(cx);
        })
        .unwrap();
        cx.update_window(harness.window, |_, window, cx| {
            window.press("cmd-a", cx);
            window.input(&original.to_string(), cx);
            settle_input(&harness, how, window, cx);
        })
        .unwrap();
        cx.run_until_parked();
        harness.view.read_with(cx, |view, _| {
            assert!(view.editor.timeline_edit().is_none(), "{how}");
            assert_eq!(
                view.editor
                    .timeline_projection()
                    .unwrap()
                    .key(&key)
                    .unwrap()
                    .value,
                original,
                "{how}"
            );
            assert_eq!(
                view.editor.document().unwrap().history_lengths(),
                (0, 0),
                "{how}"
            );
        });
    }
}

#[gpui_kit::test]
fn colliding_key_time_discards_the_last_valid_candidate(cx: &mut TestAppContext) {
    let (harness, key, _) = selected_key_fixture(cx);
    for how in ["enter", "blur", "save"] {
        let (original, collision) = harness.view.read_with(cx, |view, _| {
            let p = view.editor.timeline_projection().unwrap();
            (
                p.key(&key).unwrap().time,
                p.tracks[&key.track_id].keys.last().unwrap().time,
            )
        });
        cx.update_window(harness.window, |_, window, cx| {
            window.click("property-input-selection_time", cx);
            window.press("cmd-a", cx);
            window.input("0.5", cx);
            window.render_frame(cx);
        })
        .unwrap();
        cx.update_window(harness.window, |_, window, cx| {
            window.press("cmd-a", cx);
            window.input(&collision.to_string(), cx);
            settle_input(&harness, how, window, cx);
        })
        .unwrap();
        cx.run_until_parked();
        harness.view.read_with(cx, |view, _| {
            assert!(view.editor.timeline_edit().is_none(), "{how}");
            assert_eq!(
                view.editor
                    .timeline_projection()
                    .unwrap()
                    .key(&key)
                    .unwrap()
                    .time,
                original,
                "{how}"
            );
            assert_eq!(
                view.editor.document().unwrap().history_lengths(),
                (0, 0),
                "{how}"
            );
        });
    }
}

#[gpui_kit::test]
fn scrubbing_selection_value_back_to_baseline_creates_no_history(cx: &mut TestAppContext) {
    let (harness, key, original) = selected_key_fixture(cx);
    let origin = cx
        .update_window(harness.window, |_, window, cx| {
            let origin = window
                .find("property-input-selection_value")
                .bounds()
                .center();
            press(window, origin, cx);
            slide(window, origin + point(px(20.), px(0.)), false, cx);
            origin
        })
        .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_ne!(
            view.editor
                .timeline_projection()
                .unwrap()
                .key(&key)
                .unwrap()
                .value,
            original
        );
    });
    cx.update_window(harness.window, |_, window, cx| {
        slide(window, origin, false, cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        release(window, origin, cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .timeline_projection()
                .unwrap()
                .key(&key)
                .unwrap()
                .value,
            original
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
}
