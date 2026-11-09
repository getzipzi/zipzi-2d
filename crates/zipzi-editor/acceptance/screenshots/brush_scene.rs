//! Native visual QA for weighted selection and the bounded brush controls.
use super::super::harness::Harness;
use super::common::save;
use crate::{
    i18n,
    settings::{Settings, UiScale},
};
use gpui_kit::{BorrowAppContext as _, InputEvent as _, test::TestWindowExt};
use zipzi_editor_core::{BrushSelectionSettings, CommandId, EditorCommand, ObjectKind, ObjectRef};

pub(crate) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    let mut harness = Harness::with_size(1100., 740.)?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.activate_window();
        i18n::activate(Some("zh-CN"));
        harness.view.update(cx, |view, cx| {
            let mesh = view.editor.layers().into_iter().next().unwrap().id;
            view.editor
                .execute(EditorCommand::SelectObject(ObjectRef::new(
                    ObjectKind::Mesh,
                    mesh,
                )))
                .unwrap();
            view.refresh_document(window, cx);
        });
    })?;
    harness.capture()?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::BrushSelectTool, window, cx);
            view.editor
                .execute(EditorCommand::SetBrushSelectionSettings(
                    BrushSelectionSettings {
                        weight: 0.2,
                        hardness: 0.2,
                        diameter: 280.,
                        color: [1., 0., 0., 0.8],
                        ..Default::default()
                    },
                ))
                .unwrap();
            cx.notify();
        });
    })?;
    let before = harness.capture()?;
    let mut probe = None;
    let mut tint_distances = Vec::new();
    for pass in 1..=8 {
        harness.cx.update_window(harness.window, |_, window, cx| {
            let p = harness.view.read(cx).editor.overlay().vertices[0].position;
            let origin = window.find("canvas-body").bounds().origin;
            let position = origin + gpui_kit::point(gpui_kit::px(p.x), gpui_kit::px(p.y));
            let modifiers = gpui_kit::Modifiers {
                shift: pass > 1,
                ..Default::default()
            };
            window.dispatch_event(
                gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position,
                    modifiers,
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            window.dispatch_event(
                gpui_kit::MouseUpEvent {
                    button: gpui_kit::MouseButton::Left,
                    position,
                    modifiers,
                    click_count: 1,
                }
                .to_platform_input(),
                cx,
            );
            if probe.is_none() {
                let overlay = harness.view.read(cx).editor.overlay();
                let (points, _) = overlay
                    .selection_weights
                    .unwrap()
                    .triangles
                    .into_iter()
                    .max_by(|a, b| a.1.iter().sum::<f32>().total_cmp(&b.1.iter().sum::<f32>()))
                    .expect("painted mesh triangle");
                let x = origin.x.as_f32() + points.iter().map(|p| p.x).sum::<f32>() / 3.;
                let y = origin.y.as_f32() + points.iter().map(|p| p.y).sum::<f32>() / 3.;
                probe = Some((
                    (x * window.scale_factor()) as u32,
                    (y * window.scale_factor()) as u32,
                ));
            }
        })?;
        if [1, 3, 8].contains(&pass) {
            let image = harness.capture()?;
            let (x, y) = probe.unwrap();
            let distance = |image: &image::RgbaImage| {
                let p = image.get_pixel(x, y).0;
                255 - p[0] as u32 + p[1] as u32 + p[2] as u32
            };
            if tint_distances.is_empty() {
                tint_distances.push(distance(&before));
            }
            tint_distances.push(distance(&image));
            save(&image, directory, &format!("brush-accumulation-{pass}.png"))?;
        }
    }
    anyhow::ensure!(
        tint_distances.windows(2).all(|p| p[0] > p[1] + 2),
        "Repeated native brush input must visibly deepen the red weight overlay: {tint_distances:?}"
    );
    direct_deform(&mut harness, directory)?;
    inflation(&mut harness, directory)?;
    harness.view.update(&mut harness.cx, |view, cx| {
        view.editor
            .execute(EditorCommand::SetBrushSelectionSettings(
                BrushSelectionSettings {
                    diameter: BrushSelectionSettings::MAX_DIAMETER,
                    weight: 1.,
                    ..view.editor.brush_selection_settings()
                },
            ))
            .unwrap();
        cx.notify();
    });
    for scale in [UiScale::Standard, UiScale::Largest] {
        harness.cx.update(|cx| {
            cx.update_global::<Settings, _>(|settings, _| settings.set_ui_scale(scale).unwrap())
        });
        harness
            .cx
            .update(|cx| crate::theme::change(crate::theme::ColorTheme::Dark, cx));
        harness.view.update(&mut harness.cx, |_, cx| cx.notify());
        let image = harness.capture()?;
        save(
            &image,
            directory,
            &format!(
                "brush-selection-{}.png",
                scale.label().trim_end_matches('%')
            ),
        )?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            let bar = window.find("brush-selection-settings").bounds();
            let canvas = window.find("canvas-body").bounds();
            assert!(bar.left() >= canvas.left() && bar.right() <= canvas.right());
            assert!(bar.left() >= window.find("canvas-toolbar").bounds().right());
            for id in [
                "brush-selection-weight-label",
                "brush-selection-weight",
                "brush-selection-size-label",
                "brush-selection-size",
            ] {
                let field = window.find(id).bounds();
                assert!(field.left() >= bar.left() && field.right() <= bar.right());
                assert!(field.top() >= bar.top() && field.bottom() <= bar.bottom());
            }
            window.click("brush-selection-more-button", cx);
        })?;
        let image = harness.capture()?;
        save(
            &image,
            directory,
            &format!(
                "brush-selection-settings-{}.png",
                scale.label().trim_end_matches('%')
            ),
        )?;
        harness
            .cx
            .update_window(harness.window, |_, window, cx| window.press("escape", cx))?;
    }
    Ok(())
}

