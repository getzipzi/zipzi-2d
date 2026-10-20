use super::{document_fixture, setup_with_document};
use editor_core::{AnimationEvaluator, CommandId};
use gpui_kit::{AppContext as _, TestAppContext, prelude::*, test::TestWindowExt};

#[gpui_kit::test]
fn motion_and_expression_rows_share_compact_height_at_every_ui_scale(cx: &mut TestAppContext) {
    use crate::settings::{Settings, UiScale};
    use gpui_kit::component::ThemeMode;
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    for scale in UiScale::ALL {
        cx.update_window(harness.window, |_, window, cx| {
            cx.update_global::<Settings, _>(|settings, _| settings.set_ui_scale(scale).unwrap());
            crate::theme::change(ThemeMode::Dark, cx);
            harness.view.update(cx, |view, cx| {
                view.run_command(CommandId::AnimationWorkspace, window, cx);
                cx.notify();
            });
            window.refresh();
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            for kind in ["animation-motion", "animation-expression"] {
                let height = window.find((kind, 0usize)).bounds().size.height.as_f32();
                assert!(
                    (height - 22. * scale.factor()).abs() <= 1.,
                    "{kind}: {height}"
                );
            }
        })
        .unwrap();
    }
}

fn reveal_property(
    window: &mut gpui_kit::Window,
    id: impl Into<gpui_kit::ElementId>,
    cx: &mut gpui_kit::App,
) {
    use gpui_kit::{InputEvent as _, ScrollDelta, ScrollWheelEvent, point, px};
    let id = id.into();
    for _ in 0..30 {
        if window.try_find(id.clone()).is_some() {
            return;
        }
        window.dispatch_event(
            ScrollWheelEvent {
                position: window.find("object-properties").bounds().center(),
                delta: ScrollDelta::Pixels(point(px(0.), px(-32.))),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    }
    panic!("property was not reachable by scrolling: {id:?}");
}

#[gpui_kit::test]
fn double_click_play_restart_and_keyboard_browsing_preserve_transport_contract(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        assert!(window.try_find(("animation-motion-play", 0usize)).is_none());
        window.click(("animation-motion", 0usize), cx);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        window.double_click(("animation-motion", 0usize), cx);
        assert!(harness.view.read(cx).editor.animation().unwrap().playing);
        harness
            .view
            .update(cx, |view, _| view.editor.advance_animation_clock(0.75));
        let anchor = harness.view.read(cx).animation_anchor;
        window.click(("animation-motion", 0usize), cx);
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert!(animation.playing);
        assert_eq!(animation.time, 0.75);
        assert_eq!(harness.view.read(cx).animation_anchor, anchor);
        window.double_click(("animation-motion", 0usize), cx);
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert!(animation.playing, "double click restarts a playing motion");
        assert_eq!(animation.time, 0.);
        harness
            .view
            .update(cx, |view, _| view.editor.advance_animation_clock(0.5));
        window.press("space", cx);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        window.click(("animation-motion", 0usize), cx);
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert!(!animation.playing);
        assert_eq!(animation.time, 0.5);
        window.double_click(("animation-motion", 0usize), cx);
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert!(animation.playing);
        assert_eq!(animation.time, 0.);
        window.press("down", cx);
        assert!(
            harness.view.read(cx).editor.animation().unwrap().playing,
            "browsing must not select"
        );
        window.press("enter", cx);
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert!(!animation.playing);
        assert_eq!(
            animation.motion.as_ref().unwrap().entry.as_ref().unwrap().1,
            1
        );
        window.press("space", cx);
        assert!(harness.view.read(cx).editor.animation().unwrap().playing);
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
fn motion_groups_collapse_without_changing_selection(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        assert!(window.try_find("animation-motion-search").is_none());
        assert!(window.find("animation-motion-toolbar").visible());
        assert!(window.find("animation-new-motion").visible());
        assert!(window.try_find("animation-expression-search").is_none());
        window.click(("animation-motion", 0usize), cx);
        window.click(("animation-motion-group", 0usize), cx);
        window.render_frame(cx);
        assert_eq!(
            window
                .find(("animation-motion-group-header", 0usize))
                .expanded(),
            Some(false)
        );
        assert_eq!(
            window
                .find(("animation-motion-group-count", 0usize))
                .value(),
            Some("2")
        );
        assert!(window.try_find(("animation-motion", 0usize)).is_none());
        assert!(window.try_find(("animation-motion", 1usize)).is_none());
        assert!(window.find(("animation-motion", 2usize)).visible());
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert_eq!(
            animation.motion.as_ref().unwrap().entry.as_ref().unwrap().1,
            0
        );
        assert!(!animation.playing);
        window.click(("animation-motion-group", 0usize), cx);
        window.render_frame(cx);
        assert!(window.find(("animation-motion", 0usize)).visible());
        assert!(window.find(("animation-motion", 1usize)).visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn expression_rows_show_active_icon_at_the_end_and_clear_without_editing(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        super::animation::click_expression(window, 0, cx);
        window.render_frame(cx);
        assert!(
            window
                .try_find(("animation-expression-status", 0usize))
                .is_none()
        );
        assert!(window.try_find("animation-expression-feedback").is_none());
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::PlayPause, window, cx);
            view.editor.advance_animation_clock(0.5);
            let request = view.editor.animation_request().unwrap();
            let frame = AnimationEvaluator::default()
                .evaluate(&request, |_, _| true)
                .unwrap();
            assert!(
                view.editor
                    .present_animation(frame.identity, frame.snapshot)
            );
            view.notify_animation(cx);
        });
        window.render_frame(cx);
        assert!(
            window
                .try_find(("animation-expression-status", 0usize))
                .is_none()
        );
        assert!(window.try_find("animation-expression-feedback").is_none());
        assert_eq!(
            harness
                .view
                .read(cx)
                .displayed_animation()
                .unwrap()
                .active_expressions
                .len(),
            1
        );
        let row = window.find(("animation-expression", 0usize)).bounds();
        let active = window
            .find(("animation-expression-active", 0usize))
            .bounds();
        assert!(active.center().x > row.center().x);
        assert!(active.right() <= row.right());
        assert!(window.find("animation-clear-expression").visible());
        window.click("animation-clear-expression", cx);
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert!(animation.playing);
        assert_eq!(animation.time, 0.5);
        assert!(animation.expressions.is_empty());
        assert!(animation.expression.is_none());
        assert!(!harness.view.read(cx).editor.document().unwrap().modified());
    })
    .unwrap();
}

