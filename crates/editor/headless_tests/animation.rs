use super::{document_fixture, setup_with_document};
use editor_core::{CommandId, WorkspaceKind};
use gpui_kit::component::{WindowExt, dock::PanelId};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, InputEvent as _, KeyDownEvent, KeyUpEvent, Keystroke, TestAppContext,
};

#[gpui_kit::test]
fn immediate_replay_after_natural_end_starts_with_a_fresh_clock(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::PlayPause, window, cx);
            // The transport can reach its end before the timer's next wake or
            // the final GPU result clears the old wall-clock anchor.
            view.animation_anchor = Some((
                std::time::Instant::now() - std::time::Duration::from_secs(2),
                0.5,
            ));
            view.editor.advance_animation_clock(2.);
            assert!(!view.editor.animation().unwrap().playing);
            view.run_command(CommandId::PlayPause, window, cx);
            assert!(view.editor.animation().unwrap().playing);
            assert_eq!(view.editor.animation().unwrap().time, 0.);
            assert_eq!(view.animation_anchor.unwrap().1, 0.);
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn motion_selection_space_button_and_menu_share_transport_and_repeat_guard(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        let focus = harness
            .view
            .read(cx)
            .dock
            .as_ref()
            .unwrap()
            .canvas
            .read(cx)
            .focus
            .clone();
        window.focus(&focus, cx);
        for held in [false, true, true] {
            window.dispatch_event(
                KeyDownEvent {
                    keystroke: Keystroke::parse("space").unwrap(),
                    is_held: held,
                    prefer_character_input: false,
                }
                .to_platform_input(),
                cx,
            );
        }
        window.dispatch_event(
            KeyUpEvent {
                keystroke: Keystroke::parse("space").unwrap(),
            }
            .to_platform_input(),
            cx,
        );
        assert!(harness.view.read(cx).editor.animation().unwrap().playing);
        window.render_frame(cx);
        window.click("animation-play-pause", cx);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::PlayPause, window, cx)
        });
        assert!(harness.view.read(cx).editor.animation().unwrap().playing);
        window.render_frame(cx);
        // Selecting a second registration of the same clip is a new recipe.
        window.click(("animation-motion", 1usize), cx);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        let entry = &harness
            .view
            .read(cx)
            .editor
            .animation()
            .unwrap()
            .motion
            .as_ref()
            .unwrap()
            .entry;
        assert_eq!(entry.as_ref().unwrap().1, 1);
        window.click("animation-play-pause", cx);
        window.render_frame(cx);
        window.click("animation-rewind", cx);
        assert_eq!(harness.view.read(cx).editor.animation().unwrap().time, 0.);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        assert!(!harness.view.read(cx).editor.document().unwrap().modified());
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
fn navigation_keeps_playing_modal_and_workspace_switch_pause_and_panels_reopen(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 2usize), cx);
        window.render_frame(cx);
        window.click("animation-play-pause", cx);
        harness.view.update(cx, |view, cx| {
            view.editor
                .pan_by(editor_core::CanvasPoint { x: 12., y: 5. });
            view.redraw_canvas(cx);
        });
        assert!(harness.view.read(cx).editor.animation().unwrap().playing);
        window.open_dialog(cx, |dialog, _, _| dialog.title("Modal"));
        window.render_frame(cx);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        window.press("space", cx);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        window.close_dialog(cx);
        window.render_frame(cx);
        window.click("animation-play-pause", cx);
        window.click("workspace-modeling", cx);
        window.render_frame(cx);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        assert_eq!(
            harness.view.read(cx).editor.workspace(),
            WorkspaceKind::Modeling
        );
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        let info = PanelId::from(
            harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .animation_info
                .entity_id(),
        );
        for visible in [false, true] {
            harness.view.update(cx, |view, cx| {
                view.run_command(CommandId::AnimationInfo, window, cx)
            });
            window.render_frame(cx);
            let area = harness.view.read(cx).dock.as_ref().unwrap().area.read(cx);
            assert_eq!(area.panel(info).is_some(), visible);
            crate::docking::validate_layout(&area.dump(cx), WorkspaceKind::Animation).unwrap();
        }
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
    })
    .unwrap();
}

