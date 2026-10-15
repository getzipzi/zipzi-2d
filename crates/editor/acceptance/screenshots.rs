//! Pixel sensitivity checks for the real editor, including its asynchronous Metal preview.
#![cfg(target_os = "macos")]

use crate::{i18n, theme};
use editor_core::Editor;
use gpui_kit::component::{ActiveTheme, WindowExt};
use gpui_kit::{BorrowAppContext as _, point, px, test::TestWindowExt};
use image::RgbaImage;
use std::{path::PathBuf, time::Duration};

use super::{fixture, harness::Harness};

fn save(image: &RgbaImage, directory: &std::path::Path, name: &str) -> anyhow::Result<()> {
    let path = directory.join(name);
    image.save_with_format(&path, image::ImageFormat::Png)?;
    // Verify the actual exported artifact, not just the in-memory buffer.
    anyhow::ensure!(
        image::open(&path)?.into_rgba8() == *image,
        "PNG round trip changed pixels"
    );
    println!("Screenshot: {}", path.display());
    Ok(())
}

fn animation_replacement_keeps_canvas(harness: &mut Harness) -> anyhow::Result<()> {
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
            let frame = view.animation_frame.as_ref().unwrap();
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
                view.animation_frame
                    .as_ref()
                    .unwrap()
                    .state
                    .identity
                    .request
                    > previous
            );
        });
    }
    Ok(())
}

