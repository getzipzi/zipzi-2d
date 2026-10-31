use super::*;
use zipzi_editor_core::{EditorCommand, PropertyValue};

fn press(h: &super::super::Harness, id: &str, cx: &mut TestAppContext) {
    show(h, id, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(id.to_owned(), cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
}
#[gpui_kit::test]
fn deformer_properties_original_actions_and_auto_toggle_are_persistent_and_undoable(
    cx: &mut TestAppContext,
) {
    let (document, assets, object) = warp_grid_document();
    let h = setup_with_document(cx, document, assets);
    press(&h, "property-original-update", cx);
    h.view.read_with(cx, |view, _| {
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.deformer_original_info(&object).unwrap().source,
            Some(zipzi_core::DeformerOriginalSource::Manual)
        );
        assert_eq!(
            doc.property_values(&object).unwrap()[&schema::deformer::ORIGINAL_AUTO.key],
            PropertyValue::Bool(false)
        );
        assert_eq!(doc.history_lengths(), (1, 0));
    });
    press(&h, "property-toggle-original_auto_update", cx);
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .property_values(&object)
                .unwrap()[&schema::deformer::ORIGINAL_AUTO.key],
            PropertyValue::Bool(true)
        );
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
                .unwrap()[&schema::deformer::ORIGINAL_AUTO.key],
            PropertyValue::Bool(false)
        );
    });
}
#[gpui_kit::test]
fn deformer_properties_original_restore_reviews_shared_density_and_cancel_is_unchanged(
    cx: &mut TestAppContext,
) {
    let (mut document, assets, object) = warp_grid_document();
    document
        .set_object_property(&object, "warp.rows", serde_json::json!(3))
        .unwrap();
    let saved = document
        .save(&assets.path().join("original-density"))
        .unwrap();
    let mut document = zipzi_editor_core::Document::open(&saved.manifest).unwrap();
    document.navigate(object.clone()).unwrap();
    let h = setup_with_document(cx, document, assets);
    for cancel in [true, false] {
        press(&h, "property-original-restore", cx);
        cx.update_window(h.window, |_, window, cx| {
            assert!(window.try_find("original-restore-review").is_some());
            window.click(
                if cancel {
                    "original-restore-cancel"
                } else {
                    "original-restore-apply"
                },
                cx,
            );
            window.render_frame(cx);
            if !cancel {
                assert!(
                    window.try_find("original-restore-error").is_none(),
                    "{:?}",
                    window
                        .try_find("original-restore-error")
                        .and_then(|n| n.label().map(str::to_owned))
                );
                assert!(
                    window.try_find("original-restore-review").is_none(),
                    "Restore dialog did not submit"
                );
            }
        })
        .unwrap();
        cx.run_until_parked();
        h.view.read_with(cx, |view, _| {
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
            let doc = view.editor.document().unwrap();
            assert_eq!(
                doc.property_values(&object).unwrap()[&schema::deformer::ROWS.key],
                PropertyValue::Integer(if cancel { 3 } else { 1 })
            );
            assert_eq!(doc.history_lengths(), (usize::from(!cancel), 0));
        });
    }
}