#[gpui_kit::test]
fn expression_controls_preserve_pause_clear_at_current_time_and_reset(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-expression", 0usize), cx);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        window.render_frame(cx);
        window.click("animation-play-pause", cx);
        harness
            .view
            .update(cx, |view, _| view.editor.advance_animation_clock(0.5));
        window.click(("animation-expression", 2usize), cx);
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert!(animation.playing);
        assert_eq!(animation.expressions[1].time, 0.5);
        window.render_frame(cx);
        window.click(("animation-expression", 2usize), cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .animation()
                .unwrap()
                .expressions
                .len(),
            2
        );
        window.render_frame(cx);
        window.click("animation-clear-expression", cx);
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert!(animation.playing);
        assert_eq!(animation.time, 0.5);
        assert!(animation.expression.is_none());
        assert!(animation.expressions.is_empty());
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.click(("animation-expression", 0usize), cx);
        window.render_frame(cx);
        window.click("animation-reset", cx);
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert!(!animation.playing);
        assert!(animation.motion.is_none());
        assert!(animation.expression.is_none());
        assert_eq!(animation.time, 0.);
        assert!(!harness.view.read(cx).editor.document().unwrap().modified());
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
fn ruler_press_pauses_immediately_and_scrubbing_preserves_nonloop_triggers(
    cx: &mut TestAppContext,
) {
    use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, point, px};
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.click(("animation-expression", 0usize), cx);
        window.render_frame(cx);
        window.click("animation-play-pause", cx);
        window.render_frame(cx);
        let track = window
            .within("animation-seek")
            .find("slider-bar-container")
            .bounds();
        // A thumb press pauses even before movement generates a SliderEvent.
        let start = point(track.left() + px(1.), track.center().y);
        window.dispatch_event(
            MouseMoveEvent {
                position: start,
                pressed_button: None,
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position: start,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        window.dispatch_event(
            MouseMoveEvent {
                position: point(track.left() + track.size.width * 0.75, track.center().y),
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        let track = window
            .within("animation-seek")
            .find("slider-bar-container")
            .bounds();
        let end = point(track.left() + track.size.width * 0.25, track.center().y);
        window.dispatch_event(
            MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert!(!animation.playing);
        assert!(
            (animation.time - 0.5).abs() < 0.01,
            "time={}",
            animation.time
        );
        assert_eq!(animation.expressions.len(), 1);
        assert_eq!(animation.expressions[0].time, 0.);
        assert!(!harness.view.read(cx).editor.document().unwrap().modified());
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
        window.render_frame(cx);
        window.open_dialog(cx, |dialog, _, _| dialog.title("Modal"));
        window.render_frame(cx);
        assert!(
            window
                .within("animation-seek")
                .try_find("slider-bar-container")
                .is_none()
        );
        window.close_dialog(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn ruler_loop_locates_first_cycle_and_expression_panel_reopens(cx: &mut TestAppContext) {
    use gpui_kit::point;
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 2usize), cx);
        window.click(("animation-expression", 0usize), cx);
        window.render_frame(cx);
        window.click("animation-play-pause", cx);
        harness
            .view
            .update(cx, |view, _| view.editor.advance_animation_clock(5.));
        window.click(("animation-expression", 1usize), cx);
        window.render_frame(cx);
        let mut ruler = window.within("animation-seek");
        let track = ruler.find("slider-bar-container").bounds();
        ruler.click_at(
            "slider-bar-container",
            point(track.size.width * 0.4, track.size.height / 2.),
            cx,
        );
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert!(!animation.playing);
        assert!((animation.time - 0.4).abs() < 0.01);
        assert_eq!(animation.expressions.len(), 1);
        assert_eq!(animation.expressions[0].time, 0.);
        let panel = PanelId::from(
            harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .animation_expressions
                .entity_id(),
        );
        for visible in [false, true] {
            harness.view.update(cx, |view, cx| {
                view.run_command(CommandId::Expressions, window, cx)
            });
            window.render_frame(cx);
            let area = harness.view.read(cx).dock.as_ref().unwrap().area.read(cx);
            assert_eq!(area.panel(panel).is_some(), visible);
            crate::docking::validate_layout(&area.dump(cx), WorkspaceKind::Animation).unwrap();
        }
    })
    .unwrap();
}

#[gpui_kit::test]
fn leaving_animation_cancels_ruler_drag_and_old_pointer_events(cx: &mut TestAppContext) {
    use gpui_kit::{MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, point};
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    let end = cx
        .update_window(harness.window, |_, window, cx| {
            window.click("workspace-animation", cx);
            window.render_frame(cx);
            window.click(("animation-motion", 0usize), cx);
            window.render_frame(cx);
            let track = window
                .within("animation-seek")
                .find("slider-bar-container")
                .bounds();
            let start = point(track.left() + track.size.width * 0.3, track.center().y);
            let end = point(track.left() + track.size.width * 0.8, track.center().y);
            window.dispatch_event(
                MouseMoveEvent {
                    position: start,
                    pressed_button: None,
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position: start,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseMoveEvent {
                    position: end,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            assert!(cx.has_active_drag());
            end
        })
        .unwrap();
    let time = harness
        .view
        .read_with(cx, |view, _| view.editor.animation().unwrap().time);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::ModelingWorkspace, window, cx)
        });
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        assert!(!cx.has_active_drag());
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationWorkspace, window, cx)
        });
        window.render_frame(cx);
        window.dispatch_event(
            MouseMoveEvent {
                position: end,
                pressed_button: Some(MouseButton::Left),
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseUpEvent {
                button: MouseButton::Left,
                position: end,
                modifiers: Default::default(),
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.animation().unwrap().time, time);
        assert!(!view.editor.animation().unwrap().playing);
    });
}

