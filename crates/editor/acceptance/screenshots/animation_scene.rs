use super::super::{fixture, harness::Harness};
use super::common::{click_expression, save};
use crate::{i18n, theme};
use editor_core::Editor;
use gpui_kit::{point, px, test::TestWindowExt};
use std::{path::PathBuf, time::Duration};

pub(crate) fn animation_replacement_keeps_canvas(harness: &mut Harness) -> anyhow::Result<()> {
    let (document, assets) = fixture::animation_document()?;
    harness.cx.update_window(harness.window, |_, _, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor = Editor::from_document(document);
            view.editor
                .set_workspace(editor_core::WorkspaceKind::Animation)
                .unwrap();
            let id = view.editor.active_id().unwrap();
            let motion = view.editor.document().unwrap().animation_catalog().motions[2]
                .selection
                .clone();
            view.editor.select_motion(id, motion).unwrap();
            view.editor.seek_animation(id, 0.5).unwrap();
            view.redraw(cx);
        });
    })?;
    harness._assets = assets;
    let mut before = harness.capture()?;
    for change in 0..6 {
        harness.cx.update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.editor.toggle_animation().unwrap();
                let id = view.editor.active_id().unwrap();
                let catalog = view.editor.document().unwrap().animation_catalog();
                match change {
                    0 | 1 => view
                        .editor
                        .select_expression(id, catalog.expressions[change * 2].id.clone())
                        .unwrap(),
                    2 | 3 => view
                        .editor
                        .select_motion(id, catalog.motions[(change - 2) * 2].selection.clone())
                        .unwrap(),
                    4 => view.editor.clear_expression().unwrap(),
                    _ => view.editor.reset_animation().unwrap(),
                }
                // Delay submitting the replacement to deterministically capture the gap
                // between changing the recipe and receiving the next Metal image.
                view.notify_animation(cx);
                cx.notify();
            });
            window.render_frame(cx);
        })?;
        let waiting = harness.cx.capture_screenshot(harness.window)?;
        anyhow::ensure!(
            harness.canvas_image(&before)? == harness.canvas_image(&waiting)?,
            "Animation change {change} cleared the canvas while waiting for a replacement"
        );
        let previous = harness.view.read_with(&harness.cx, |view, _| {
            let frame = view.animation_frame().unwrap();
            assert!(std::ptr::eq(
                view.displayed_animation().unwrap(),
                frame.state.snapshot.as_ref()
            ));
            frame.state.identity.request
        });
        harness.cx.update_window(harness.window, |_, _, cx| {
            harness.view.update(cx, |view, cx| {
                view.editor.pause_animation();
                view.redraw(cx);
            });
        })?;
        before = harness.capture()?;
        harness.view.read_with(&harness.cx, |view, _| {
            assert!(
                view.animation_frame()
                    .as_ref()
                    .unwrap()
                    .state
                    .identity
                    .request
                    > previous
            );
        });
    }
    // Saving moves the session out of core and clears live viewport bindings.
    // The frozen image must still be painted throughout that ownership gap.
    let path = harness._assets.path().join("animation-save");
    let save = harness.cx.update_window(harness.window, |_, window, cx| {
        let save = harness.view.update(cx, |view, cx| {
            let id = view.editor.active_id().unwrap();
            let save = view.editor.prepare_save(id, &path, false).unwrap();
            cx.notify();
            save
        });
        window.render_frame(cx);
        save
    })?;
    let saving = harness.cx.capture_screenshot(harness.window)?;
    let center = |image: image::RgbaImage| {
        image::imageops::crop_imm(
            &image,
            image.width() / 4,
            image.height() / 4,
            image.width() / 2,
            image.height() / 2,
        )
        .to_image()
    };
    anyhow::ensure!(
        center(harness.canvas_image(&before)?) == center(harness.canvas_image(&saving)?),
        "Saving cleared the frozen Animation viewport image"
    );
    let mut completion = save.run();
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor.finish_save(&mut completion).unwrap();
            view.refresh_document(window, cx);
        });
    })?;
    Ok(())
}

