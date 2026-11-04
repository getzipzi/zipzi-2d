use super::super::harness::Harness;
use super::{common::save, properties_scene::show, warp_authoring_scene::fixture};
use gpui_kit::test::TestWindowExt;
use zipzi_editor_core::{
    DeformerAdjustment, Document, Editor, EditorCommand, ModelVector, ObjectKind, ObjectRef,
    ViewportPoint,
};

fn canvas_points(h: &Harness, object: &ObjectRef) -> Vec<zipzi_editor_core::ModelPoint> {
    h.view.read_with(&h.cx, |view, _| {
        view.modeling_frame()
            .unwrap()
            .canvas
            .geometry
            .controls
            .iter()
            .find(|c| c.object == *object)
            .unwrap()
            .points
            .clone()
    })
}
pub(super) fn run(directory: &std::path::Path) -> anyhow::Result<()> {
    for locale in ["en", "zh-CN", "ja"] {
        let mut h = Harness::new()?;
        crate::i18n::activate(Some(locale));
        let (document, _assets, object) = fixture(zipzi_core::WarpBezierEditMode::Local)?;
        h.cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |view, cx| {
                view.replace_editor(Editor::from_document(document), cx);
                view.editor
                    .execute(EditorCommand::SelectObject(object.clone()))
                    .unwrap();
                view.editor
                    .execute(EditorCommand::ToggleDeformerGrid)
                    .unwrap();
                view.editor.reset_view();
                view.editor.pan_by(ViewportPoint { x: -128., y: 128. });
                view.refresh_document(window, cx);
            })
        })?;
        h.capture()?;
        h.cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |view, cx| {
                let target = view.editor.warp_transform_target(&object).unwrap();
                view.editor
                    .execute(EditorCommand::AdjustWarp {
                        target,
                        adjustment: DeformerAdjustment {
                            translation: ModelVector { x: 3., y: 0. },
                            angle: 10.,
                            scale: 0.8,
                        },
                    })
                    .unwrap();
                view.refresh_document(window, cx);
            })
        })?;
        h.capture()?;
        h.cx.update_window(h.window, |_, window, cx| {
            show(window, "property-original-update", cx);
            window.click("property-original-update", cx);
            window.render_frame(cx);
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("deformer-original-warp-{locale}.png"),
        )?;
        let manual = canvas_points(&h, &object);
        h.cx.update_window(h.window, |_, window, cx| {
            for id in ["property-input-columns", "property-input-rows"] {
                show(window, id, cx);
                window.click(id, cx);
                window.press("cmd-a", cx);
                window.input("4", cx);
                window.press("enter", cx);
                window.render_frame(cx);
            }
        })?;
        h.capture()?;
        h.cx.update_window(h.window, |_, window, cx| {
            window.render_frame(cx);
        })?;
        h.capture()?;
        assert_eq!(canvas_points(&h, &object).len(), 25);
        h.cx.update_window(h.window, |_, window, cx| {
            show(window, "property-original-restore", cx);
            window.click("property-original-restore", cx);
            window.render_frame(cx);
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("deformer-original-review-{locale}.png"),
        )?;
        h.cx.update_window(h.window, |_, window, cx| {
            window.click("original-restore-cancel", cx);
            window.render_frame(cx);
        })?;
        h.capture()?;
        assert_eq!(canvas_points(&h, &object).len(), 25);
        h.cx.update_window(h.window, |_, window, cx| {
            show(window, "property-original-restore", cx);
            window.click("property-original-restore", cx);
            window.render_frame(cx);
        })?;
        h.capture()?;
        h.cx.update_window(h.window, |_, window, cx| {
            window.click("original-restore-apply", cx);
            window.render_frame(cx);
        })?;
        let pixels = h.capture()?;
        let restored = canvas_points(&h, &object);
        assert_eq!(manual.len(), restored.len());
        for (a, b) in manual.iter().zip(&restored) {
            assert!((a.x - b.x).hypot(a.y - b.y) < 0.001);
        }
        h.view.read_with(&h.cx, |view, _| {
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            assert_eq!(view.editor.document().unwrap().history_lengths(), (5, 0));
        });
        save(
            &pixels,
            directory,
            &format!("deformer-original-restored-{locale}.png"),
        )?;

        let (mut document, assets, _) = fixture(zipzi_core::WarpBezierEditMode::Local)?;
        let saved = document.save(&assets.path().join("rotation-original"))?;
        drop(document);
        let mut sdk = zipzi_sdk::AuthoringSession::new(
            &uuid::Uuid::new_v4().to_string(),
            zipzi_core::Canvas::new(256., 256., Default::default(), 1.),
        )?;
        sdk.open_project(&saved.path, None)?;
        let rotation = ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::from_u128(503).to_string());
        sdk.edit("Rotation original", None, |edit| {
            edit.create_transform(zipzi_core::Transform {
                id: rotation.id.clone(),
                name: "Rotation".into(),
                data: zipzi_core::TransformData::Rotation(zipzi_core::RotationTransform {
                    base_angle: 0.,
                    pose: zipzi_core::RotationPose {
                        origin: zipzi_core::PreciseVec2::new(128.123456789, 128.),
                        angle: 20.,
                        scale: 0.8,
                        ..Default::default()
                    },
                }),
                ..Default::default()
            })
        })?;
        sdk.save_project(&saved.path, None)?;
        let document = Document::open(&saved.path)?;
        let mut h = Harness::new()?;
        crate::i18n::activate(Some(locale));
        h.cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |view, cx| {
                view.replace_editor(Editor::from_document(document), cx);
                view.editor
                    .execute(EditorCommand::SelectObject(rotation.clone()))
                    .unwrap();
                view.refresh_document(window, cx);
            })
        })?;
        h.capture()?;
        h.cx.update_window(h.window, |_, window, cx| {
            show(window, "property-original-update", cx);
            window.click("property-original-update", cx);
            window.render_frame(cx);
        })?;
        save(
            &h.capture()?,
            directory,
            &format!("deformer-original-rotation-{locale}.png"),
        )?;
    }
    Ok(())
}
