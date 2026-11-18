use super::super::slide_number_input::{press, release, slide};
use super::super::{Harness, document_fixture, setup_with_document};
use gpui_kit::{
    App, AppContext as _, MouseButton, TestAppContext, Window, point, px,
    test::{ClickOptions, TestWindowExt},
};
use zipzi_contract::{
    kernel_commands::MotionInterpolation,
    motion_curve::{MotionEasingPreset, MotionFrameStep},
};
use zipzi_editor_core::{CommandId, TimelineKeyRef};

fn setup(cx: &mut TestAppContext) -> Harness {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, w, c| {
        w.click("workspace-animation", c);
        w.render_frame(c);
        w.click(("animation-motion", 0usize), c);
        w.render_frame(c);
    })
    .unwrap();
    h
}
fn select(h: &Harness, w: &mut Window, c: &mut App) -> TimelineKeyRef {
    let p = h.view.read(c).editor.timeline_projection().unwrap();
    let key = p.tracks.values().next().unwrap().keys[0].reference.clone();
    let position = h
        .view
        .read(c)
        .dock
        .as_ref()
        .unwrap()
        .timeline
        .read(c)
        .key_position(&key)
        .unwrap()
        + point(px(2.), px(0.));
    press(w, position, c);
    release(w, position, c);
    w.render_frame(c);
    key
}
fn open(w: &mut Window, c: &mut App) {
    w.click("timeline-curve-menu", c);
    w.render_frame(c);
    assert!(w.find("timeline-curve-settings-popover").visible());
}

