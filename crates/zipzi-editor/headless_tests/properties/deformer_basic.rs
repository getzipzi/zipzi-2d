use super::*;
use zipzi_editor_core::{EditorCommand, ObjectKind, PropertyValue};

#[gpui_kit::test]
fn warp_and_rotation_basic_fields_use_the_flat_scene_object_layout(cx: &mut TestAppContext) {
    for number in [502, 501] {
        let (mut document, assets) =
            document_fixture::sample_document_with_scene_objects().unwrap();
        let object = document_fixture::object(ObjectKind::Deformer, number);
        document.navigate(object).unwrap();
        let h = setup_with_document(cx, document, assets);
        cx.update_window(h.window, |_, window, cx| {
            window.render_frame(cx);
            let mut previous = None;
            let mut left = None;
            for name in [
                "name",
                "runtime_id",
                "part_id",
                "parent_id",
                "opacity",
                "multiply",
                "screen",
                "enabled",
            ] {
                let bounds = window.find(format!("object-property-{name}")).bounds();
                if let Some(y) = previous {
                    assert!(
                        bounds.top() > y,
                        "{name} must follow the shared field order"
                    );
                }
                if let Some(x) = left {
                    assert_eq!(bounds.left(), x);
                }
                previous = Some(bounds.top());
                left = Some(bounds.left());
            }
            assert!(
                window
                    .try_find("property-group-deformer-identity")
                    .is_none()
            );
            assert!(
                window
                    .try_find("property-group-deformer-appearance")
                    .is_none()
            );
            assert!(window.try_find("property-color-multiply-reset").is_some());
            assert!(window.try_find("property-color-screen-reset").is_some());
        })
        .unwrap();
    }
}

#[gpui_kit::test]
fn deformer_color_resets_share_mesh_controls_and_remain_undoable(cx: &mut TestAppContext) {
    for number in [502, 501] {
        let (mut document, assets) =
            document_fixture::sample_document_with_scene_objects().unwrap();
        let object = document_fixture::object(ObjectKind::Deformer, number);
        document.navigate(object.clone()).unwrap();
        let h = setup_with_document(cx, document, assets);
        for (field, key, color) in [
            ("multiply", schema::deformer::MULTIPLY.key, [1.; 3]),
            ("screen", schema::deformer::SCREEN.key, [0.; 3]),
        ] {
            fill(&h, &format!("property-input-{field}"), "#804020", cx);
            cx.update_window(h.window, |_, window, cx| {
                window.press("enter", cx);
                window.render_frame(cx);
            })
            .unwrap();
            let before = h.view.read_with(cx, |view, _| {
                view.editor.document().unwrap().history_lengths().0
            });
            cx.update_window(h.window, |_, window, cx| {
                window.click(format!("property-color-{field}-reset"), cx);
                window.render_frame(cx);
            })
            .unwrap();
            h.view.read_with(cx, |view, _| {
                assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
                let doc = view.editor.document().unwrap();
                assert_eq!(
                    doc.property_values(&object).unwrap()[&key],
                    PropertyValue::Color(color)
                );
                assert_eq!(doc.history_lengths(), (before + 1, 0));
            });
            h.view.update(cx, |view, cx| {
                view.editor.execute(EditorCommand::Undo).unwrap();
                view.redraw(cx);
            });
            h.view.read_with(cx, |view, _| {
                assert_eq!(
                    view.editor
                        .document()
                        .unwrap()
                        .property_values(&object)
                        .unwrap()[&key],
                    PropertyValue::Color([128. / 255., 64. / 255., 32. / 255.])
                )
            });
        }
    }
}