fn animation_scrubbing_presents_frames_before_release() -> anyhow::Result<()> {
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

pub(super) fn run() -> anyhow::Result<()> {
    anyhow::ensure!(
        std::env::var_os("KASANE_DEV_SESSION_DIR").is_none(),
        "Unset KASANE_DEV_SESSION_DIR to keep screenshots isolated from development sessions"
    );
    let directory = std::env::var_os("KASANE_SCREENSHOT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/headless-screenshots")
        });
    std::fs::create_dir_all(&directory)?;
    if std::env::var("KASANE_SCREENSHOT_SCENE").as_deref() == Ok("compact-popups") {
        compact_popup_screenshots(&directory)?;
        println!("Compact popup density and scale screenshots passed.");
        return Ok(());
    }
    if std::env::var("KASANE_SCREENSHOT_SCENE").as_deref() == Ok("titlebar") {
        titlebar_screenshots(&directory)?;
        println!("Title bar project and empty-state screenshots passed.");
        return Ok(());
    }
    if std::env::var("KASANE_SCREENSHOT_SCENE").as_deref() == Ok("compact") {
        compact_screenshots(&directory)?;
        println!("Compact density, scale and Metal corner screenshots passed.");
        return Ok(());
    }
    if std::env::var("KASANE_SCREENSHOT_SCENE").as_deref() == Ok("dock") {
        dock_screenshots(&directory)?;
        println!("Dock Metal screenshots passed.");
        return Ok(());
    }
    if std::env::var("KASANE_SCREENSHOT_SCENE").as_deref() == Ok("timeline") {
        timeline_screenshots(&directory)?;
        println!("Timeline Metal screenshots passed.");
        return Ok(());
    }
    {
        let mut empty = Harness::new()?;
        empty.cx.update_window(empty.window, |_, window, cx| {
            empty.view.update(cx, |view, cx| {
                view.run_command(editor_core::CommandId::Close, window, cx);
            });
        })?;
        save(&empty.capture()?, &directory, "titlebar-empty.png")?;
        empty.cx.update_window(empty.window, |_, window, cx| {
            window.click("project-select", cx);
        })?;
        save(&empty.capture()?, &directory, "titlebar-project-menu.png")?;
    }
    animation_scrubbing_presents_frames_before_release()?;
    let mut harness = Harness::new()?;
    let baseline = harness.capture()?;
    anyhow::ensure!(
        baseline.dimensions() == (2200, 1480),
        "Unexpected screenshot size"
    );
    anyhow::ensure!(
        baseline == harness.capture()?,
        "Unchanged editor must render identical pixels"
    );
    save(&baseline, &directory, "editor.png")?;
    let baseline_canvas = harness.canvas_image(&baseline)?;

    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        anyhow::ensure!(
            window.try_find("animation-timeline").is_some(),
            "Animation transport is missing"
        );
        anyhow::ensure!(
            window.try_find("tool_glue").is_none(),
            "Animation must hide modeling tools"
        );
        Ok::<_, anyhow::Error>(())
    })??;
    let animation = harness.capture()?;
    save(&animation, &directory, "workspace-animation.png")?;
    anyhow::ensure!(
        animation != baseline,
        "Workspace switch must change the visible layout"
    );
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-modeling", cx);
    })?;
    let restored = harness.capture()?;
    anyhow::ensure!(
        harness.canvas_image(&restored)? == baseline_canvas,
        "Returning to Modeling must restore its canvas"
    );

    let mesh = harness
        .view
        .read_with(&harness.cx, |view, _| view.editor.layers()[0].id.clone());
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("canvas-body", cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(view.editor.selected_layer_id(), Some(mesh.as_str()));
        });
    })?;
    let selected = harness.capture()?;
    let selected_canvas = harness.canvas_image(&selected)?;
    let marker = harness.view.read_with(&mut harness.cx, |view, _| {
        let position = view.editor.overlay().vertices[0].position;
        let scale = view
            .frame
            .as_ref()
            .unwrap()
            .canvas
            .transform
            .metrics
            .scale_factor;
        (
            (position.x * scale).round() as u32,
            (position.y * scale).round() as u32,
        )
    });
    anyhow::ensure!(
        image::imageops::crop_imm(&selected_canvas, marker.0 - 8, marker.1 - 8, 16, 16).to_image()
            != image::imageops::crop_imm(&baseline_canvas, marker.0 - 8, marker.1 - 8, 16, 16)
                .to_image(),
        "Selecting the mesh must paint a vertex marker at its canvas position"
    );
    save(&selected, &directory, "editor-selected.png")?;

    harness.cx.update_window(harness.window, |_, window, cx| {
        window.hover(format!("mesh-item-Mesh:{mesh}"), cx);
        window
            .within("mesh-tree-panel")
            .click(format!("mesh-lock-{mesh}"), cx);
        harness.view.read_with(cx, |view, _| {
            assert!(view.editor.overlay().vertices.is_empty());
            assert!(
                view.editor
                    .document()
                    .unwrap()
                    .object_state_index()
                    .is_locked(&editor_core::ObjectRef::new(
                        editor_core::ObjectKind::Mesh,
                        &mesh
                    ))
            );
        });
    })?;
    let locked = harness.capture()?;
    let locked_canvas = harness.canvas_image(&locked)?;
    anyhow::ensure!(
        image::imageops::crop_imm(&locked_canvas, marker.0 - 8, marker.1 - 8, 16, 16).to_image()
            != image::imageops::crop_imm(&selected_canvas, marker.0 - 8, marker.1 - 8, 16, 16)
                .to_image(),
        "Locking must remove the editable vertex marker"
    );
    let (w, h) = locked_canvas.dimensions();
    anyhow::ensure!(
        image::imageops::crop_imm(&locked_canvas, w / 2 - 10, h / 2 - 10, 20, 20).to_image()
            == image::imageops::crop_imm(&selected_canvas, w / 2 - 10, h / 2 - 10, 20, 20)
                .to_image(),
        "Locking must preserve the rendered artwork"
    );
    save(&locked, &directory, "editor-locked.png")?;

    harness.cx.update_window(harness.window, |_, window, cx| {
        window
            .within("mesh-tree-panel")
            .click(format!("mesh-vis-{mesh}"), cx);
    })?;
    let hidden = harness.capture()?;
    let hidden_canvas = harness.canvas_image(&hidden)?;
    let changed = hidden_canvas
        .pixels()
        .zip(baseline_canvas.pixels())
        .filter(|(a, b)| a != b)
        .count();
    anyhow::ensure!(
        // The diamond occupies over a quarter of its 256x256 canvas at actual pixel size.
        changed > 256 * 256 / 4,
        "Hiding the model must remove its filled pixels, not just the overlay"
    );
    save(&hidden, &directory, "editor-hidden.png")?;

    let (document, assets) = fixture::sample_document_with_parameter_groups()?;
    let sliding_parameter = document.parameters()[0].id.clone();
    harness._assets = assets;
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor = Editor::from_document(document);
            let dock = view.dock.as_ref().unwrap();
            // Give the grouped list a full-height, standard-width panel for visual QA.
            use gpui_kit::component::dock::{DockLayout, panel_handle};
            dock.area.update(cx, |area, cx| {
                area.set_center(
                    DockLayout::h_split()
                        .child(
                            DockLayout::tabs()
                                .panel_view(panel_handle(dock.active_canvas().clone()), cx),
                            None,
                        )
                        .child(
                            DockLayout::tabs()
                                .panel_view(panel_handle(dock.parameters.clone()), cx),
                            Some(px(360.)),
                        ),
                    window,
                    cx,
                );
            });
            view.redraw(cx);
        });
    })?;
    let grouped = harness.capture()?;
    save(
        &harness.element_image(&grouped, "parameters-panel")?,
        &directory,
        "parameters-grouped.png",
    )?;
    let number_origin = harness.cx.update_window(harness.window, |_, window, cx| {
        use gpui_kit::{InputEvent as _, MouseButton, MouseDownEvent, MouseMoveEvent};
        let position = window
            .find(format!("parameter-value-{sliding_parameter}"))
            .bounds()
            .center();
        window.dispatch_event(
            MouseMoveEvent {
                position,
                pressed_button: None,
                modifiers: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.dispatch_event(
            MouseDownEvent {
                button: MouseButton::Left,
                position,
                modifiers: Default::default(),
                click_count: 1,
                first_mouse: false,
            }
            .to_platform_input(),
            cx,
        );
        position
    })?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        use gpui_kit::{InputEvent as _, Modifiers, MouseButton, MouseMoveEvent};
        window.dispatch_event(
            MouseMoveEvent {
                position: number_origin + point(px(16.), px(0.)),
                pressed_button: Some(MouseButton::Left),
                modifiers: Modifiers {
                    shift: true,
                    ..Default::default()
                },
            }
            .to_platform_input(),
            cx,
        );
    })?;
    let sliding = harness.capture()?;
    let row_id = format!("parameter-row-{sliding_parameter}");
    let sliding_row = harness.element_image(&sliding, &row_id)?;
    anyhow::ensure!(
        sliding_row != harness.element_image(&grouped, &row_id)?,
        "Sliding must display the live value and interaction border"
    );
    save(&sliding_row, &directory, "parameter-number-sliding.png")?;
    harness
        .cx
        .update_window(harness.window, |_, window, cx| window.press("escape", cx))?;
    let face = uuid::Uuid::from_u128(101).to_string();
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-group-toggle-{face}"), cx);
        assert!(
            !window
                .find(format!("parameter-group-{face}"))
                .expanded()
                .unwrap()
        );
        assert!(
            window
                .try_find(format!("parameter-name-{}", uuid::Uuid::from_u128(3)))
                .is_none()
        );
    })?;
    let collapsed = harness.capture()?;
    let collapsed_panel = harness.element_image(&collapsed, "parameters-panel")?;
    anyhow::ensure!(
        collapsed != grouped,
        "Collapsing a parameter group must change its rendered rows"
    );
    save(&collapsed_panel, &directory, "parameters-collapsed.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-group-toggle-{face}"), cx);
        theme::change(gpui_kit::component::ThemeMode::Light, cx);
    })?;
    let light = harness.capture()?;
    save(
        &harness.element_image(&light, "parameters-panel")?,
        &directory,
        "parameters-grouped-light.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        cx.set_reduce_motion(true);
        theme::change(gpui_kit::component::ThemeMode::Dark, cx);
        window.click("new-parameter", cx);
    })?;
    let popup = harness.capture()?;
    harness.cx.update_window(harness.window, |_, window, _| {
        assert!(window.try_find("new-parameter-advanced").is_none());
        for field in [
            "new-parameter-precision",
            "new-parameter-kind",
            "new-parameter-group",
            "new-parameter-repeat",
        ] {
            assert!(
                window.find(field).visible(),
                "{field} must be visible on open"
            );
        }
    })?;
    save(
        &harness.dialog_image(&popup)?,
        &directory,
        "new-parameter.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("new-parameter-cancel", cx);
        i18n::activate(Some("zh-CN"));
        theme::change(gpui_kit::component::ThemeMode::Light, cx);
        window.click("new-parameter", cx);
    })?;
    let popup_zh = harness.capture()?;
    save(
        &harness.dialog_image(&popup_zh)?,
        &directory,
        "new-parameter-zh-light.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("new-parameter-cancel", cx);
    })?;
    for (action, filename) in [
        (
            crate::panels::parameters::ParameterAction::Edit(uuid::Uuid::from_u128(3).to_string()),
            "edit-parameter-zh-light.png",
        ),
        (
            crate::panels::parameters::ParameterAction::CreateGroup,
            "new-parameter-group-zh-light.png",
        ),
        (
            crate::panels::parameters::ParameterAction::DeleteGroup(
                uuid::Uuid::from_u128(101).to_string(),
            ),
            "delete-parameter-group-zh-light.png",
        ),
    ] {
        harness.cx.update_window(harness.window, |_, window, cx| {
            let panel = harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .parameters
                .clone();
            panel.update(cx, |panel, cx| panel.open_management(action, window, cx));
        })?;
        let popup = harness.capture()?;
        save(&harness.dialog_image(&popup)?, &directory, filename)?;
        harness
            .cx
            .update_window(harness.window, |_, window, cx| window.close_dialog(cx))?;
    }
    let (document, assets) = fixture::rectangle_document(false)?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor = Editor::from_document(document);
            view.redraw(cx);
        });
        window.render_frame(cx);
        window.click("tool_generate_mesh", cx);
    })?;
    harness._assets = assets;
    harness.cx.run_until_parked();
    harness.cx.advance_clock(Duration::from_millis(150));
    harness.cx.run_until_parked();
    let mesh_preview = harness.capture()?;
    save(
        &harness.dialog_image(&mesh_preview)?,
        &directory,
        "mesh-generation-zh-light.png",
    )?;
    let wire = harness.element_image(&mesh_preview, "mesh-generation-preview")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("preview-wireframe", cx);
    })?;
    let artwork = harness.capture()?;
    anyhow::ensure!(
        wire != harness.element_image(&artwork, "mesh-generation-preview")?,
        "Wireframe toggle must change preview pixels"
    );
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-generation-cancel", cx);
        i18n::activate(Some("en"));
        theme::change(gpui_kit::component::ThemeMode::Dark, cx);
        window.click("tool_generate_mesh", cx);
    })?;
    harness.cx.run_until_parked();
    harness.cx.advance_clock(Duration::from_millis(150));
    harness.cx.run_until_parked();
    let mesh_preview = harness.capture()?;
    save(
        &harness.dialog_image(&mesh_preview)?,
        &directory,
        "mesh-generation-dark.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("preview-fit", cx);
    })?;
    let fit_preview = harness.capture()?;
    let fitted = harness.element_image(&fit_preview, "preview-viewport")?;
    anyhow::ensure!(fitted.width() == fitted.height(), "Preview must be square");
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("preview-zoom-in", cx);
    })?;
    let zoomed = harness.capture()?;
    let zoomed_view = harness.element_image(&zoomed, "preview-viewport")?;
    anyhow::ensure!(fitted != zoomed_view, "Zoom must change preview pixels");
    harness.cx.update_window(harness.window, |_, window, cx| {
        let center = window.find("preview-viewport").bounds().center();
        window.drag(center, center + point(px(50.), px(30.)), cx);
    })?;
    let panned = harness.capture()?;
    anyhow::ensure!(
        zoomed_view != harness.element_image(&panned, "preview-viewport")?,
        "Dragging must pan the image"
    );
    save(
        &harness.dialog_image(&panned)?,
        &directory,
        "mesh-generation-panned.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("preview-fit", cx);
    })?;
    let reset = harness.capture()?;
    anyhow::ensure!(
        fitted == harness.element_image(&reset, "preview-viewport")?,
        "Fit must restore both zoom and pan"
    );
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-generation-cancel", cx);
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
    })?;
    let (document, assets) = fixture::alpha_document()?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        i18n::activate(Some("zh-CN"));
        harness.view.update(cx, |view, cx| {
            view.editor = Editor::from_document(document);
            view.redraw(cx);
        });
        window.render_frame(cx);
        window.click("tool_generate_mesh", cx);
    })?;
    harness._assets = assets;
    harness.capture()?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-generation-alpha-heavy", cx);
    })?;
    harness.cx.run_until_parked();
    harness.cx.advance_clock(Duration::from_millis(150));
    harness.cx.run_until_parked();
    let alpha = harness.capture()?;
    save(
        &harness.dialog_image(&alpha)?,
        &directory,
        "mesh-generation-alpha.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        assert!(window.find("mesh-generation-counts").visible());
        window.click("preview-wireframe", cx);
    })?;
    let artwork = harness.capture()?;
    anyhow::ensure!(
        harness.element_image(&alpha, "mesh-generation-preview")?
            != harness.element_image(&artwork, "mesh-generation-preview")?,
        "Alpha mesh wireframe must change rendered pixels"
    );
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-generation-cancel", cx);
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
    })?;
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
    save(&zero, &directory, "animation-motion-zero.png")?;
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
    save(&advanced, &directory, "animation-motion-one-second.png")?;
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
    save(&add, &directory, "animation-expression-add.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        click_expression(window, 2, cx);
        harness.view.update(cx, |view, cx| {
            let id = view.editor.active_id().unwrap();
            view.editor.seek_animation(id, 1.9).unwrap();
            view.redraw(cx);
        });
    })?;
    let overwrite = harness.capture()?;
    save(&overwrite, &directory, "animation-expression-overwrite.png")?;
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
    save(&back, &directory, "animation-expression-seek-back.png")?;
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
    save(&clear, &directory, "animation-expression-cleared.png")?;
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
    save(&parameters, &directory, "animation-readonly-parameters.png")?;
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
    save(&diagnostics, &directory, "animation-diagnostics-zh.png")?;
    drop(diagnostics_assets);
    i18n::activate(Some("en"));
    animation_replacement_keeps_canvas(&mut harness)?;
    animation_panel_review_screenshots(&mut harness, &directory)?;
    println!("Headless editor screenshots passed (real Metal, no native window).");
    Ok(())
}