#[gpui_kit::test]
fn curve_authoring_toolbar_inspector_and_menu_share_real_selection_and_one_undo(
    cx: &mut TestAppContext,
) {
    let h = setup(cx);
    let key = cx
        .update_window(h.window, |_, w, c| {
            assert!(w.try_find("timeline-curve-settings").is_none());
            w.click("timeline-curve-menu", c);
            w.render_frame(c);
            assert!(w.try_find("timeline-curve-settings-popover").is_none());
            let key = select(&h, w, c);
            let graph = w.find("timeline-graph").bounds();
            open(w, c);
            assert_eq!(w.find("timeline-graph").bounds(), graph);
            assert_eq!(
                h.view.read(c).editor.timeline_view().unwrap().mode,
                zipzi_editor_core::TimelineDisplayMode::Overview
            );
            key
        })
        .unwrap();
    cx.run_until_parked();
    for item in [
        "timeline-interpolation-auto_smooth",
        "timeline-interpolation-bezier",
        "timeline-easing",
        "timeline-easing-family-sine",
        "timeline-easing-family-cubic",
        "timeline-easing-cubic_out",
        "timeline-easing-cubic_out",
        "timeline-frame-step-3",
    ] {
        cx.update_window(h.window, |_, w, c| {
            w.render_frame(c);
            w.click(item, c);
            w.render_frame(c);
            assert!(w.find("timeline-curve-settings-popover").visible());
            if item == "timeline-easing-family-sine" {
                assert!(w.find("timeline-easing-sine_in").visible());
                assert!(w.find("timeline-easing-sine_in_out").visible());
                assert!(w.try_find("timeline-easing-cubic_out").is_none());
            }
        })
        .unwrap();
        cx.run_until_parked();
    }
    cx.update_window(h.window, |_, w, c| {
        for (checked, unchecked) in [
            (
                "timeline-interpolation-bezier",
                "timeline-interpolation-auto_smooth",
            ),
            ("timeline-easing-cubic_out", "timeline-easing-cubic_in"),
            ("timeline-frame-step-3", "timeline-frame-step-1"),
        ] {
            assert_eq!(w.find(checked).role(), Some(gpui_kit::Role::RadioButton));
            assert_eq!(w.find(checked).checked(), Some(true));
            assert_eq!(w.find(unchecked).checked(), Some(false));
        }
        assert_eq!(
            w.find("timeline-aligned-handles").role(),
            Some(gpui_kit::Role::CheckBox)
        );
        let p = h.view.read(c).editor.timeline_projection().unwrap();
        let s = p.curve_settings(&[key.clone()], false);
        assert_eq!(
            s.preset,
            zipzi_editor_core::TimelineMixed::Uniform(Some(MotionEasingPreset::CubicOut))
        );
        assert_eq!(
            s.frame_step,
            zipzi_editor_core::TimelineMixed::Uniform(MotionFrameStep::EveryFrames { frames: 3 })
        );
        assert!(w.find("property-select-easing_preset").visible());
        assert!(w.find("property-input-frame_step").visible());
        assert_eq!(
            h.view.read(c).editor.document().unwrap().history_lengths(),
            (4, 0)
        );
        h.view
            .update(c, |v, c| v.run_command(CommandId::Undo, w, c));
        w.render_frame(c);
        assert_eq!(
            h.view
                .read(c)
                .editor
                .timeline_projection()
                .unwrap()
                .key(&key)
                .unwrap()
                .frame_step,
            MotionFrameStep::Off {}
        );
        h.view
            .update(c, |v, c| v.run_command(CommandId::Redo, w, c));
        w.render_frame(c);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, w, c| {
        w.render_frame(c);
        w.click_with_options(
            "timeline-graph",
            ClickOptions::new().with_button(MouseButton::Right),
            c,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, w, c| {
        w.render_frame(c);
        w.click("timeline-menu-curve-settings", c);
        w.render_frame(c);
        assert!(w.find("timeline-curve-settings-popover").visible());
        w.click("timeline-view", c);
        w.render_frame(c);
        assert!(w.try_find("timeline-curve-settings-popover").is_none());
        let state = h.view.read(c).editor.timeline_view().unwrap();
        assert_eq!(state.mode, zipzi_editor_core::TimelineDisplayMode::Curves);
        assert!(state.keys.contains(&key));
        open(w, c);
        assert!(w.find("timeline-easing").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn curve_authoring_n_uses_draft_cancel_single_commit_and_integer_validation(
    cx: &mut TestAppContext,
) {
    let h = setup(cx);
    let key = cx
        .update_window(h.window, |_, w, c| {
            let key = select(&h, w, c);
            open(w, c);
            assert!(w.try_find("timeline-frame-step-number").is_none());
            w.click("timeline-frame-step-4", c);
            w.render_frame(c);
            w.press("tab", c);
            w.render_frame(c);
            key
        })
        .unwrap();
    cx.run_until_parked();
    for cancel in [true, false] {
        let origin = cx
            .update_window(h.window, |_, w, c| {
                w.render_frame(c);
                let p = w.find("timeline-frame-step-number").bounds().center();
                press(w, p, c);
                w.render_frame(c);
                p
            })
            .unwrap();
        cx.update_window(h.window, |_, w, c| {
            slide(w, origin + point(px(40.), px(0.)), false, c);
            w.render_frame(c);
        })
        .unwrap();
        cx.run_until_parked();
        h.view.read_with(cx, |v, _| {
            assert!(v.editor.timeline_edit().is_some());
            assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0));
        });
        cx.update_window(h.window, |_, w, c| {
            if cancel {
                w.press("escape", c);
            }
            release(w, origin + point(px(40.), px(0.)), c);
            w.render_frame(c);
        })
        .unwrap();
        cx.run_until_parked();
        if cancel {
            h.view.read_with(cx, |v, _| {
                assert_eq!(
                    v.editor
                        .timeline_projection()
                        .unwrap()
                        .key(&key)
                        .unwrap()
                        .frame_step,
                    MotionFrameStep::Off {}
                )
            });
        }
    }
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (1, 0))
    });
    for value in ["0", "-2", "2.5", "4294967295"] {
        cx.update_window(h.window, |_, w, c| {
            w.click("timeline-frame-step-number", c);
            w.render_frame(c);
            w.press("cmd-a", c);
            w.input(value, c);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(h.window, |_, w, c| {
            w.press("enter", c);
            w.render_frame(c);
        })
        .unwrap();
        cx.run_until_parked();
        h.view.read_with(cx, |v, _| {
            assert!(v.editor.timeline_edit().is_none());
            assert_eq!(
                v.editor.document().unwrap().history_lengths(),
                (if value == "4294967295" { 2 } else { 1 }, 0)
            );
        });
    }
    h.view.read_with(cx, |v, _| {
        assert_eq!(
            v.editor
                .timeline_projection()
                .unwrap()
                .key(&key)
                .unwrap()
                .frame_step,
            MotionFrameStep::EveryFrames { frames: u32::MAX }
        )
    });
}

#[gpui_kit::test]
fn curve_authoring_terminal_mixed_and_event_selection_explain_disabled_state(
    cx: &mut TestAppContext,
) {
    let h = setup(cx);
    cx.update_window(h.window, |_, w, c| {
        let first = select(&h, w, c);
        let p = h.view.read(c).editor.timeline_projection().unwrap();
        let last = p.tracks[&first.track_id]
            .keys
            .last()
            .unwrap()
            .reference
            .clone();
        h.view.update(c, |v, c| {
            v.editor
                .reveal_timeline_keys(v.editor.active_id().unwrap(), &[last.clone()])
                .unwrap();
            v.redraw(c);
        });
        w.render_frame(c);
        w.click("timeline-curve-menu", c);
        w.render_frame(c);
        assert!(w.try_find("timeline-curve-settings-popover").is_none());
        w.click("timeline-view", c);
        w.render_frame(c);
        assert_eq!(
            h.view.read(c).editor.timeline_view().unwrap().mode,
            zipzi_editor_core::TimelineDisplayMode::Curves
        );
        h.view.update(c, |v, c| {
            v.editor
                .reveal_timeline_keys(v.editor.active_id().unwrap(), &[first, last])
                .unwrap();
            v.redraw(c);
        });
        w.render_frame(c);
        open(w, c);
        assert!(
            w.find("timeline-curve-status")
                .label()
                .unwrap()
                .contains("1 intervals")
        );
        super::tool(w, "timeline-add-event", c);
        w.click("timeline-curve-menu", c);
        w.render_frame(c);
        assert!(w.try_find("timeline-curve-settings-popover").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn curve_authoring_narrow_panel_keeps_a_labeled_menu_and_custom_n(cx: &mut TestAppContext) {
    use gpui_kit::component::dock::{DockPlacement, InsertTarget, PanelId};
    let h = setup(cx);
    cx.update_window(h.window, |_, w, c| {
        select(&h, w, c);
        let dock = h.view.read(c).dock.as_ref().unwrap();
        let area = dock.area.clone();
        let panel = PanelId::from(dock.timeline.entity_id());
        let info = PanelId::from(dock.properties.entity_id());
        let node = area
            .read(c)
            .layout(DockPlacement::Center)
            .unwrap()
            .find_panel_node(info)
            .unwrap();
        area.update(c, |area, c| {
            area.move_panel(
                panel,
                InsertTarget::Tabs {
                    node,
                    ix: None,
                    activate: true,
                },
                w,
                c,
            )
        });
        w.render_frame(c);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, w, c| {
        w.render_frame(c);
        assert!(w.try_find("timeline-curve-menu").is_none());
        w.click("timeline-more", c);
        w.render_frame(c);
        assert!(w.find("timeline-menu-view").visible());
        assert_eq!(
            w.find("timeline-menu-curve-settings").label(),
            Some("Interval Curve Settings…")
        );
        w.click("timeline-menu-curve-settings", c);
        w.render_frame(c);
        assert!(w.find("timeline-curve-settings-popover").visible());
        assert!(w.try_find("timeline-frame-step-number").is_none());
        w.click("timeline-frame-step-4", c);
        w.render_frame(c);
        assert!(w.find("timeline-frame-step-number").visible());
        w.click("timeline-interpolation-auto_smooth", c);
        w.render_frame(c);
        let p = h.view.read(c).editor.timeline_projection().unwrap();
        let s = p.curve_settings(&h.view.read(c).editor.timeline_view().unwrap().keys, false);
        assert_eq!(
            s.interpolation,
            zipzi_editor_core::TimelineMixed::Uniform(MotionInterpolation::AutoSmooth)
        );
    })
    .unwrap();
}
