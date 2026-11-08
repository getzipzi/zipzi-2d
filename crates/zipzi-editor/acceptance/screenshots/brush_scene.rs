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
            assert!(bar.size.width.as_f32() <= 320. * scale.factor());
            assert!((bar.size.height.as_f32() - 28. * scale.factor()).abs() < 1.);
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