pub(crate) fn animation_scrubbing_presents_frames_before_release() -> anyhow::Result<()> {
    use gpui_kit::{InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent};
    let mut harness = Harness::new()?;
    let (document, assets) = fixture::animation_document()?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor = Editor::from_document(document);
            view.redraw(cx);
        });
        window.render_frame(cx);
        window.click("workspace-animation", cx);
    })?;
    harness._assets = assets;
    for motion in [0usize, 2] {
        harness.cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            window.click(("animation-motion", motion), cx);
        })?;
        let before = harness.capture()?;
        let before = harness.canvas_image(&before)?;
        let track = harness.cx.update_window(harness.window, |_, window, _| {
            window
                .within("animation-seek")
                .find("timeline-ruler")
                .bounds()
        })?;
        let mut pointer = point(track.left() + track.size.width * 0.15, track.center().y);
        harness.cx.update_window(harness.window, |_, window, cx| {
            window.dispatch_event(
                MouseMoveEvent {
                    position: pointer,
                    pressed_button: None,
                    modifiers: Default::default(),
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position: pointer,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
        })?;
        let mut shown_times = Vec::new();
        let mut pixels_changed = false;
        for sample in 0..40 {
            // Move forward, then backward, without giving the worker an idle pause
            // between targets. GPU completions may lag behind several pointer events.
            let progress = if sample < 20 { sample } else { 39 - sample } as f32 / 20.;
            pointer.x = track.left() + track.size.width * (0.15 + 0.7 * progress);
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.dispatch_event(
                    MouseMoveEvent {
                        position: pointer,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Default::default(),
                    }
                    .to_platform_input(),
                    cx,
                );
                window.render_frame(cx);
            })?;
            std::thread::sleep(Duration::from_millis(8));
            harness.cx.run_until_parked();
            let time = harness.view.read_with(&harness.cx, |view, _| {
                view.displayed_animation().unwrap().time
            });
            if shown_times.last() != Some(&time) {
                shown_times.push(time);
                let image = harness.cx.capture_screenshot(harness.window)?;
                pixels_changed |= harness.canvas_image(&image)? != before;
            }
        }
        anyhow::ensure!(
            shown_times.len() > 2 && pixels_changed,
            "Scrubbing motion {motion} must display changing Metal frames before release: {shown_times:?}"
        );
        anyhow::ensure!(
            shown_times.windows(2).any(|pair| pair[1] < pair[0]),
            "Backward dragging must display earlier frames before release: {shown_times:?}"
        );
        harness.cx.update_window(harness.window, |_, window, cx| {
            window.dispatch_event(
                MouseUpEvent {
                    button: MouseButton::Left,
                    position: pointer,
                    modifiers: Default::default(),
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
        })?;
        harness.capture()?;
        harness.view.read_with(&harness.cx, |view, _| {
            let animation = view.editor.animation().unwrap();
            assert!(!animation.playing);
            assert_eq!(view.displayed_animation().unwrap().time, animation.time);
        });
    }
    Ok(())
}