// Optional real-model layout review; portable fixtures above remain the required checks.
fn animation_panel_review_screenshots(
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

fn timeline_screenshots(directory: &std::path::Path) -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    let (document, assets) = fixture::animation_document()?;
    harness._assets = assets;
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor = Editor::from_document(document);
            view.redraw(cx);
        });
        window.click("workspace-animation", cx);
        window.render_frame(cx);
        window.click(("animation-motion", 0usize), cx);
        window.render_frame(cx);
        harness.view.update(cx, |view, cx| {
            view.run_command(editor_core::CommandId::FitView, window, cx);
            let projection = view.editor.timeline_projection().unwrap();
            let row = projection
                .rows
                .iter()
                .find(|row| row.track_id.is_some())
                .unwrap();
            let key = row.keys[0].clone();
            let row = row.id.clone();
            let d = view.editor.active_id().unwrap();
            view.editor
                .update_timeline_view(d, |state| {
                    state.channels.insert(row);
                    state.keys.insert(key);
                })
                .unwrap();
            view.redraw(cx);
        });
    })?;
    let overview = harness.capture()?;
    save(&overview, directory, "timeline-overview.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        let bounds = window.find("editor-bottom-dock").bounds();
        let scale = window.scale_factor();
        let expected = if cx.theme().is_dark() { 31 } else { 217 };
        for fraction in [0.25, 0.5, 0.75] {
            let x = ((bounds.left().as_f32() + bounds.size.width.as_f32() * fraction) * scale)
                .floor() as u32;
            let y = (bounds.top().as_f32() * scale).ceil() as u32;
            let pixel = overview.get_pixel(x, y);
            anyhow::ensure!(
                pixel.0[..3]
                    .iter()
                    .all(|channel| channel.abs_diff(expected) <= 3),
                "Extra line above Timeline at ({x}, {y}): {:?}",
                pixel.0
            );
        }
        Ok::<_, anyhow::Error>(())
    })??;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("timeline-interpolation-bezier", cx);
        window.render_frame(cx);
        window.click("timeline-view", cx);
        window.render_frame(cx);
    })?;
    let curves = harness.capture()?;
    save(&curves, directory, "timeline-curves.png")?;
    anyhow::ensure!(
        overview != curves,
        "Timeline mode did not alter the rendered scene"
    );
    let canvas_before = harness.canvas_image(&curves)?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        let key = harness
            .view
            .read(cx)
            .editor
            .timeline_view()
            .unwrap()
            .keys
            .first()
            .unwrap()
            .clone();
        window.click(format!("timeline-number-KeyValue({key:?})"), cx);
        window.press("cmd-a", cx);
        window.input("0.6", cx);
    })?;
    let candidate = harness.capture()?;
    save(&candidate, directory, "timeline-candidate.png")?;
    anyhow::ensure!(
        harness.canvas_image(&candidate)? != canvas_before,
        "Candidate key edit did not alter the rendered image"
    );
    harness.cx.update_window(harness.window, |_, window, cx| {
        let view = harness.view.read(cx);
        anyhow::ensure!(
            view.editor.timeline_edit().is_some(),
            "Candidate was committed during preview"
        );
        anyhow::ensure!(
            view.editor.document().unwrap().history_lengths() == (1, 0),
            "Preview added history"
        );
        window.press("escape", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    let restored = harness.capture()?;
    save(&restored, directory, "timeline-cancel-restored.png")?;
    anyhow::ensure!(
        harness.canvas_image(&restored)? == canvas_before,
        "Cancel did not restore the canvas image"
    );
    harness.cx.update_window(harness.window, |_, _window, cx| {
        i18n::activate(Some("zh-CN"));
        theme::change(gpui_kit::component::ThemeMode::Light, cx);
        harness.view.update(cx, |view, cx| view.redraw(cx));
    })?;
    save(
        &harness.capture()?,
        directory,
        "timeline-curves-zh-light.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        use gpui_kit::component::dock::{DockPlacement, InsertTarget, PanelId};
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
    })?;
    save(
        &harness.capture()?,
        directory,
        "timeline-narrow-zh-light.png",
    )?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        anyhow::ensure!(
            window.find("timeline-graph").bounds().size.width >= px(150.),
            "Narrow dock lost its editable graph"
        );
        anyhow::ensure!(
            window.find("timeline-graph").bounds().size.height >= px(50.),
            "Narrow dock chrome consumed the graph height"
        );
        window.click("timeline-more", cx);
        window.render_frame(cx);
        window.click("timeline-menu-properties", cx);
        Ok::<_, anyhow::Error>(())
    })??;
    save(
        &harness.capture()?,
        directory,
        "timeline-narrow-properties-zh-light.png",
    )?;
    i18n::activate(Some("en"));
    Ok(())
}

