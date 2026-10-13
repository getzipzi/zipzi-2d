use super::{document_fixture, setup_with_document};
use editor_core::{AnimationEvaluator, CommandId};
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};

#[gpui_kit::test]
fn inline_play_pause_restart_and_keyboard_browsing_preserve_transport_contract(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion-play", 0usize), cx);
        assert!(harness.view.read(cx).editor.animation().unwrap().playing);
        harness
            .view
            .update(cx, |view, _| view.editor.advance_animation_clock(0.75));
        window.render_frame(cx);
        window.click(("animation-motion-play", 0usize), cx);
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert!(!animation.playing);
        assert_eq!(
            animation.time, 0.75,
            "nested button must not bubble into row selection"
        );
        window.render_frame(cx);
        window.click(("animation-motion-play", 0usize), cx);
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
fn motion_search_reveals_collapsed_groups_without_changing_selection(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
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
        window.click("animation-motion-search", cx);
        window.input("iDlE", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find(("animation-motion", 0usize)).visible());
        assert!(window.find(("animation-motion", 1usize)).visible());
        assert!(window.try_find(("animation-motion", 2usize)).is_none());
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert_eq!(
            animation.motion.as_ref().unwrap().entry.as_ref().unwrap().1,
            0
        );
        assert!(!animation.playing);
        window.press("space", cx);
        assert!(
            !harness.view.read(cx).editor.animation().unwrap().playing,
            "typing must not play"
        );
        window.click("animation-motion-search-clear", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find(("animation-motion", 0usize)).is_none(),
            "search must preserve collapsed groups"
        );
        assert!(window.find(("animation-motion", 2usize)).visible());
        window.click(("animation-motion-group", 0usize), cx);
        window.render_frame(cx);
        assert!(window.find(("animation-motion", 0usize)).visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn expression_feedback_uses_presented_frames_and_clear_survives_filtering(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-expression", 0usize), cx);
        window.render_frame(cx);
        assert_eq!(
            window.find(("animation-expression-status", 0usize)).label(),
            Some("Selected")
        );
        assert_eq!(
            window.find("animation-expression-feedback").label(),
            Some("Paused · Play to view the expression transition.")
        );
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
        assert_eq!(
            window.find(("animation-expression-status", 0usize)).label(),
            Some("Active in frame")
        );
        window.click("animation-expression-search", cx);
        window.input("no results", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(("animation-expression", 0usize)).is_none());
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
fn browsing_state_is_independent_and_restored_per_document(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    let original = harness
        .view
        .read_with(cx, |view, _| view.editor.active_id().unwrap());
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion-group", 0usize), cx);
        window.click("animation-motion-search", cx);
        window.input("idle", cx);
        window.click("animation-expression-search", cx);
        window.input("add", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor.new_document().unwrap();
            cx.notify();
        });
        window.render_frame(cx);
        assert_eq!(window.find("animation-motion-search").value(), Some(""));
        assert_eq!(window.find("animation-expression-search").value(), Some(""));
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor.activate(original).unwrap();
            cx.notify();
        });
        window.render_frame(cx);
        assert_eq!(window.find("animation-motion-search").value(), Some("idle"));
        assert_eq!(
            window.find("animation-expression-search").value(),
            Some("add")
        );
        window.click("animation-motion-search-clear", cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(("animation-motion", 0usize)).is_none());
        assert_eq!(
            window.find("animation-expression-search").value(),
            Some("add")
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn properties_follow_registration_and_expression_without_authoring_edits(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("animation-property-motion").label(),
            Some("Once")
        );
        assert_eq!(
            window.find("animation-property-registration").label(),
            Some("Idle [1]")
        );
        assert_eq!(
            window.find("animation-property-fades").label(),
            Some("Fade in 0.000 s / out 0.000 s")
        );
        window.click(("animation-motion", 1usize), cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("animation-property-registration").label(),
            Some("Idle [2]")
        );
        assert_eq!(
            window.find("animation-property-fades").label(),
            Some("Fade in 1.000 s / out 0.000 s")
        );
        window.click(("animation-expression", 0usize), cx);
        window.render_frame(cx);
        assert_eq!(
            window.find("animation-property-expression").label(),
            Some("Add")
        );
        assert_eq!(
            window
                .find(("animation-property-expression-entry", 0usize))
                .label(),
            Some("Add · 0.3000")
        );
        window.click("animation-clear-expression", cx);
        window.render_frame(cx);
        assert!(window.try_find("animation-property-expression").is_none());
        assert!(
            window
                .try_find("animation-property-no-expression")
                .is_some()
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
        assert!(window.try_find("animation-property-no-motion").is_some());
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
            window.find("animation-property-motion").label(),
            Some("Once")
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
                PanelKind::AnimationProperties,
                PanelKind::AnimationPhysics,
                PanelKind::AnimationPose,
            ] {
                assert!(dock.contains(kind, cx));
            }
            for kind in [PanelKind::Properties, PanelKind::Physics, PanelKind::Pose] {
                assert!(!dock.contains(kind, cx));
            }
            view.run_command(CommandId::Properties, window, cx);
            assert!(
                !view
                    .dock
                    .as_ref()
                    .unwrap()
                    .contains(PanelKind::AnimationProperties, cx)
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
            assert!(!dock.contains(PanelKind::AnimationProperties, cx));
            view.run_command(CommandId::ResetLayout, window, cx);
            let dock = view.dock.as_ref().unwrap();
            assert!(dock.contains(PanelKind::AnimationProperties, cx));
            assert!(dock.contains(PanelKind::AnimationInfo, cx));
            assert!(!dock.contains(PanelKind::AnimationPhysics, cx));
            assert!(!dock.contains(PanelKind::AnimationPose, cx));
        });
    })
    .unwrap();
}