pub(crate) fn animation_playback_screenshots(
    harness: &mut Harness,
    directory: &std::path::Path,
) -> anyhow::Result<()> {
    let (document, assets) = fixture::animation_document()?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        i18n::activate(Some("en"));
        harness.view.update(cx, |view, cx| {
            view.editor = Editor::from_document(document);
            view.edit_error = None;
            view.redraw(cx);
        });
        window.render_frame(cx);
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(editor_core::CommandId::FitView, window, cx)
        });
    })?;
    harness._assets = assets;
    let zero = harness.capture()?;
    save(&zero, directory, "animation-motion-zero.png")?;
    harness.cx.update_window(harness.window, |_, _, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor.toggle_animation().unwrap();
            view.editor.advance_animation_clock(1.);
            view.editor.pause_animation();
            let id = view.editor.active_id().unwrap();
            view.editor.seek_animation(id, 1.).unwrap();
            view.redraw(cx);
        });
    })?;
    let advanced = harness.capture()?;
    save(&advanced, directory, "animation-motion-one-second.png")?;
    anyhow::ensure!(
        harness.canvas_image(&zero)? != harness.canvas_image(&advanced)?,
        "Detached motion must change runtime pixels"
    );
    harness.view.read_with(&mut harness.cx, |view, _| {
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
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        assert!(!view.editor.document().unwrap().modified());
    });
    harness.cx.update_window(harness.window, |_, window, cx| {
        click_expression(window, 0, cx);
        harness.view.update(cx, |view, cx| {
            view.editor.toggle_animation().unwrap();
            view.editor.advance_animation_clock(1.5);
            view.editor.pause_animation();
            let id = view.editor.active_id().unwrap();
            view.editor.seek_animation(id, 1.5).unwrap();
            view.redraw(cx);
        });
    })?;
    let add = harness.capture()?;
    save(&add, directory, "animation-expression-add.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        click_expression(window, 2, cx);
        harness.view.update(cx, |view, cx| {
            let id = view.editor.active_id().unwrap();
            view.editor.seek_animation(id, 1.9).unwrap();
            view.redraw(cx);
        });
    })?;
    let overwrite = harness.capture()?;
    save(&overwrite, directory, "animation-expression-overwrite.png")?;
    anyhow::ensure!(
        harness.canvas_image(&add)? != harness.canvas_image(&overwrite)?,
        "Expression switching must change actual Metal pixels"
    );
    harness.cx.update_window(harness.window, |_, _, cx| {
        harness.view.update(cx, |view, cx| {
            let id = view.editor.active_id().unwrap();
            view.editor.seek_animation(id, 1.25).unwrap();
            view.redraw(cx);
        });
    })?;
    let back = harness.capture()?;
    save(&back, directory, "animation-expression-seek-back.png")?;
    harness.view.read_with(&mut harness.cx, |view, _| {
        let expressions = view.editor.document().unwrap().expression_resources();
        assert_eq!(
            view.editor
                .animation()
                .unwrap()
                .presented
                .as_ref()
                .unwrap()
                .active_expressions,
            vec![expressions[0].id.clone()]
        );
    });
    harness.cx.update_window(harness.window, |_, _, cx| {
        harness.view.update(cx, |view, cx| {
            let id = view.editor.active_id().unwrap();
            view.editor.seek_animation(id, 1.9).unwrap();
            view.redraw(cx);
        });
    })?;
    let replayed = harness.capture()?;
    anyhow::ensure!(
        harness.canvas_image(&overwrite)? == harness.canvas_image(&replayed)?,
        "Backward / forward seek must reproduce the same runtime pixels"
    );
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("animation-clear-expression", cx);
    })?;
    let clear = harness.capture()?;
    save(&clear, directory, "animation-expression-cleared.png")?;
    anyhow::ensure!(
        harness.canvas_image(&clear)? != harness.canvas_image(&overwrite)?,
        "Clearing must immediately rebuild without the expression"
    );
    harness.cx.update_window(harness.window, |_, _, cx| {
        harness.view.update(cx, |view, cx| {
            let id = view.editor.active_id().unwrap();
            view.editor.seek_animation(id, 0.25).unwrap();
            view.editor.seek_animation(id, 1.9).unwrap();
            view.redraw(cx);
        });
    })?;
    let clear_replayed = harness.capture()?;
    anyhow::ensure!(
        harness.canvas_image(&clear)? == harness.canvas_image(&clear_replayed)?,
        "Seek must never resurrect cleared expressions"
    );
    harness.view.read_with(&mut harness.cx, |view, _| {
        let animation = view.editor.animation().unwrap();
        assert!(!animation.playing);
        assert!(animation.expressions.is_empty());
        assert!(
            animation
                .presented
                .as_ref()
                .unwrap()
                .active_expressions
                .is_empty()
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        assert!(!view.editor.document().unwrap().modified());
    });
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.dock.as_ref().unwrap().show(
                editor_core::PanelKind::AnimationParameters,
                None,
                window,
                cx,
            )
        });
    })?;
    let parameters = harness.capture()?;
    save(&parameters, directory, "animation-readonly-parameters.png")?;
    // Verify localized resource feedback with actual rendered UI, before playback.
    let (document, diagnostics_assets) = fixture::diagnostic_animation_document()?;
    i18n::activate(Some("zh-CN"));
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.run_command(editor_core::CommandId::AnimationParameters, window, cx);
            view.editor = Editor::from_document(document);
            view.editor
                .set_workspace(editor_core::WorkspaceKind::Animation)
                .unwrap();
            view.redraw(cx);
        });
    })?;
    let diagnostics = harness.capture()?;
    save(&diagnostics, directory, "animation-diagnostics-zh.png")?;
    drop(diagnostics_assets);
    i18n::activate(Some("en"));
    Ok(())
}