fn inflation(harness: &mut Harness, directory: &std::path::Path) -> anyhow::Result<()> {
    use zipzi_editor_core::BrushMode;
    let (start, before, history, old_weights) =
        harness.cx.update_window(harness.window, |_, window, cx| {
            window.click("brush-mode-inflate", cx);
            harness.view.update(cx, |view, _| {
                assert_eq!(view.editor.brush_mode(), BrushMode::Inflate);
                view.editor
                    .execute(EditorCommand::SetBrushSelectionSettings(
                        BrushSelectionSettings {
                            diameter: 80.,
                            weight: 1.,
                            hardness: 1.,
                            ..view.editor.brush_selection_settings()
                        },
                    ))
                    .unwrap();
            });
            let view = harness.view.read(cx);
            let overlay = view.editor.overlay();
            let p = overlay
                .vertices
                .iter()
                .find(|v| !v.selected)
                .unwrap()
                .position;
            let origin = window.find("canvas-body").bounds().origin;
            let start = origin + gpui_kit::point(gpui_kit::px(p.x + 20.), gpui_kit::px(p.y));
            let before = view.editor.document().unwrap().selected_vertex_positions();
            let history = view.editor.document().unwrap().history_lengths().0;
            let weights = overlay
                .selection_weights
                .unwrap()
                .points
                .iter()
                .map(|p| p.1)
                .collect::<Vec<_>>();
            window.dispatch_event(
                gpui_kit::MouseDownEvent {
                    position: start,
                    button: gpui_kit::MouseButton::Left,
                    modifiers: Default::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            (start, before, history, weights)
        })?;
    for _ in 0..10 {
        harness
            .cx
            .advance_clock(std::time::Duration::from_millis(50));
        harness.cx.update_window(harness.window, |_, window, cx| {
            window.simulate_next_frame(cx);
            window.render_frame(cx);
        })?;
    }
    let image = harness.capture()?;
    save(&image, directory, "brush-inflation-hold.png")?;
    let inflated = harness.view.read_with(&harness.cx, |view, _| {
        view.editor.document().unwrap().selected_vertex_positions()
    });
    anyhow::ensure!(
        inflated != before,
        "Holding without pointer motion must inflate"
    );
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.dispatch_event(
            gpui_kit::ModifiersChangedEvent {
                modifiers: gpui_kit::Modifiers {
                    shift: true,
                    ..Default::default()
                },
                capslock: Default::default(),
            }
            .to_platform_input(),
            cx,
        );
        assert!(
            harness
                .view
                .read(cx)
                .editor
                .overlay()
                .selection_brush
                .unwrap()
                .subtract
        );
    })?;
    for _ in 0..5 {
        harness
            .cx
            .advance_clock(std::time::Duration::from_millis(50));
        harness.cx.update_window(harness.window, |_, window, cx| {
            window.simulate_next_frame(cx);
            window.render_frame(cx);
        })?;
    }
    let image = harness.capture()?;
    save(&image, directory, "brush-inflation-shift-contract.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        let contracted = harness
            .view
            .read(cx)
            .editor
            .document()
            .unwrap()
            .selected_vertex_positions();
        assert_ne!(contracted, inflated);
        window.dispatch_event(
            gpui_kit::MouseUpEvent {
                position: start,
                button: gpui_kit::MouseButton::Left,
                modifiers: gpui_kit::Modifiers {
                    shift: true,
                    ..Default::default()
                },
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        let view = harness.view.read(cx);
        assert_eq!(
            view.editor.document().unwrap().history_lengths(),
            (history + 1, 0)
        );
        assert_eq!(
            view.editor
                .overlay()
                .selection_weights
                .unwrap()
                .points
                .iter()
                .map(|p| p.1)
                .collect::<Vec<_>>(),
            old_weights
        );
        assert!(!view.editor.gesture_active());
    })?;
    let image = harness.capture()?;
    save(&image, directory, "brush-inflation-complete.png")?;
    Ok(())
}

fn direct_deform(harness: &mut Harness, directory: &std::path::Path) -> anyhow::Result<()> {
    let modifiers = gpui_kit::Modifiers::default();
    let (end, original_weights, before) =
        harness.cx.update_window(harness.window, |_, window, cx| {
            window.click("brush-mode-deform", cx);
            assert_eq!(
                harness.view.read(cx).editor.brush_mode(),
                zipzi_editor_core::BrushMode::Deform
            );
            harness.view.update(cx, |view, _| {
                view.editor
                    .execute(EditorCommand::SetBrushSelectionSettings(
                        BrushSelectionSettings {
                            diameter: 80.,
                            weight: 0.8,
                            hardness: 1.,
                            ..view.editor.brush_selection_settings()
                        },
                    ))
                    .unwrap();
            });
            let overlay = harness.view.read(cx).editor.overlay();
            let original = overlay.selection_weights.unwrap().points;
            assert_eq!(original.len(), 1);
            let target = overlay
                .vertices
                .iter()
                .find(|v| !v.selected)
                .unwrap()
                .position;
            let center = zipzi_editor_core::ViewportPoint {
                x: overlay.vertices.iter().map(|v| v.position.x).sum::<f32>()
                    / overlay.vertices.len() as f32,
                y: overlay.vertices.iter().map(|v| v.position.y).sum::<f32>()
                    / overlay.vertices.len() as f32,
            };
            let dx = center.x - target.x;
            let dy = center.y - target.y;
            let length = dx.hypot(dy);
            let origin = window.find("canvas-body").bounds().origin;
            let start = origin
                + gpui_kit::point(
                    gpui_kit::px(target.x + dx / length * 12.),
                    gpui_kit::px(target.y + dy / length * 12.),
                );
            let end = start
                + gpui_kit::point(
                    gpui_kit::px(dx / length * 35.),
                    gpui_kit::px(dy / length * 35.),
                );
            let before = harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .selected_vertex_positions();
            window.dispatch_event(
                gpui_kit::MouseDownEvent {
                    button: gpui_kit::MouseButton::Left,
                    position: start,
                    modifiers,
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            assert_eq!(
                harness.view.read(cx).editor.overlay().active_gesture,
                Some(zipzi_editor_core::CanvasGestureKind::Move)
            );
            let temporary = harness
                .view
                .read(cx)
                .editor
                .overlay()
                .selection_weights
                .unwrap()
                .points;
            assert_eq!(temporary.len(), 1);
            assert!((temporary[0].1 - 0.8).abs() < 1e-6);
            window.render_frame(cx);
            (end, original, before)
        })?;
    let image = harness.capture()?;
    save(&image, directory, "brush-direct-deform-press.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_event(
            gpui_kit::MouseMoveEvent {
                position: end,
                pressed_button: Some(gpui_kit::MouseButton::Left),
                modifiers,
            }
            .to_platform_input(),
            cx,
        );
        assert_ne!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .selected_vertex_positions(),
            before
        );
        window.render_frame(cx);
    })?;
    let image = harness.capture()?;
    save(&image, directory, "brush-direct-deform-during.png")?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.dispatch_event(
            gpui_kit::MouseUpEvent {
                button: gpui_kit::MouseButton::Left,
                position: end,
                modifiers,
                click_count: 1,
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        let view = harness.view.read(cx);
        assert_ne!(
            view.editor.document().unwrap().selected_vertex_positions(),
            before
        );
        assert_eq!(
            view.editor.overlay().selection_weights.unwrap().points,
            original_weights
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    })?;
    let image = harness.capture()?;
    save(&image, directory, "brush-direct-deform-after.png")?;
    Ok(())
}