#[gpui_kit::test]
fn motion_group_folding_is_restored_per_document(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    let original = harness
        .view
        .read_with(cx, |view, _| view.editor.active_id().unwrap());
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion-group", 0usize), cx);
        harness.view.update(cx, |view, cx| {
            view.editor.new_document().unwrap();
            cx.notify();
        });
        window.render_frame(cx);
        assert!(
            window
                .try_find(("animation-motion-group", 0usize))
                .is_none()
        );
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor.activate(original).unwrap();
            cx.notify();
        });
        window.render_frame(cx);
        assert_eq!(
            window
                .find(("animation-motion-group-header", 0usize))
                .expanded(),
            Some(false)
        );
        assert!(window.try_find(("animation-motion", 0usize)).is_none());
        assert!(window.find(("animation-motion", 2usize)).visible());
        assert!(window.find(("animation-expression", 0usize)).visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn properties_follow_registration_and_expression_without_authoring_edits(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        harness.view.update(cx, |view, cx| {
            view.dock
                .as_ref()
                .unwrap()
                .show(editor_core::PanelKind::Properties, None, window, cx);
        });
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        reveal_property(window, "property-group-$details", cx);
        window.click("property-group-$details", cx);
        window.render_frame(cx);
        assert_eq!(window.find("property-title-name").label(), Some("Once"));
        reveal_property(window, "animation-property-registration", cx);
        assert_eq!(
            window.find("animation-property-registration").label(),
            Some("Idle [1]")
        );

        window.click(("animation-motion", 1usize), cx);
        window.render_frame(cx);
        reveal_property(window, "animation-property-registration", cx);
        assert_eq!(
            window.find("animation-property-registration").label(),
            Some("Idle [2]")
        );

        super::animation::click_expression(window, 0, cx);
        window.render_frame(cx);
        reveal_property(window, "animation-property-expression", cx);
        assert_eq!(
            window.find("animation-property-expression").label(),
            Some("Add")
        );
        reveal_property(window, ("animation-property-expression-entry", 0usize), cx);
        assert_eq!(
            window
                .find(("animation-property-expression-entry", 0usize))
                .label(),
            Some("Add · 0.3000")
        );
        window.click("animation-clear-expression", cx);
        window.render_frame(cx);
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .animation()
                .unwrap()
                .expression
                .is_none()
        );
        let document = harness.view.read(cx).editor.document().unwrap();
        assert_eq!(document.history_lengths(), (0, 0));
        assert!(!document.modified());
    })
    .unwrap();
}

