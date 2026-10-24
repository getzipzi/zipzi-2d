//! Focused visual review of both attached toolbars, including fractional UI scale.
use super::super::harness::Harness;
use super::common::save;
use crate::{
    settings::{Settings, UiScale},
    theme::{self, ColorTheme},
};
use gpui_kit::{BorrowAppContext as _, test::TestWindowExt};
use zipzi_core::{Canvas, Transform, TransformData, Vec2, WarpTransform};
use zipzi_editor_core::{CommandId, Document, Editor, EditorCommand, ObjectKind, ObjectRef};

fn toolbar_document() -> anyhow::Result<(Document, tempfile::TempDir)> {
    let assets = tempfile::tempdir()?;
    let mut session = zipzi_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::default(), 1.),
    )?;
    session.edit("Toolbar fixture", None, |edit| {
        edit.create_transform(Transform {
            id: uuid::Uuid::from_u128(502).to_string(),
            name: "Warp".into(),
            data: TransformData::Warp(WarpTransform {
                bezier: None,
                rows: 1,
                columns: 1,
                quad: false,
                points: vec![
                    Vec2::new(32., 32.),
                    Vec2::new(224., 32.),
                    Vec2::new(32., 224.),
                    Vec2::new(224., 224.),
                ],
            }),
            ..Default::default()
        })
    })?;
    let saved = session.save_project(&assets.path().join("toolbar"), None)?;
    Ok((Document::open(&saved.manifest)?, assets))
}

pub(crate) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    for (mode, appearance) in [(ColorTheme::Dark, "dark"), (ColorTheme::Light, "light")] {
        let mut harness = Harness::with_size(1200., 820.)?;
        let (document, assets) = toolbar_document()?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.editor = Editor::from_document(document);
                view.editor
                    .execute(EditorCommand::SelectObject(ObjectRef::new(
                        ObjectKind::Deformer,
                        uuid::Uuid::from_u128(502).to_string(),
                    )))
                    .unwrap();
                view.refresh_document(window, cx);
            });
        })?;
        harness.capture()?;
        harness.cx.update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.run_command(CommandId::FitView, window, cx);
            });
        })?;
        for scale in UiScale::ALL {
            harness.cx.update(|cx| {
                cx.update_global::<Settings, _>(|settings, _| {
                    settings.set_ui_scale(scale).unwrap()
                });
                theme::change(mode, cx);
            });
            harness.view.update(&mut harness.cx, |_, cx| cx.notify());
            let frame = harness.capture()?;
            let (viewport, device_scale) =
                harness.cx.update_window(harness.window, |_, window, cx| {
                    window.render_frame(cx);
                    let viewport = window.find("canvas-body").bounds();
                    let main = window.find("canvas-toolbar").bounds();
                    let secondary = window.find("deformer-actions").bounds();
                    assert!((main.left() - viewport.left()).abs().as_f32() < 0.5);
                    assert!((secondary.top() - viewport.top()).abs().as_f32() < 0.5);
                    assert!((secondary.center().x - viewport.center().x).abs().as_f32() < 0.5);
                    assert!(
                        window.find("tool_select").bounds().size.height
                            > window.find("deformer-ancestors").bounds().size.height
                    );
                    (viewport, window.scale_factor())
                })?;
            let crop = image::imageops::crop_imm(
                &frame,
                (viewport.left().as_f32() * device_scale).round() as u32,
                (viewport.top().as_f32() * device_scale).round() as u32,
                (viewport.size.width.as_f32() * device_scale).floor() as u32,
                (viewport.size.height.as_f32().min(180.) * device_scale).floor() as u32,
            )
            .to_image();
            save(
                &crop,
                directory,
                &format!(
                    "toolbar-{appearance}-{}.png",
                    scale.label().trim_end_matches('%')
                ),
            )?;
        }
        drop(assets);
    }
    Ok(())
}