fn compact_popup_screenshots(directory: &std::path::Path) -> anyhow::Result<()> {
    use crate::settings::{Settings, UiScale};
    use gpui_kit::component::ThemeMode;
    for (mode, locale, appearance) in [
        (ThemeMode::Dark, "en", "dark"),
        (ThemeMode::Light, "zh-CN", "light-zh"),
    ] {
        let mut harness = Harness::new()?;
        for scale in UiScale::ALL {
            harness
                .cx
                .update_window(harness.window, |_, window, _| window.activate_window())?;
            i18n::activate(Some(locale));
            harness.cx.update(|cx| {
                cx.update_global::<Settings, _>(|settings, _| {
                    settings.set_ui_scale(scale).unwrap()
                });
                theme::change(mode, cx);
            });
            harness.view.update(&mut harness.cx, |_, cx| cx.notify());
            harness.capture()?;
            let name = format!(
                "compact-popup-{appearance}-{}",
                scale.label().trim_end_matches('%')
            );
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.click("project-select", cx);
            })?;
            save(
                &harness.capture()?,
                directory,
                &format!("{name}-projects.png"),
            )?;
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.press("escape", cx);
            })?;
            harness.capture()?;
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.click("workspace-layout-menu", cx);
                // The first entry opens an Add Panel submenu.
                window.within("popup-menu").hover(0usize, cx);
                window.render_frame(cx);
                let row = window
                    .within("submenu")
                    .within("popup-menu")
                    .find(0usize)
                    .bounds();
                anyhow::ensure!(
                    (row.size.height - px(20. * scale.factor())).abs() <= px(0.5),
                    "Submenu density at {}: {row:?}",
                    scale.label()
                );
                Ok::<_, anyhow::Error>(())
            })??;
            save(
                &harness.capture()?,
                directory,
                &format!("{name}-submenu.png"),
            )?;
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.press("escape", cx);
            })?;
            harness.capture()?;
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.click("new-parameter", cx);
            })?;
            harness.capture()?;
            for (id, suffix) in [
                ("new-parameter-kind", "select"),
                ("new-parameter-group", "search-select"),
            ] {
                harness.cx.update_window(harness.window, |_, window, cx| {
                    let bounds = window.find(id).bounds();
                    anyhow::ensure!(
                        (bounds.size.height - px(crate::ui::compact::CONTROL * scale.factor()))
                            .abs()
                            <= px(0.5),
                        "Select density at {}: {bounds:?}",
                        scale.label()
                    );
                    window.click(id, cx);
                    Ok::<_, anyhow::Error>(())
                })??;
                save(
                    &harness.capture()?,
                    directory,
                    &format!("{name}-{suffix}.png"),
                )?;
                harness
                    .cx
                    .update_window(harness.window, |_, window, cx| window.press("escape", cx))?;
            }
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.click("new-parameter-cancel", cx)
            })?;
            harness.capture()?;
        }
    }
    i18n::activate(Some("en"));
    Ok(())
}