#[gpui_kit::test]
fn diagnostics_precede_playback_invalid_expressions_are_disabled_and_parameters_are_read_only(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::diagnostic_animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    let authoring = harness
        .view
        .read_with(cx, |view, _| view.editor.document().unwrap().parameters());
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        for index in 0..5usize {
            assert!(window.try_find(("animation-diagnostic", index)).is_some());
        }
        window.click(("animation-expression", 3usize), cx);
        harness.view.read_with(cx, |view, _| {
            assert!(view.editor.animation().unwrap().expression.is_none());
            assert!(view.edit_error.is_none());
        });
        window.click(("animation-motion", 0usize), cx);
        harness.view.update(cx, |view, cx| {
            view.editor
                .seek_animation(view.editor.active_id().unwrap(), 1.)
                .unwrap();
            let request = view.editor.animation_request().unwrap();
            let frame = editor_core::AnimationEvaluator::default()
                .evaluate(&request, |_, _| true)
                .unwrap();
            assert!(
                view.editor
                    .present_animation(frame.identity, frame.snapshot)
            );
            view.notify_animation(cx);
            view.run_command(CommandId::AnimationParameters, window, cx);
        });
        window.render_frame(cx);
        assert!(window.try_find("AnimationParameters").is_some());
        let row = window.find(("animation-parameter", 0usize));
        window.drag(
            row.bounds().center(),
            row.bounds().center() + gpui_kit::point(gpui_kit::px(40.), gpui_kit::px(0.)),
            cx,
        );
        harness.view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor
                    .animation()
                    .unwrap()
                    .presented
                    .as_ref()
                    .unwrap()
                    .time,
                1.
            );
            assert_ne!(
                view.editor
                    .animation()
                    .unwrap()
                    .presented
                    .as_ref()
                    .unwrap()
                    .parameters[&authoring[0].id],
                authoring[0].value
            );
            assert_eq!(view.editor.document().unwrap().parameters(), authoring);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
            assert!(!view.editor.document().unwrap().modified());
        });
        // Closing the opt-in panel does not affect playback or the document.
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::AnimationParameters, window, cx)
        });
        window.render_frame(cx);
        assert!(window.try_find("AnimationParameters").is_none());
        assert!(
            harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .area
                .read(cx)
                .layout(gpui_kit::component::dock::DockPlacement::Right)
                .is_none()
        );
        window.click("workspace-modeling", cx);
        window.render_frame(cx);
        assert!(window.try_find("AnimationParameters").is_none());
        assert!(window.try_find("animation-playback-bar").is_none());
    })
    .unwrap();
}
