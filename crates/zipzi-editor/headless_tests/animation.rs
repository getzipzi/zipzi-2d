use super::{document_fixture, setup_with_document};
use gpui_kit::component::{WindowExt, dock::PanelId};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, InputEvent as _, KeyDownEvent, KeyUpEvent, Keystroke, TestAppContext,
};
use zipzi_editor_core::{CommandId, WorkspaceKind};

#[gpui_kit::test]
fn hiding_animation_during_evaluation_reissues_content_when_shown(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    harness.view.update(cx, |view, _| {
        view.editor.set_workspace(WorkspaceKind::Animation).unwrap();
        let viewport = view.viewport_slots[1];
        view.editor.set_viewport_visible(viewport, true);
        view.editor.set_viewport_metrics(
            viewport,
            zipzi_editor_core::ViewportMetrics {
                width: 512.,
                height: 512.,
                scale_factor: 1.,
            },
        );
        let motion = view.editor.document().unwrap().motion_resources()[0]
            .selection
            .clone();
        view.editor
            .select_motion(view.editor.active_id().unwrap(), motion)
            .unwrap();
        view.editor
            .scrub_timeline(view.editor.active_id().unwrap(), 0.5)
            .unwrap();
        let request = view.editor.animation_request().unwrap();
        let old = request.identity;
        // Hold the result receiver so this models hiding before CPU completion
        // is delivered, regardless of how fast the worker runs.
        let (mut worker, _results) = crate::animation::AnimationWorker::start();
        worker.send(request);
        view.animation_worker = Some(worker);
        view.animation_content = None;
        view.editor.set_viewport_visible(viewport, false);
        view.request_preview_for_input(0);
        view.editor.set_viewport_visible(viewport, true);
        let replacement = view
            .editor
            .animation_request()
            .expect("cancelled content must be retried");
        assert!(replacement.identity.request > old.request);
        assert!(!view.editor.accepts_animation(old));
        assert!(view.editor.accepts_animation(replacement.identity));
        // Repeated hidden redraws do not keep advancing the cancellation epoch.
        view.editor.set_viewport_visible(viewport, false);
        view.request_preview_for_input(0);
        view.editor.set_viewport_visible(viewport, true);
        assert!(view.editor.animation_request().is_none());
        assert!(view.editor.accepts_animation(replacement.identity));
        // Hiding a viewport must not end a scrub owned by Timeline. Its next
        // sample stays in the same acceptance epoch, allowing intermediate frames.
        view.editor
            .scrub_timeline(view.editor.active_id().unwrap(), 0.6)
            .unwrap();
        let next = view.editor.animation_request().unwrap();
        assert_eq!(
            next.identity.acceptance_floor,
            replacement.identity.acceptance_floor
        );
    });
}

#[gpui_kit::test]
fn preview_modes_and_preview_loop_are_session_controls(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .animation()
                .unwrap()
                .preview_mode,
            zipzi_editor_core::AnimationPreviewMode::Result
        );
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .animation()
                .unwrap()
                .preview_loop
        );
        assert!(
            window
                .try_find("animation-expression-preview-result")
                .is_none()
        );
        window.click("animation-preview-mode", cx);
        window.render_frame(cx);
        window.click("animation-result-preview", cx);
        window.render_frame(cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .animation()
                .unwrap()
                .preview_mode,
            zipzi_editor_core::AnimationPreviewMode::Result
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.click("animation-preview-mode", cx);
        window.render_frame(cx);
        window.click("animation-edit-preview", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.click("animation-preview-loop", cx);
        window.render_frame(cx);
        let view = harness.view.read(cx);
        assert!(!view.editor.animation().unwrap().preview_loop);
        assert!(!view.editor.document().unwrap().modified());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    })
    .unwrap();
}

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
            .active_viewport()
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
        assert_eq!(
            entry,
            &harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .motion_resources()[1]
                .selection
                .entry
        );
        window.click("animation-play-pause", cx);
        window.render_frame(cx);
        window.click("timeline-start", cx);
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
                .pan_by(zipzi_editor_core::ViewportPoint { x: 12., y: 5. });
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
                .properties
                .entity_id(),
        );
        for visible in [false, true] {
            harness.view.update(cx, |view, cx| {
                view.run_command(CommandId::Properties, window, cx)
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
        super::animation::click_expression(window, 0, cx);
        assert!(!harness.view.read(cx).editor.animation().unwrap().playing);
        window.render_frame(cx);
        window.click("animation-play-pause", cx);
        harness
            .view
            .update(cx, |view, _| view.editor.advance_animation_clock(0.5));
        super::animation::click_expression(window, 2, cx);
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert!(animation.playing);
        assert_eq!(animation.expressions[1].time, 0.5);
        window.render_frame(cx);
        super::animation::click_expression(window, 2, cx);
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
        super::animation::click_expression(window, 0, cx);
        window.render_frame(cx);
        window.click("animation-preview-mode", cx);
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
        super::animation::click_expression(window, 0, cx);
        window.render_frame(cx);
        window.click("animation-play-pause", cx);
        window.render_frame(cx);
        let track = window
            .within("animation-seek")
            .find("timeline-ruler")
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
            .find("timeline-ruler")
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
            (animation.edit_time - 0.5).abs() < 0.01,
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
        let before = harness.view.read(cx).editor.animation().unwrap().edit_time;
        let ruler = window
            .within("animation-seek")
            .find("timeline-ruler")
            .bounds();
        window.within("animation-seek").click_at(
            "timeline-ruler",
            point(ruler.size.width * 0.8, ruler.size.height / 2.),
            cx,
        );
        assert_eq!(
            harness.view.read(cx).editor.animation().unwrap().edit_time,
            before
        );
        window.close_dialog(cx);
    })
    .unwrap();
}