fn titlebar_screenshots(directory: &std::path::Path) -> anyhow::Result<()> {
    use gpui_kit::component::ThemeMode;
    for (mode, locale, appearance) in [
        (ThemeMode::Dark, "en", "dark"),
        (ThemeMode::Light, "zh-CN", "light-zh"),
    ] {
        let mut harness = Harness::new()?;
        i18n::activate(Some(locale));
        harness.cx.update(|cx| theme::change(mode, cx));
        harness.cx.update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                let first = view.editor.active_id().unwrap();
                view.editor.new_document().unwrap();
                view.editor.new_document().unwrap();
                view.editor.activate(first).unwrap();
                view.editor
                    .execute(editor_core::EditorCommand::CreateParameter(
                        editor_core::NewParameter {
                            name: "Test".into(),
                            runtime_id: "ParamTest".into(),
                            minimum: -1.,
                            maximum: 1.,
                            default_value: 0.,
                            ..Default::default()
                        },
                    ))
                    .unwrap();
                view.refresh_document(window, cx);
            });
        })?;
        save(
            &harness.capture()?,
            directory,
            &format!("titlebar-{appearance}.png"),
        )?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            window.click("project-select", cx)
        })?;
        save(
            &harness.capture()?,
            directory,
            &format!("titlebar-{appearance}-projects.png"),
        )?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            window.press("escape", cx);
            harness.view.update(cx, |view, cx| {
                view.editor = Editor::empty();
                view.refresh_document(window, cx);
            });
        })?;
        save(
            &harness.capture()?,
            directory,
            &format!("titlebar-{appearance}-empty.png"),
        )?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            window.click("project-select", cx)
        })?;
        save(
            &harness.capture()?,
            directory,
            &format!("titlebar-{appearance}-empty-menu.png"),
        )?;
    }
    i18n::activate(Some("en"));
    Ok(())
}

