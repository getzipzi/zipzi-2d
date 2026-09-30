use super::{document_fixture, setup, setup_with_document};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext};

#[gpui_kit::test]
fn mesh_properties_are_read_only_and_document_navigation_preserves_history(
    cx: &mut TestAppContext,
) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("object-property-name", cx);
        assert!(window.try_find("object-property-input").is_none());
        assert!(window.try_find("apply-object-property").is_none());
        assert!(window.try_find("cancel-object-property").is_none());
        window.click("inspect-document", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.context().inspected.as_ref().unwrap().kind,
            editor_core::ObjectKind::Document
        );
        assert_eq!(view.editor.selected_layer().unwrap().name, "Diamond");
        assert_eq!(doc.history_lengths(), (0, 0));
        assert!(!doc.modified());
    });
}

#[gpui_kit::test]
fn part_properties_use_the_same_read_only_inspector(cx: &mut TestAppContext) {
    let (mut document, assets) =
        document_fixture::sample_document_with_nonuniform_parameter_keys().unwrap();
    let part = document.object_index().bindings[0].target.clone();
    document.select_object(part.clone()).unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("object-property-name", cx);
        assert!(window.try_find("object-property-input").is_none());
        assert!(window.try_find("apply-object-property").is_none());
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.context().inspected.as_ref(), Some(&part));
        assert_eq!(doc.history_lengths(), (0, 0));
        assert!(!doc.modified());
    });
}
