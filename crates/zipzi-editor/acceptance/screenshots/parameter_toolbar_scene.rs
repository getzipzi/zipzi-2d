//! Review the actual icon slots at every UI scale, with enabled and disabled actions.
use super::super::{fixture, harness::Harness};
use super::common::save;
use crate::{
    settings::{Settings, UiScale},
    theme::{self, ColorTheme},
};
use gpui_kit::{BorrowAppContext as _, test::TestWindowExt};
use zipzi_editor_core::{Editor, NewParameter};

pub(crate) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    let mut harness = Harness::new()?;
    let (mut document, assets) = fixture::sample_document()?;
    let parameter = document.create_parameter(NewParameter {
        name: "Angle".into(),
        runtime_id: "Angle".into(),
        minimum: -1.,
        maximum: 1.,
        default_value: 0.,
        ..Default::default()
    })?;
    harness._assets = assets;
    harness.cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.replace_editor(Editor::from_document(document), cx);
            view.refresh_document(window, cx);
        });
    })?;
    harness.capture()?;
    harness.cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-name-{parameter}"), cx);
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
            for state in ["empty", "keyed"] {
                let frame = harness.capture()?;
                let crop = harness.cx.update_window(harness.window, |_, window, _| {
                    let standard = window.find("new-parameter").bounds();
                    let mut right = standard.right();
                    for id in [
                        "parameter-add-keyform",
                        "parameter-delete-keyform",
                        "parameter-add-two-keyforms",
                        "parameter-add-three-keyforms",
                        "parameter-delete-all-keyforms",
                    ] {
                        let bounds = window.find(id).bounds();
                        assert_eq!(bounds.size, standard.size, "{id}: standard button size");
                        assert_eq!(
                            bounds.center().y,
                            standard.center().y,
                            "{id}: vertical alignment"
                        );
                        right = bounds.right();
                    }
                    let device = window.scale_factor();
                    (
                        (standard.left().as_f32() * device).floor() as u32,
                        (standard.top().as_f32() * device).floor() as u32,
                        ((right - standard.left()).as_f32() * device).ceil() as u32,
                        (standard.size.height.as_f32() * device).ceil() as u32,
                    )
                })?;
                let toolbar =
                    image::imageops::crop_imm(&frame, crop.0, crop.1, crop.2, crop.3).to_image();
                save(
                    &toolbar,
                    directory,
                    &format!(
                        "parameter-toolbar-{appearance}-{}-{state}.png",
                        scale.label().trim_end_matches('%')
                    ),
                )?;
                harness.cx.update_window(harness.window, |_, window, cx| {
                    window.click(
                        if state == "empty" {
                            "parameter-add-three-keyforms"
                        } else {
                            "parameter-delete-all-keyforms"
                        },
                        cx,
                    );
                })?;
            }
        }
    }
    Ok(())
}