#[gpui_kit::test]
fn ruler_loop_keeps_expression_schedule_and_expression_panel_reopens(cx: &mut TestAppContext) {
    use gpui_kit::point;
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 2usize), cx);
        super::animation::click_expression(window, 0, cx);
        window.render_frame(cx);
        window.click("animation-play-pause", cx);
        harness
            .view
            .update(cx, |view, _| view.editor.advance_animation_clock(5.));
        super::animation::click_expression(window, 1, cx);
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
        let mut ruler = window.within("animation-seek");
        let track = ruler.find("timeline-ruler").bounds();
        ruler.click_at(
            "timeline-ruler",
            point(track.size.width * 0.4, track.size.height / 2.),
            cx,
        );
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        let animation = harness.view.read(cx).editor.animation().unwrap();
        assert!(!animation.playing);
        assert!((animation.edit_time - 0.4).abs() < 0.01);
        assert_eq!(animation.expressions.len(), 2);
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
                .find("timeline-ruler")
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
            assert!(
                harness
                    .view
                    .read(cx)
                    .dock
                    .as_ref()
                    .unwrap()
                    .timeline
                    .read(cx)
                    .pointer_active()
            );
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
        assert!(
            !harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .timeline
                .read(cx)
                .pointer_active()
        );
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
fn diagnostics_precede_playback_incomplete_expressions_are_playable_without_authoring_edits(
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
        assert!(
            window
                .try_find("animation-preview-details-toggle")
                .is_none()
        );
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .animation_catalog()
                .diagnostics_for(None)
                .len(),
            5
        );
        assert!(
            window
                .find(("animation-expression-warning", 3usize))
                .visible()
        );
        super::animation::click_expression(window, 3, cx);
        harness.view.update(cx, |view, _| {
            let id = &view
                .editor
                .document()
                .unwrap()
                .animation_catalog()
                .expressions[3]
                .id;
            assert_eq!(
                view.editor.animation().unwrap().expression.as_ref(),
                Some(id)
            );
            assert!(view.edit_error.is_none());
        });
        window.click(("animation-motion", 0usize), cx);
        harness.view.update(cx, |view, cx| {
            view.editor
                .seek_animation(view.editor.active_id().unwrap(), 1.)
                .unwrap();
            let request = view.editor.animation_request().unwrap();
            let frame = zipzi_editor_core::AnimationEvaluator::default()
                .evaluate(&request, |_, _| true)
                .unwrap();
            assert!(
                view.editor
                    .present_animation(frame.identity, frame.snapshot)
            );
            view.notify_animation(cx);
        });
        window.render_frame(cx);
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
        window.click("workspace-modeling", cx);
        window.render_frame(cx);
        assert!(window.try_find("animation-timeline").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn display_callbacks_sample_fractional_time_and_stop_after_pause(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 2usize), cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::PlayPause, window, cx);
            view.animation_anchor = Some((
                std::time::Instant::now() - std::time::Duration::from_micros(8300),
                0.,
            ));
            view.schedule_animation_frame(window, cx);
            assert!(view.animation_clock_running);
        });
        window.simulate_next_frame(cx);
        let first = harness.view.read(cx).editor.animation().unwrap().time;
        assert!(first > 0.);
        assert_ne!(
            first,
            (first * 60.).floor() / 60.,
            "display time was quantized to 60 Hz"
        );
        assert!(harness.view.read(cx).animation_clock_running);
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::PlayPause, window, cx);
        });
        window.simulate_next_frame(cx);
        let view = harness.view.read(cx);
        assert_eq!(view.editor.animation().unwrap().time, first);
        assert!(!view.animation_clock_running);
        assert!(view.animation_anchor.is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn playback_speed_select_changes_clock_without_retiming_motion(cx: &mut TestAppContext) {
    use std::time::{Duration, Instant};
    use zipzi_editor_core::AnimationPreviewMode;

    let (document, assets) = document_fixture::animation_document().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        window.click("timeline-playback-speed", cx);
        window.render_frame(cx);
        for _ in 0..3 {
            window.press("down", cx);
        }
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.animation().unwrap().playback_speed, 2.);
        assert!(!view.editor.animation().unwrap().playing);
        assert_eq!(view.editor.animation().unwrap().time, 0.);
        assert!(view.editor.timeline_edit().is_none());
    });
    let original = harness.view.read_with(cx, |view, _| {
        view.editor
            .document()
            .unwrap()
            .motion_definition(
                &view
                    .editor
                    .animation()
                    .unwrap()
                    .motion
                    .as_ref()
                    .unwrap()
                    .clip_id,
            )
            .unwrap()
            .clone()
    });
    for mode in [AnimationPreviewMode::Editing, AnimationPreviewMode::Result] {
        cx.update_window(harness.window, |_, window, cx| {
            let anchor = Instant::now() - Duration::from_millis(100);
            harness.view.update(cx, |view, cx| {
                let document = view.editor.active_id().unwrap();
                view.editor
                    .set_animation_preview_mode(document, mode)
                    .unwrap();
                view.editor.seek_animation(document, 0.5).unwrap();
                view.run_command(CommandId::PlayPause, window, cx);
                view.animation_anchor = Some((anchor, 0.5));
                view.schedule_animation_frame(window, cx);
            });
            let before = anchor.elapsed().as_secs_f32();
            window.simulate_next_frame(cx);
            let after = anchor.elapsed().as_secs_f32();
            let time = harness.view.read(cx).editor.animation().unwrap().time;
            assert!(time >= 0.5 + before * 2. - 0.00001, "{mode:?}: {time}");
            assert!(time <= 0.5 + after * 2. + 0.00001, "{mode:?}: {time}");
        })
        .unwrap();
    }
    let time = harness
        .view
        .read_with(cx, |view, _| view.editor.animation().unwrap().time);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("timeline-playback-speed", cx);
        window.render_frame(cx);
        window.press("down", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        let animation = view.editor.animation().unwrap();
        assert_eq!(animation.playback_speed, 4.);
        assert!(animation.playing);
        assert!(animation.time >= time);
        let document = view.editor.document().unwrap();
        assert_eq!(
            document
                .motion_definition(&animation.motion.as_ref().unwrap().clip_id)
                .unwrap(),
            &original
        );
        assert_eq!(document.history_lengths(), (0, 0));
        assert!(!document.modified());
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-modeling", cx);
        window.render_frame(cx);
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .animation()
                .unwrap()
                .playback_speed,
            4.
        );
        assert!(window.find("timeline-playback-speed").visible());
    })
    .unwrap();
}

// Timeline reduces the resource pane's height; center a resource before clicking.
pub(super) fn click_expression(
    window: &mut gpui_kit::Window,
    index: usize,
    cx: &mut gpui_kit::App,
) {
    window.render_frame(cx);
    let row = window.find(("animation-expression", index)).bounds();
    let viewport = window.find("animation-expression-list").bounds();
    window.scroll(
        "animation-expression-list",
        gpui_kit::ScrollDelta::Pixels(gpui_kit::point(
            gpui_kit::px(0.),
            viewport.center().y - row.center().y,
        )),
        cx,
    );
    window.click(("animation-expression", index), cx);
}