// Optional real-model layout review; portable fixtures above remain the required checks.
pub(crate) fn animation_panel_review_screenshots(
    harness: &mut Harness,
    directory: &std::path::Path,
) -> anyhow::Result<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/local/mao/runtime/mao_pro.model3.json");
    if !path.is_file() {
        return Ok(());
    }
    let task = harness
        .cx
        .update_window(harness.window, |_, _, cx| {
            harness.view.update(cx, |view, _| {
                view.editor
                    .prepare_load(editor_core::LoadRequest::Model3(path))
            })
        })??
        .unwrap();
    let loaded = task.run();
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| -> anyhow::Result<()> {
            view.editor.finish_load(loaded)?;
            view.refresh_document(window, cx);
            view.run_command(editor_core::CommandId::AnimationWorkspace, window, cx);
            view.run_command(editor_core::CommandId::ResetLayout, window, cx);
            let id = view.editor.active_id().unwrap();
            let catalog = view.editor.document().unwrap().animation_catalog();
            if let Some(motion) = catalog.motions.first() {
                view.editor.select_motion(id, motion.selection.clone())?;
            }
            if let Some(expression) = catalog.expressions.first() {
                view.editor.select_expression(id, expression.id.clone())?;
            }
            view.editor.seek_animation(id, 0.8)?;
            view.redraw(cx);
            Ok(())
        })
    })??;
    harness.cx.update_window(harness.window, |_, window, cx| {
        use gpui_kit::{InputEvent as _, MouseMoveEvent};
        window.dispatch_event(
            MouseMoveEvent {
                position: point(px(550.), px(80.)),
                pressed_button: None,
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.refresh();
    })?;
    let dark = harness.capture()?;
    save(&dark, directory, "animation-panels-mao.png")?;
    for (command, file) in [
        (editor_core::CommandId::Physics, "animation-physics-mao.png"),
        (editor_core::CommandId::Pose, "animation-pose-mao.png"),
    ] {
        harness.cx.update_window(harness.window, |_, window, cx| {
            harness
                .view
                .update(cx, |view, cx| view.run_command(command, window, cx));
        })?;
        let inspector = harness.capture()?;
        save(&inspector, directory, file)?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            harness
                .view
                .update(cx, |view, cx| view.run_command(command, window, cx));
        })?;
    }
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click(("animation-motion-group", 0usize), cx);
    })?;
    let collapsed = harness.capture()?;
    save(&collapsed, directory, "animation-panels-mao-collapsed.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click(("animation-motion-group", 0usize), cx);
    })?;
    harness.cx.update(|cx| {
        i18n::activate(Some("zh-CN"));
        theme::change(gpui_kit::component::ThemeMode::Light, cx);
    });
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            // Request a fresh preview when changing the backdrop theme.
            let id = view.editor.active_id().unwrap();
            view.editor.seek_animation(id, 0.8).unwrap();
            view.redraw(cx);
        });
        window.refresh();
    })?;
    let light = harness.capture()?;
    save(&light, directory, "animation-panels-mao-zh-light.png")?;
    i18n::activate(Some("en"));
    Ok(())
}