fn compact_screenshots(directory: &std::path::Path) -> anyhow::Result<()> {
    use crate::settings::{Settings, UiScale};
    use gpui_kit::component::dock::{DockPlacement, PaneNode, PaneRef};
    use gpui_kit::component::{ActiveTheme, ThemeMode};
    fn groups(node: &PaneNode, result: &mut Vec<u64>) {
        match node.kind() {
            PaneRef::Tabs { .. } => result.push(node.id().as_u64()),
            PaneRef::Split { children, .. } => {
                for child in children {
                    groups(child, result);
                }
            }
        }
    }
    let mut harness = Harness::new()?;
    let (mut document, assets) = fixture::sample_document()?;
    for index in 0..3 {
        document.create_parameter(editor_core::NewParameter {
            name: format!("Parameter {index}"),
            runtime_id: format!("Param{index}"),
            minimum: -1.,
            maximum: 1.,
            default_value: 0.,
            ..Default::default()
        })?;
    }
    harness._assets = assets;
    harness.view.update(&mut harness.cx, |view, cx| {
        view.editor = Editor::from_document(document);
        view.redraw(cx);
    });
    let mut metrics = Vec::new();
    for scale in UiScale::ALL {
        for (mode, locale, appearance) in [
            (ThemeMode::Dark, "en", "dark"),
            (ThemeMode::Light, "zh-CN", "light-zh"),
        ] {
            i18n::activate(Some(locale));
            harness.cx.update(|cx| {
                cx.update_global::<Settings, _>(|settings, _| {
                    settings.set_ui_scale(scale).unwrap()
                });
                theme::change(mode, cx);
            });
            harness.view.update(&mut harness.cx, |_, cx| cx.notify());
            let screenshot = harness.capture()?;
            let name = format!(
                "compact-{appearance}-{}",
                scale.label().trim_end_matches('%')
            );
            save(&screenshot, directory, &format!("{name}.png"))?;
            let data = harness.cx.update_window(harness.window, |_, window, cx| {
                let display_scale = window.scale_factor();
                let canvas = window.find("canvas-body").bounds();
                let viewport = window.viewport_size();
                let mut nodes = Vec::new();
                let dock = harness.view.read(cx).dock.as_ref().unwrap();
                for placement in [DockPlacement::Center, DockPlacement::Left, DockPlacement::Right, DockPlacement::Bottom] {
                    if let Some(tree) = dock.area.read(cx).layout(placement) { groups(tree.root(), &mut nodes); }
                }
                let mut overflow = 0;
                let headers: Vec<_> = nodes.iter().filter_map(|node| {
                    let id = format!("dock-header-{node}");
                    let header = window.try_find(id.clone())?;
                    if window.within(id.clone()).try_find("panel-header-tools").is_some() { overflow += 1; }
                    let bounds = header.bounds();
                    Some(serde_json::json!({"node": node, "x": bounds.left().as_f32(), "y": bounds.top().as_f32(), "width": bounds.size.width.as_f32(), "height": bounds.size.height.as_f32()}))
                }).collect();
                // A local corner mask must cover real Metal content at every scale.
                let expected = if cx.theme().is_dark() { 31 } else { 217 };
                for x in [(canvas.left().as_f32() * display_scale).ceil() as u32, (canvas.right().as_f32() * display_scale).floor() as u32 - 1] {
                    let pixel = screenshot.get_pixel(x, (canvas.bottom().as_f32() * display_scale).floor() as u32 - 1);
                    anyhow::ensure!(pixel.0[..3].iter().all(|channel| channel.abs_diff(expected) <= 3), "Metal corner leaked at {}: {:?}, canvas={:?}, x={:?}", scale.label(), pixel.0, canvas, x);
                }
                let tree_height = (window.find("mesh-tree-panel").bounds().size.height.as_f32() - crate::ui::compact::ROW * scale.factor()).max(0.);
                Ok::<_, anyhow::Error>(serde_json::json!({"scene": name, "logical_window": [viewport.width.as_f32(), viewport.height.as_f32()], "display_scale": display_scale, "ui_scale": scale.factor(), "canvas": {"width": canvas.size.width.as_f32(), "height": canvas.size.height.as_f32(), "area": canvas.size.width.as_f32() * canvas.size.height.as_f32()}, "tree_row_capacity": (tree_height / (crate::ui::compact::ROW * scale.factor())).floor(), "header_overflow_count": overflow, "headers": headers}))
            })??;
            metrics.push(data);
            harness.cx.update_window(harness.window, |_, window, cx| {
                window.hover("tool_glue", cx);
            })?;
            let hovered = harness.capture()?;
            harness.cx.update_window(harness.window, |_, window, cx| {
                let bounds = window.find("tool_glue").bounds();
                let display_scale = window.scale_factor();
                for point in [
                    point(bounds.left(), bounds.center().y),
                    point(bounds.right() - px(0.5), bounds.center().y),
                    // The preceding button owns the joined top border.
                    point(bounds.center().x, bounds.top() - px(0.5)),
                    point(bounds.center().x, bounds.bottom() - px(0.5)),
                ] {
                    let x = (point.x.as_f32() * display_scale).floor() as u32;
                    let y = (point.y.as_f32() * display_scale).floor() as u32;
                    anyhow::ensure!(
                        screenshot.get_pixel(x, y) == hovered.get_pixel(x, y),
                        "Toolbar hover changed border at {name} ({x}, {y})"
                    );
                }
                let x = ((bounds.left() + px(4.)).as_f32() * display_scale).floor() as u32;
                let y = ((bounds.top() + px(4.)).as_f32() * display_scale).floor() as u32;
                anyhow::ensure!(
                    screenshot.get_pixel(x, y) != hovered.get_pixel(x, y),
                    "Toolbar hover did not change its fill at {name}"
                );
                window.hover("workspace-titlebar", cx);
                Ok::<_, anyhow::Error>(())
            })??;
            save(&hovered, directory, &format!("{name}-toolbar-hover.png"))?;
            let previous_zoom = harness
                .view
                .read_with(&harness.cx, |view, _| view.editor.view().zoom);
            harness.view.update(&mut harness.cx, |view, cx| {
                view.editor.set_zoom_at(50., None);
                view.redraw(cx);
            });
            let covered = harness.capture()?;
            save(&covered, directory, &format!("{name}-metal-corners.png"))?;
            harness.cx.update_window(harness.window, |_, window, cx| {
                let canvas = window.find("canvas-body").bounds();
                let display_scale = window.scale_factor();
                let expected = if cx.theme().is_dark() { 31 } else { 217 };
                for x in [
                    (canvas.left().as_f32() * display_scale).ceil() as u32,
                    (canvas.right().as_f32() * display_scale).floor() as u32 - 1,
                ] {
                    let pixel = covered.get_pixel(
                        x,
                        (canvas.bottom().as_f32() * display_scale).floor() as u32 - 1,
                    );
                    anyhow::ensure!(
                        pixel.0[..3]
                            .iter()
                            .all(|channel| channel.abs_diff(expected) <= 3),
                        "Opaque Metal leaked outside rounded canvas at {}: {:?}",
                        scale.label(),
                        pixel.0
                    );
                }
                let inside = covered.get_pixel(
                    ((canvas.left().as_f32() + 8.) * display_scale).floor() as u32,
                    ((canvas.bottom().as_f32() - 8.) * display_scale).floor() as u32,
                );
                anyhow::ensure!(
                    inside.0[1] > inside.0[0].saturating_add(20),
                    "Metal corner fixture did not cover the canvas: {:?}",
                    inside.0
                );
                Ok::<_, anyhow::Error>(())
            })??;
            harness.view.update(&mut harness.cx, |view, cx| {
                view.editor.set_zoom_at(previous_zoom, None);
                view.redraw(cx);
            });
        }
    }
    std::fs::write(
        directory.join("compact-metrics.json"),
        serde_json::to_vec_pretty(&metrics)?,
    )?;
    i18n::activate(Some("en"));
    Ok(())
}

