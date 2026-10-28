//! Compare linked and ordinary parameter tracks in the native renderer.
use super::super::{fixture, harness::Harness};
use super::common::save;
use crate::{
    settings::{Settings, UiScale},
    theme::{self, ColorTheme},
};
use gpui_kit::{BorrowAppContext as _, test::TestWindowExt};
use zipzi_editor_core::{Editor, NewParameter, ObjectKind, ObjectRef, ParameterKeyformAction};

pub(crate) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    let (mut document, assets) = fixture::sample_document_with_scene_objects()?;
    document.select_object(ObjectRef::new(
        ObjectKind::Deformer,
        uuid::Uuid::from_u128(501).to_string(),
    ))?;
    let mut parameters = Vec::new();
    for axis in ["X", "Y", "Z"] {
        let id = document.create_parameter(NewParameter {
            name: format!("Angle_{axis}"),
            runtime_id: format!("ParamAngle{axis}"),
            minimum: -30.,
            maximum: 30.,
            default_value: 0.,
            ..Default::default()
        })?;
        document.edit_selected_parameter_keyforms(
            &id,
            ParameterKeyformAction::AddEndpointsAndMidpoint,
        )?;
        parameters.push(id);
    }
    // Select an indirectly driven mesh so the existing navigation button is
    // visible alongside both linked and ordinary tracks.
    let mesh = document
        .object_index()
        .parameter(&parameters[0])
        .indirect
        .iter()
        .find(|path| path.target.kind == ObjectKind::Mesh)
        .expect("ancestor binding influences a mesh")
        .target
        .clone();
    document.select_object(mesh)?;
    document.toggle_parameter_link(&parameters[0])?;
    harness._assets = assets;
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.replace_editor(Editor::from_document(document), cx);
            view.refresh_document(window, cx);
        });
    })?;
    for scale in UiScale::ALL {
        for (mode, appearance) in [(ColorTheme::Dark, "dark"), (ColorTheme::Light, "light")] {
            harness.cx.update(|cx| {
                cx.update_global::<Settings, _>(|settings, _| {
                    settings.set_ui_scale(scale).unwrap()
                });
                theme::change(mode, cx);
            });
            harness.view.update(&mut harness.cx, |_, cx| cx.notify());
            let frame = harness.capture()?;
            let crop = harness.cx.update_window(harness.window, |_, window, _| {
                let top = window
                    .find(format!("parameter-row-{}", parameters[0]))
                    .bounds();
                let bottom = window
                    .find(format!("parameter-row-{}", parameters[2]))
                    .bounds();
                let pad = window
                    .find(format!(
                        "parameter-pad-top-{}-{}",
                        parameters[0], parameters[1]
                    ))
                    .bounds();
                let ordinary = window
                    .within(format!("parameter-row-{}", parameters[2]))
                    .find("slider-bar-container")
                    .bounds();
                assert_eq!(pad.left(), ordinary.left(), "track left alignment");
                assert_eq!(pad.right(), ordinary.right(), "track right alignment");
                for parameter in &parameters {
                    let button = window.find(format!("parameter-indirect-{parameter}"));
                    assert!(
                        button.visible(),
                        "indirect influence navigation remains visible"
                    );
                }
                let device = window.scale_factor();
                (
                    (top.left().as_f32() * device).floor() as u32,
                    (top.top().as_f32() * device).floor() as u32,
                    (top.size.width.as_f32() * device).ceil() as u32,
                    ((bottom.bottom() - top.top()).as_f32() * device).ceil() as u32,
                )
            })?;
            let rows = image::imageops::crop_imm(&frame, crop.0, crop.1, crop.2, crop.3).to_image();
            save(
                &rows,
                directory,
                &format!(
                    "parameter-pad-{appearance}-{}.png",
                    scale.label().trim_end_matches('%')
                ),
            )?;
        }
    }
    Ok(())
}
