//! Regressions for asset scrubbing, paste targets, selection and narrow docks.
use super::slide_number_input::{press, release, slide};
use super::{Harness, document_fixture, setup_with_document};
use editor_core::{CommandId, TimelineChannelId, TimelineCommand};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, Focusable, ScrollDelta, TestAppContext, point, px};

fn animation(cx: &mut TestAppContext) -> Harness {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
    })
    .unwrap();
    harness
}

#[gpui_kit::test]
fn escape_ends_playhead_scrubbing(cx: &mut TestAppContext) {
    let harness = animation(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let position = window.find("timeline-ruler").bounds().center();
        press(window, position, cx);
        let timeline = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .clone();
        let binding = harness
            .view
            .read(cx)
            .editor
            .timeline_binding(timeline.read(cx).id)
            .unwrap();
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .active_timeline_scrub(binding)
                .is_some()
        );
        window.press("escape", cx);
        assert!(!timeline.read(cx).pointer_active());
        assert!(
            !harness
                .view
                .read(cx)
                .editor
                .active_timeline_scrub(binding)
                .is_some()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn deferred_scrub_deactivation_is_idempotent_and_preserves_new_playback(cx: &mut TestAppContext) {
    for restart in [false, true] {
        let harness = animation(cx);
        let binding = cx
            .update_window(harness.window, |_, window, cx| {
                let position = window.find("timeline-ruler").bounds().center();
                press(window, position, cx);
                let timeline = harness
                    .view
                    .read(cx)
                    .dock
                    .as_ref()
                    .unwrap()
                    .timeline
                    .clone();
                harness.view.update(cx, |view, cx| {
                    let binding = view.editor.timeline_binding(timeline.read(cx).id).unwrap();
                    assert!(view.editor.active_timeline_scrub(binding).is_some());
                    timeline.update(cx, |timeline, cx| timeline.deactivate(window, cx));
                    timeline.update(cx, |timeline, cx| timeline.deactivate(window, cx));
                    if restart {
                        view.editor.toggle_animation().unwrap();
                    }
                    binding
                })
            })
            .unwrap();
        cx.run_until_parked();
        harness.view.read_with(cx, |view, _| {
            assert!(!view.editor.active_timeline_scrub(binding).is_some());
            assert_eq!(view.editor.animation().unwrap().playing, restart);
        });
    }
}

#[gpui_kit::test]
fn deferred_deactivation_preserves_a_newer_edit(cx: &mut TestAppContext) {
    let harness = animation(cx);
    let (document, replacement, key) = cx
        .update_window(harness.window, |_, window, cx| {
            let timeline = harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .timeline
                .clone();
            let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
            let key = projection.tracks.values().next().unwrap().keys[0]
                .reference
                .clone();
            let time = projection.key(&key).unwrap().time;
            let origin = timeline.read(cx).key_position(&key).unwrap() + point(px(2.), px(0.));
            press(window, origin, cx);
            slide(window, origin + point(px(40.), px(0.)), false, cx);

            // Dock callbacks run while the owner is borrowed. A replacement
            // edit can arrive before their deferred cancellation is delivered.
            harness.view.update(cx, |view, cx| {
                let original = view.editor.timeline_edit().unwrap().identity;
                timeline.update(cx, |timeline, cx| timeline.deactivate(window, cx));
                view.editor.cancel_timeline_edit();
                let document = view.editor.active_id().unwrap();
                let replacement = view
                    .editor
                    .begin_timeline_edit(document, "Replacement")
                    .unwrap();
                assert_ne!(original.edit, replacement.edit);
                view.editor
                    .update_timeline_edit(
                        document,
                        replacement,
                        TimelineCommand::SetKey {
                            key: key.clone(),
                            time,
                            value: 0.75,
                        },
                    )
                    .unwrap();
                (document, replacement, key)
            })
        })
        .unwrap();
    cx.run_until_parked();
    harness.view.update(cx, |view, cx| {
        assert_eq!(
            view.editor.timeline_edit().unwrap().identity.edit,
            replacement.edit
        );
        assert_eq!(
            view.editor
                .timeline_projection()
                .unwrap()
                .key(&key)
                .unwrap()
                .value,
            0.75
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        assert!(
            !view
                .dock
                .as_ref()
                .unwrap()
                .timeline
                .read(cx)
                .pointer_active()
        );
        view.editor
            .finish_timeline_edit(document, replacement)
            .unwrap();
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
        assert_eq!(
            view.editor
                .timeline_projection()
                .unwrap()
                .key(&key)
                .unwrap()
                .value,
            0.75
        );
    });
}

#[gpui_kit::test]
fn motion_property_scrubs_use_unit_steps_and_commit_once(cx: &mut TestAppContext) {
    let harness = animation(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("timeline-properties", cx);
        window.render_frame(cx);
    })
    .unwrap();
    for (field, expected) in [
        ("Duration", 2.01_f32),
        ("Fps", 61.),
        ("FadeIn", 0.01),
        ("FadeOut", 0.01),
        ("Scale", 2.02),
    ] {
        cx.update_window(harness.window, |_, window, cx| {
            let bounds = window.find(format!("timeline-number-{field}")).bounds();
            let viewport = window.find("timeline-inspector").bounds();
            window.scroll(
                "timeline-inspector",
                ScrollDelta::Pixels(point(px(0.), viewport.center().y - bounds.center().y)),
                cx,
            );
        })
        .unwrap();
        let end = cx
            .update_window(harness.window, |_, window, cx| {
                window.render_frame(cx);
                let origin = window
                    .find(format!("timeline-number-{field}"))
                    .bounds()
                    .center();
                let end = origin + point(px(8.), px(0.));
                press(window, origin, cx);
                slide(window, end, false, cx);
                end
            })
            .unwrap();
        harness.view.read_with(cx, |view, _| {
            let clip = view
                .editor
                .timeline_edit()
                .expect("scrub candidate")
                .candidate
                .clip();
            let actual = match field {
                "Duration" | "Scale" => clip.duration,
                "Fps" => clip.fps,
                "FadeIn" => clip.fade_in.unwrap(),
                "FadeOut" => clip.fade_out.unwrap(),
                _ => unreachable!(),
            };
            assert!((actual - expected).abs() < 0.00001, "{field}: {actual}");
            assert_eq!(view.editor.document().unwrap().history_lengths().0, 0);
        });
        cx.update_window(harness.window, |_, window, cx| {
            release(window, end, cx);
            window.render_frame(cx);
            assert!(harness.view.read(cx).editor.timeline_edit().is_none());
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
            harness
                .view
                .update(cx, |view, cx| view.run_command(CommandId::Undo, window, cx));
            window.render_frame(cx);
            let p = harness.view.read(cx).editor.timeline_projection().unwrap();
            assert_eq!(p.duration, 2.);
            assert_eq!(p.fps, 60.);
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn selected_keys_never_expose_edit_head_value_controls(cx: &mut TestAppContext) {
    let harness = animation(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let projection = harness.view.read(cx).editor.timeline_projection().unwrap();
        let row = projection
            .rows
            .iter()
            .find(|r| r.track_id.is_some())
            .unwrap();
        for key in &row.keys {
            harness.view.update(cx, |view, cx| {
                let d = view.editor.active_id().unwrap();
                view.editor
                    .update_timeline_view(d, |s| {
                        s.channels.clear();
                        s.keys.clear();
                        s.channels.insert(row.id.clone());
                        s.keys.insert(key.clone());
                    })
                    .unwrap();
                view.redraw(cx);
            });
            window.render_frame(cx);
            let inspector = window.within("timeline-inspector");
            assert!(
                inspector
                    .try_find(format!("timeline-number-KeyValue({key:?})"))
                    .is_some()
            );
            assert!(
                inspector
                    .try_find(format!("timeline-number-RowValue({:?})", row.id))
                    .is_none()
            );
        }
        harness.view.update(cx, |view, cx| {
            let d = view.editor.active_id().unwrap();
            view.editor
                .update_timeline_view(d, |s| s.keys.clear())
                .unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
        assert!(
            window
                .within("timeline-inspector")
                .try_find(format!("timeline-number-RowValue({:?})", row.id))
                .is_some()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn multi_track_shortcut_paste_ignores_single_channel_target(cx: &mut TestAppContext) {
    let harness = animation(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let d = view.editor.active_id().unwrap();
            view.editor
                .execute_timeline(
                    d,
                    TimelineCommand::WriteValue {
                        channel: TimelineChannelId::Model("Opacity".into()),
                        track_id: None,
                        selected_key: None,
                        value: 1.,
                    },
                )
                .unwrap();
            let p = view.editor.timeline_projection().unwrap();
            let keys: Vec<_> = p
                .tracks
                .values()
                .map(|t| t.keys[0].reference.clone())
                .collect();
            assert_eq!(keys.len(), 2);
            view.editor.reveal_timeline_keys(d, &keys).unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
        let focus = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .focus_handle(cx);
        window.focus(&focus, cx);
        crate::keybindings::reset(cx).unwrap();
        window.hover("timeline-graph", cx);
        window.press("cmd-c", cx);
        assert!(
            !harness
                .view
                .read(cx)
                .editor
                .timeline_clipboard_can_retarget()
        );
        harness.view.update(cx, |view, cx| {
            let d = view.editor.active_id().unwrap();
            let p = view.editor.timeline_projection().unwrap();
            let row = p
                .rows
                .iter()
                .find(|r| r.track_id.is_some())
                .unwrap()
                .id
                .clone();
            view.editor
                .update_timeline_view(d, |s| {
                    s.keys.clear();
                    s.channels.clear();
                    s.channels.insert(row);
                })
                .unwrap();
            view.editor.seek_timeline(d, 0.5).unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
        window.press("cmd-v", cx);
        let view = harness.view.read(cx);
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        let p = view.editor.timeline_projection().unwrap();
        assert!(
            p.tracks
                .values()
                .all(|t| t.keys.iter().any(|k| k.time == 0.5))
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
        harness
            .view
            .update(cx, |view, cx| view.run_command(CommandId::Undo, window, cx));
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_projection()
                .unwrap()
                .tracks
                .values()
                .all(|t| t.keys.iter().all(|k| k.time != 0.5))
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn side_dock_keeps_graph_and_tools_accessible(cx: &mut TestAppContext) {
    use gpui_kit::component::dock::{DockPlacement, InsertTarget, PanelId};
    let harness = animation(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let dock = harness.view.read(cx).dock.as_ref().unwrap();
        let panel = PanelId::from(dock.timeline.entity_id());
        let info = PanelId::from(dock.animation_info.entity_id());
        let area = dock.area.clone();
        let node = area
            .read(cx)
            .layout(DockPlacement::Center)
            .unwrap()
            .find_panel_node(info)
            .unwrap();
        area.update(cx, |area, cx| {
            area.move_panel(
                panel,
                InsertTarget::Tabs {
                    node,
                    ix: None,
                    activate: true,
                },
                window,
                cx,
            )
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("timeline-graph").bounds().size.width >= px(150.));
        assert!(window.find("timeline-graph").bounds().size.height >= px(50.));
        assert!(window.find("animation-preview-mode").visible());
        assert!(window.find("timeline-more").visible());
        assert!(window.try_find("timeline-inspector").is_none());
        let key = harness
            .view
            .read(cx)
            .editor
            .timeline_projection()
            .unwrap()
            .tracks
            .values()
            .next()
            .unwrap()
            .keys[0]
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
        slide(window, origin + point(px(12.), px(0.)), false, cx);
        assert!(harness.view.read(cx).editor.timeline_edit().is_some());
        release(window, origin + point(px(12.), px(0.)), cx);
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
        harness
            .view
            .update(cx, |view, cx| view.run_command(CommandId::Undo, window, cx));
        harness.view.update(cx, |view, cx| {
            let document = view.editor.active_id().unwrap();
            view.editor
                .update_timeline_view(document, |state| {
                    state.mode = editor_core::TimelineDisplayMode::Curves;
                })
                .unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
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
        slide(window, origin - point(px(0.), px(8.)), false, cx);
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .timeline_edit()
                .unwrap()
                .candidate
                .clip()
                .tracks[0]
                .initial
                .value
                > 0.
        );
        release(window, origin - point(px(0.), px(8.)), cx);
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
        harness
            .view
            .update(cx, |view, cx| view.run_command(CommandId::Undo, window, cx));
        window.render_frame(cx);
        window.click("timeline-more", cx);
        window.render_frame(cx);
        window.click("timeline-menu-properties", cx);
        window.render_frame(cx);
        assert!(window.find("timeline-number-Duration").visible());
        assert!(window.find("timeline-graph").bounds().size.width >= px(150.));
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("timeline-inspector-close", cx);
        window.render_frame(cx);
        assert!(window.try_find("timeline-inspector").is_none());
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 1)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn keyboard_navigation_reveals_hidden_keys_and_preserves_zoom(cx: &mut TestAppContext) {
    let harness = animation(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let d = view.editor.active_id().unwrap();
            view.editor
                .update_timeline_view(d, |s| {
                    s.time_range = (0., 0.1);
                    s.search = "no matching channel".into();
                })
                .unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
        let focus = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .read(cx)
            .focus_handle(cx);
        window.focus(&focus, cx);
        crate::keybindings::reset(cx).unwrap();
        window.hover("timeline-graph", cx);
        window.press("cmd-right", cx);
        let view = harness.view.read(cx);
        assert_eq!(view.editor.animation().unwrap().edit_time, 2.);
        let s = view.editor.timeline_view().unwrap();
        assert!(s.search.is_empty());
        assert_eq!(s.keys.len(), 1);
        assert!(s.time_range.0 < 2. && s.time_range.1 > 2.);
        assert!((s.time_range.1 - s.time_range.0 - 0.1).abs() < 1e-9);
        harness.view.update(cx, |view, cx| {
            let d = view.editor.active_id().unwrap();
            view.editor.seek_timeline(d, 0.5).unwrap();
            view.editor
                .update_timeline_view(d, |s| s.time_range = (0., 0.1))
                .unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
        window.press("right", cx);
        let view = harness.view.read(cx);
        let time = f64::from(view.editor.animation().unwrap().edit_time);
        let range = view.editor.timeline_view().unwrap().time_range;
        assert!(range.0 < time && range.1 > time);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    })
    .unwrap();
}

#[gpui_kit::test]
fn key_list_reveals_keys_outside_the_time_range(cx: &mut TestAppContext) {
    let harness = animation(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let d = view.editor.active_id().unwrap();
            let p = view.editor.timeline_projection().unwrap();
            let row = p
                .rows
                .iter()
                .find(|r| r.track_id.is_some())
                .unwrap()
                .id
                .clone();
            view.editor
                .update_timeline_view(d, |s| {
                    s.time_range = (0., 0.1);
                    s.channels.insert(row);
                })
                .unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
        window.scroll(
            "timeline-inspector",
            ScrollDelta::Pixels(point(px(0.), px(-10000.))),
            cx,
        );
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.click("timeline-key-list", cx);
        window.render_frame(cx);
        window.press("up", cx);
        window.press("enter", cx);
        window.render_frame(cx);
        let view = harness.view.read(cx);
        let s = view.editor.timeline_view().unwrap();
        let p = view.editor.timeline_projection().unwrap();
        let key = p.key(s.keys.first().expect("selected key")).unwrap();
        assert_eq!(key.time, 2.);
        assert!(s.time_range.0 < key.time && s.time_range.1 > key.time);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    })
    .unwrap();
}

#[gpui_kit::test]
fn shared_header_preserves_control_focus_without_an_implicit_timeline_target(
    cx: &mut TestAppContext,
) {
    let harness = animation(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.blur(cx);
        let mut found = false;
        for _ in 0..64 {
            window.focus_next(cx);
            window.render_frame(cx);
            if window.find("animation-preview-mode").focused() == Some(true) {
                found = true;
                break;
            }
        }
        assert!(
            found,
            "transport remains reachable by keyboard in the shared header"
        );
        window.hover("animation-preview-mode", cx);
        let before = harness.view.read(cx).editor.animation().unwrap().edit_time;
        window.press("right", cx);
        let after = harness.view.read(cx).editor.animation().unwrap().edit_time;
        assert_eq!(after, before);
        assert_eq!(window.find("animation-preview-mode").focused(), Some(true));
    })
    .unwrap();
}

#[gpui_kit::test]
fn deferred_deactivation_preserves_a_new_scrub_in_the_same_panel(cx: &mut TestAppContext) {
    let harness = animation(cx);
    let replacement = cx
        .update_window(harness.window, |_, window, cx| {
            let position = window.find("timeline-ruler").bounds().center();
            press(window, position, cx);
            let timeline = harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .timeline
                .clone();
            harness.view.update(cx, |view, cx| {
                let binding = view.editor.timeline_binding(timeline.read(cx).id).unwrap();
                timeline.update(cx, |timeline, cx| timeline.deactivate(window, cx));
                let token = view.editor.begin_timeline_scrub(binding, 0.5).unwrap();
                (token, view.editor.animation().unwrap().time)
            })
        })
        .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.timeline_scrub_valid(replacement.0));
        assert_eq!(view.editor.animation().unwrap().time, replacement.1);
    });
}

#[gpui_kit::test]
fn blocking_ui_releases_stale_capture_without_invalidating_its_dialog_context(
    cx: &mut TestAppContext,
) {
    let harness = animation(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let position = window.find("timeline-ruler").bounds().center();
        press(window, position, cx);
        let timeline = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .timeline
            .clone();
        harness.view.update(cx, |view, cx| {
            let document = view.editor.active_id().unwrap();
            view.editor
                .end_editing(document, editor_core::EditingLifecycle::BlockingUi)
                .unwrap();
            let dialog = view.editor.editing_context_token().unwrap();
            assert!(timeline.read(cx).pointer_active());
            assert!(!view.suspend_input(editor_core::EditingLifecycle::BlockingUi, window, cx));
            assert!(!timeline.read(cx).pointer_active());
            assert!(view.editor.editing_context_token_valid(dialog));
            assert!(!view.suspend_input(editor_core::EditingLifecycle::BlockingUi, window, cx));
            assert!(view.editor.editing_context_token_valid(dialog));
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn blocking_ui_ends_domain_scrub_without_a_local_pointer_owner(cx: &mut TestAppContext) {
    let harness = animation(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let panel = view.dock.as_ref().unwrap().timeline.read(cx).id;
            let binding = view.editor.timeline_binding(panel).unwrap();
            let scrub = view.editor.begin_timeline_scrub(binding, 0.5).unwrap();
            assert!(view.suspend_input(editor_core::EditingLifecycle::BlockingUi, window, cx));
            assert!(!view.editor.timeline_scrub_valid(scrub));
            assert!(!view.editor.editing_active());
        });
    })
    .unwrap();
}