fn dock_screenshots(directory: &std::path::Path) -> anyhow::Result<()> {
    use gpui_kit::component::dock::{DockLayout, DockPlacement, PanelId, panel_handle};
    let mut harness = Harness::new()?;
    save(&harness.capture()?, directory, "dock-workspace.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        let dock = harness.view.read(cx).dock.as_ref().unwrap();
        let node = dock
            .area
            .read(cx)
            .layout(DockPlacement::Center)
            .unwrap()
            .find_panel_node(PanelId::from(dock.mesh_tree.entity_id()))
            .unwrap();
        window.click(format!("dock-selector-{}", node.as_u64()), cx);
    })?;
    save(&harness.capture()?, directory, "dock-panel-selector.png")?;
    harness
        .cx
        .update_window(harness.window, |_, window, cx| window.press("escape", cx))?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        let dock = harness.view.read(cx).dock.as_ref().unwrap();
        let area = dock.area.clone();
        let layout = DockLayout::h_split()
            .child(
                DockLayout::tabs().panel_view(panel_handle(dock.mesh_tree.clone()), cx),
                Some(px(100.)),
            )
            .child(
                DockLayout::tabs().panel_view(panel_handle(dock.active_canvas().clone()), cx),
                None,
            );
        area.update(cx, |area, cx| area.set_center(layout, window, cx));
    })?;
    save(&harness.capture()?, directory, "dock-narrow.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click("panel-header-tools", cx)
    })?;
    save(&harness.capture()?, directory, "dock-narrow-tools.png")?;
    harness
        .cx
        .update_window(harness.window, |_, window, cx| window.press("escape", cx))?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        let dock = harness.view.read(cx).dock.as_ref().unwrap();
        let area = dock.area.clone();
        let layout = DockLayout::tabs()
            .panel_view(panel_handle(dock.active_canvas().clone()), cx)
            .panel_view(panel_handle(dock.mesh_tree.clone()), cx);
        area.update(cx, |area, cx| area.set_center(layout, window, cx));
    })?;
    save(&harness.capture()?, directory, "dock-canvas-group.png")?;
    i18n::activate(Some("zh-CN"));
    harness
        .cx
        .update(|cx| theme::change(gpui_kit::component::ThemeMode::Light, cx));
    harness.view.update(&mut harness.cx, |_, cx| cx.notify());
    save(
        &harness.capture()?,
        directory,
        "dock-canvas-group-zh-light.png",
    )?;
    i18n::activate(Some("en"));
    Ok(())
}

fn click_expression(window: &mut gpui_kit::Window, index: usize, cx: &mut gpui_kit::App) {
    window.click("animation-expression-preview-result", cx);
    window.render_frame(cx);
    let bounds = window.find(("animation-expression", index)).bounds();
    let viewport = window.find("animation-expression-list").bounds();
    window.scroll(
        "animation-expression-list",
        gpui_kit::ScrollDelta::Pixels(point(px(0.), viewport.center().y - bounds.center().y)),
        cx,
    );
    window.click(("animation-expression", index), cx);
}