#[gpui_kit::test]
fn physics_and_pose_inspect_presented_values_and_clear_on_document_switch(cx: &mut TestAppContext) {
    use editor_core::PanelKind;
    let (document, assets) = document_fixture::combined_animation_document().unwrap();
    let catalog = document.animation_catalog();
    let input = catalog
        .parameters
        .iter()
        .find(|p| p.runtime_id == "ParamX")
        .unwrap()
        .id
        .clone();
    let output = catalog
        .parameters
        .iter()
        .find(|p| p.runtime_id == "ParamY")
        .unwrap()
        .id
        .clone();
    let kasane_core::document::PosePartRef::Resolved { part_id } =
        &document.pose_definition().unwrap().groups[0][0].part
    else {
        panic!("fixture must bind pose")
    };
    let part_id = part_id.clone();
    let harness = setup_with_document(cx, document, assets);
    let original = harness
        .view
        .read_with(cx, |view, _| view.editor.active_id().unwrap());
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        harness.view.update(cx, |view, cx| {
            view.dock
                .as_ref()
                .unwrap()
                .show(editor_core::PanelKind::Properties, None, window, cx);
        });
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        let (input_value, output_value, opacity) = harness.view.update(cx, |view, cx| {
            view.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::AnimationPhysics, None, window, cx);
            view.editor.seek_animation(original, 0.25).unwrap();
            let request = view.editor.animation_request().unwrap();
            let frame = AnimationEvaluator::default()
                .evaluate(&request, |_, _| true)
                .unwrap();
            let values = (
                frame.snapshot.parameters[&input],
                frame.snapshot.parameters[&output],
                frame.snapshot.part_opacities[&part_id],
            );
            assert!(
                view.editor
                    .present_animation(frame.identity, frame.snapshot)
            );
            view.notify_animation(cx);
            values
        });
        window.render_frame(cx);
        assert_eq!(
            window.find("animation-physics-fps").label(),
            Some("30.0 fps")
        );
        assert_eq!(
            window.find("animation-physics-input-0-0").label(),
            Some(format!("{input_value:.4}").as_str())
        );
        assert_eq!(
            window.find("animation-physics-output-0-0").label(),
            Some(format!("{output_value:.4}").as_str())
        );
        harness.view.update(cx, |view, cx| {
            view.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::AnimationPose, None, window, cx);
            view.run_command(CommandId::PlayPause, window, cx);
            view.editor.advance_animation_clock(0.5);
            view.notify_animation(cx);
        });
        window.render_frame(cx);
        assert_eq!(window.find("animation-pose-fade").label(), Some("0.500 s"));
        assert_eq!(
            window.find("animation-pose-part-0-0-0").label(),
            Some(format!("{opacity:.4}").as_str()),
            "must retain presented frame while next frame is pending"
        );
        assert!(!harness.view.read(cx).editor.document().unwrap().modified());
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor.new_document().unwrap();
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.try_find("animation-pose-empty").is_some());
        assert!(window.try_find("animation-pose-part-0-0-0").is_none());
        harness.view.update(cx, |view, cx| {
            view.dock
                .as_ref()
                .unwrap()
                .show(PanelKind::AnimationPhysics, None, window, cx);
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.try_find("animation-physics-empty").is_some());
        assert!(window.try_find("animation-physics-input-0-0").is_none());
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .active_inspection()
                .unwrap()
                .object()
                .is_none()
        );
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor.activate(original).unwrap();
            cx.notify();
        });
        window.render_frame(cx);
        assert!(window.try_find("animation-physics-empty").is_none());
        assert!(window.try_find("animation-physics-input-0-0").is_some());
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .animation()
                .unwrap()
                .motion
                .as_ref()
                .unwrap()
                .clip_id,
            harness
                .view
                .read(cx)
                .editor
                .active_inspection()
                .unwrap()
                .object()
                .unwrap()
                .id
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn inspector_commands_restore_separate_workspace_layouts(cx: &mut TestAppContext) {
    use editor_core::PanelKind;
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::Physics, window, cx);
            view.run_command(CommandId::Pose, window, cx);
            let dock = view.dock.as_ref().unwrap();
            for kind in [
                PanelKind::Properties,
                PanelKind::AnimationPhysics,
                PanelKind::AnimationPose,
            ] {
                assert!(dock.contains(kind, cx));
            }
            for kind in [PanelKind::Physics, PanelKind::Pose] {
                assert!(!dock.contains(kind, cx));
            }
            view.run_command(CommandId::Properties, window, cx);
            assert!(
                !view
                    .dock
                    .as_ref()
                    .unwrap()
                    .contains(PanelKind::Properties, cx)
            );
        });
        window.render_frame(cx);
        window.click("workspace-modeling", cx);
        window.render_frame(cx);
        let view = harness.view.read(cx);
        let dock = view.dock.as_ref().unwrap();
        assert!(dock.contains(PanelKind::Properties, cx));
        assert!(!dock.contains(PanelKind::Physics, cx));
        assert!(!dock.contains(PanelKind::AnimationPhysics, cx));
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        harness.view.update(cx, |view, cx| {
            let dock = view.dock.as_ref().unwrap();
            assert!(dock.contains(PanelKind::AnimationPhysics, cx));
            assert!(dock.contains(PanelKind::AnimationPose, cx));
            assert!(!dock.contains(PanelKind::Properties, cx));
            view.run_command(CommandId::ResetLayout, window, cx);
            let dock = view.dock.as_ref().unwrap();
            assert!(dock.contains(PanelKind::Properties, cx));
            assert!(!dock.contains(PanelKind::AnimationPhysics, cx));
            assert!(!dock.contains(PanelKind::AnimationPose, cx));
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn paused_expression_clicks_show_only_the_latest_expression_as_active(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    let mut evaluator = AnimationEvaluator::default();
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        for selected in [0usize, 2, 1, 2] {
            super::animation::click_expression(window, selected, cx);
            harness.view.update(cx, |view, cx| {
                view.editor.invalidate_animation();
                let frame = evaluator
                    .evaluate(&view.editor.animation_request().unwrap(), |_, _| true)
                    .unwrap();
                assert!(
                    view.editor
                        .present_animation(frame.identity, frame.snapshot)
                );
                view.notify_animation(cx);
            });
            window.render_frame(cx);
            for index in 0..3usize {
                assert!(
                    window
                        .try_find(("animation-expression-status", index))
                        .is_none()
                );
                assert_eq!(
                    window
                        .try_find(("animation-expression-active", index))
                        .is_some(),
                    index == selected
                );
            }
            let view = harness.view.read(cx);
            let animation = view.editor.animation().unwrap();
            assert!(!animation.playing);
            assert_eq!(animation.time, 0.);
            assert_eq!(animation.expressions.len(), 1);
            assert_eq!(
                view.displayed_animation()
                    .unwrap()
                    .active_expressions
                    .as_slice(),
                std::slice::from_ref(animation.expression.as_ref().unwrap())
            );
            if selected == 2 {
                let parameter = &view.editor.document().unwrap().parameters()[0].id;
                assert!(
                    (view.displayed_animation().unwrap().parameters[parameter] - 0.8).abs() < 1e-6
                );
            }
        }
        window.click("animation-clear-expression", cx);
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .animation()
                .unwrap()
                .expressions
                .is_empty()
        );
        assert!(!harness.view.read(cx).editor.document().unwrap().modified());
    })
    .unwrap();
}
