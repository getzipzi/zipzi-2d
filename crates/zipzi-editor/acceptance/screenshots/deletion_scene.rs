//! Native layout checks for simple, scoped and dependent object deletion.
use super::super::{fixture, harness::Harness};
use super::common::save;
use crate::authoring::AuthoringContext;
use gpui_kit::test::TestWindowExt;
use std::time::{Duration, Instant};
use zipzi_editor_core::{Document, Editor, ObjectKind, ObjectRef, SceneObjectType};

fn settle(h: &mut Harness) -> anyhow::Result<()> {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        h.cx.run_until_parked();
        let done = h.cx.update_window(h.window, |_, window, cx| {
            window.render_frame(cx);
            window.try_find("tree-deletion-dialog").is_some()
                && window.try_find("deletion-planning").is_none()
        })?;
        if done {
            return Ok(());
        }
        anyhow::ensure!(Instant::now() < deadline, "Deletion preview did not finish");
        std::thread::sleep(Duration::from_millis(5));
    }
}

pub(crate) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    for locale in ["en", "zh-CN"] {
        for scene in ["simple", "mask", "texture", "scope"] {
            let mut h = Harness::with_size(1100., 800.)?;
            let (mut document, assets) = fixture::sample_document_with_two_layers()?;
            let object = if scene == "texture" {
                ObjectRef::new(ObjectKind::Texture, &document.resource_collection()?[0].id)
            } else {
                document = Document::new_blank()?;
                let kind = if scene == "scope" {
                    SceneObjectType::Part
                } else {
                    SceneObjectType::Mesh
                };
                let name = if locale == "en" { "Left eye" } else { "左眼" };
                let plan = document.plan_scene_object_creation(kind, name.into(), &[])?;
                let object = document
                    .execute_scene_object_creation(&plan)?
                    .suggested_selection[0]
                    .clone();
                if scene == "mask" {
                    let plan = document.plan_scene_object_creation(
                        SceneObjectType::Mesh,
                        if locale == "en" {
                            "Eye highlight"
                        } else {
                            "眼睛高光"
                        }
                        .into(),
                        &[],
                    )?;
                    let user = document
                        .execute_scene_object_creation(&plan)?
                        .suggested_selection[0]
                        .clone();
                    let address =
                        document.scene_field_address(&user.id, zipzi_sdk::SceneField::Masks)?;
                    let plan = document.plan_scene_field_patch(
                        &address,
                        zipzi_sdk::SceneFieldValue::Masks(vec![object.id.clone()]),
                        false,
                    )?;
                    document.execute_scene_authoring(&plan)?;
                } else if scene == "scope" {
                    let plan = document.plan_scene_object_creation(
                        SceneObjectType::Mesh,
                        if locale == "en" {
                            "Eye highlight"
                        } else {
                            "眼睛高光"
                        }
                        .into(),
                        &[object.clone()],
                    )?;
                    document.execute_scene_object_creation(&plan)?;
                }
                object
            };
            crate::i18n::activate(Some(locale));
            h.cx.update_window(h.window, |_, window, cx| {
                h.view.update(cx, |view, cx| {
                    view.replace_editor(Editor::from_document(document), cx);
                    view.refresh_document(window, cx);
                    let context = AuthoringContext::capture(&view.editor).unwrap();
                    view.run_object_deletion(context, &[object], window, cx);
                });
            })?;
            settle(&mut h)?;
            let image = h.capture()?;
            let dialog = h.dialog_image(&image)?;
            save(
                &dialog,
                directory,
                &format!("deletion-{scene}-{locale}.png"),
            )?;
            if scene == "simple" {
                h.cx.update_window(h.window, |_, window, cx| {
                    window.click("deletion-details", cx);
                    window.render_frame(cx);
                })?;
                let image = h.capture()?;
                let dialog = h.dialog_image(&image)?;
                save(
                    &dialog,
                    directory,
                    &format!("deletion-details-{locale}.png"),
                )?;
            }
            drop(h);
            drop(assets);
        }
    }
    crate::i18n::activate(Some("en"));
    Ok(())
}
