use super::{document_fixture, setup_with_document};
use gpui_kit::component::WindowExt;
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};
use zipzi_editor_core::{Document, EditorCommand, ObjectKind};

#[gpui_kit::test]
fn new_parameter_selects_properties_immediately_with_unique_ids_and_history(
    cx: &mut TestAppContext,
) {
    let h = setup_with_document(
        cx,
        Document::new_blank().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    for count in 1..=2 {
        cx.update_window(h.window, |_, window, cx| {
            window.click("new-parameter", cx);
            window.render_frame(cx);
            assert!(!window.has_active_dialog(cx));
            assert!(window.find("property-input-name").visible());
            assert!(window.try_find("new-parameter-form").is_none());
            assert!(window.try_find("property-delete-parameter").is_none());
        })
        .unwrap();
        h.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            let parameters = doc.parameters();
            assert_eq!(parameters.len(), count);
            assert_eq!(doc.history_lengths(), (count, 0));
            let selected = doc.context().inspected.as_ref().unwrap();
            assert_eq!(selected.kind, ObjectKind::Parameter);
            assert_eq!(selected.id, parameters.last().unwrap().id);
            assert_eq!(
                doc.parameter_draft(&parameters.last().unwrap().id)
                    .unwrap()
                    .runtime_id,
                if count == 1 { "ParamNew" } else { "ParamNew2" }
            );
        });
    }
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::Undo);
            view.complete_edit(result, cx);
        });
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters().len(), 1)
    });
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::Redo);
            view.complete_edit(result, cx);
        });
    })
    .unwrap();
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters().len(), 2)
    });
}

#[gpui_kit::test]
fn creation_reveals_last_parameter_and_reopens_hidden_properties(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(128).unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.dock.as_ref().unwrap().toggle(
                zipzi_editor_core::PanelKind::Properties,
                window,
                cx,
            );
        });
        window.click("new-parameter", cx);
        window.render_frame(cx);
        let doc = h.view.read(cx).editor.document().unwrap();
        let id = doc.parameters().last().unwrap().id.clone();
        assert!(window.find(format!("parameter-value-{id}")).visible());
        assert!(window.find("property-input-name").visible());
        assert!(!window.has_active_dialog(cx));
    })
    .unwrap();
}
