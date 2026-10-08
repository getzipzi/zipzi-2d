use super::{Harness, setup_with_document};
use editor_core::{Document, EditorCommand, NewParameter};
use gpui_kit::component::WindowExt;
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};

fn click(h: &Harness, id: &str, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.click(id.to_owned(), cx);
    })
    .unwrap();
    cx.run_until_parked();
}
fn fill(h: &Harness, id: &str, value: &str, cx: &mut TestAppContext) {
    click(h, id, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.press("cmd-a", cx);
        window.input(value, cx);
    })
    .unwrap();
    cx.run_until_parked();
}
fn blank(cx: &mut TestAppContext) -> Harness {
    cx.update(|cx| cx.set_reduce_motion(true));
    setup_with_document(
        cx,
        Document::new_blank().unwrap(),
        tempfile::tempdir().unwrap(),
    )
}

#[gpui_kit::test]
fn parameter_edit_and_delete_from_properties_are_undoable(cx: &mut TestAppContext) {
    let mut doc = Document::new_blank().unwrap();
    let id = doc.create_parameter(NewParameter::default()).unwrap();
    let h = setup_with_document(cx, doc, tempfile::tempdir().unwrap());
    cx.update(|cx| cx.set_reduce_motion(true));
    click(&h, "property-edit-parameter", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("edit-parameter-form").is_some());
    })
    .unwrap();
    fill(&h, "new-parameter-name", "Edited", cx);
    fill(&h, "new-parameter-default", "0.375", cx);
    click(&h, "new-parameter-create", cx);
    h.view.read_with(cx, |v, _| {
        let d = v.editor.document().unwrap();
        assert_eq!(d.parameter_draft(&id).unwrap().name, "Edited");
        assert_eq!(d.parameters()[0].default_value, 0.375);
        assert_eq!(d.history_lengths().0, 2);
    });
    click(&h, "property-delete-parameter", cx);
    click(&h, "parameter-management-submit", cx);
    h.view.read_with(cx, |v, _| {
        assert!(v.editor.document().unwrap().parameters().is_empty())
    });
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |v, cx| {
            let r = v.editor.execute(EditorCommand::Undo);
            v.complete_edit(r, cx);
        })
    })
    .unwrap();
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().parameters()[0].id, id)
    });
}

#[gpui_kit::test]
fn group_create_edit_delete_keep_parameter_and_have_single_history_entries(
    cx: &mut TestAppContext,
) {
    let h = blank(cx);
    click(&h, "new-parameter-group-action", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("parameter-group-parent").is_none());
    })
    .unwrap();
    fill(&h, "parameter-group-name", "Eyes", cx);
    click(&h, "parameter-management-submit", cx);
    let group = h.view.read_with(cx, |v, _| {
        let d = v.editor.document().unwrap();
        assert_eq!(d.parameter_groups()[0].name, "Eyes");
        assert_eq!(d.history_lengths().0, 1);
        d.parameter_groups()[0].id.clone()
    });
    click(&h, "property-edit-parameter", cx);
    fill(&h, "parameter-group-name", "Face", cx);
    click(&h, "parameter-management-submit", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(
            v.editor.document().unwrap().parameter_groups()[0].name,
            "Face"
        )
    });
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |v, cx| {
            v.editor
                .execute(EditorCommand::CreateParameter(NewParameter {
                    group_id: Some(group.clone()),
                    ..Default::default()
                }))
                .unwrap();
            v.editor
                .execute(EditorCommand::Navigate(editor_core::ObjectRef::new(
                    editor_core::ObjectKind::ParameterGroup,
                    &group,
                )))
                .unwrap();
            v.redraw(cx);
        })
    })
    .unwrap();
    click(&h, "property-delete-parameter", cx);
    click(&h, "parameter-management-submit", cx);
    h.view.read_with(cx, |v, _| {
        let d = v.editor.document().unwrap();
        assert!(d.parameter_groups().is_empty());
        assert_eq!(d.parameters().len(), 1);
        assert!(d.parameters()[0].group_id.is_none());
        assert_eq!(d.history_lengths().0, 4);
    });
}

#[gpui_kit::test]
fn stale_parameter_form_keeps_input_until_explicit_reload(cx: &mut TestAppContext) {
    let mut doc = Document::new_blank().unwrap();
    let id = doc.create_parameter(NewParameter::default()).unwrap();
    let h = setup_with_document(cx, doc, tempfile::tempdir().unwrap());
    cx.update(|cx| cx.set_reduce_motion(true));
    click(&h, "property-edit-parameter", cx);
    fill(&h, "new-parameter-name", "Stale draft", cx);
    cx.update_window(h.window, |_, _, cx| {
        h.view.update(cx, |v, cx| {
            let doc = v.editor.document().unwrap();
            let mut draft = doc.parameter_draft(&id).unwrap();
            draft.name = "External edit".into();
            let revision = doc.parameter_model_revision();
            v.editor
                .execute(EditorCommand::UpdateParameter {
                    id: id.clone(),
                    draft,
                    expected_version: revision,
                })
                .unwrap();
            v.redraw(cx);
        })
    })
    .unwrap();
    click(&h, "new-parameter-create", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(
            v.editor.document().unwrap().parameters()[0].name,
            "External edit"
        )
    });
    cx.update_window(h.window, |_, window, cx| {
        assert!(window.has_active_dialog(cx))
    })
    .unwrap();
    click(&h, "parameter-reload", cx);
    fill(&h, "new-parameter-name", "Reloaded edit", cx);
    click(&h, "new-parameter-create", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(
            v.editor.document().unwrap().parameters()[0].name,
            "Reloaded edit"
        )
    });
}

#[gpui_kit::test]
fn group_cancel_does_not_create_history(cx: &mut TestAppContext) {
    let h = blank(cx);
    click(&h, "new-parameter-group-action", cx);
    fill(&h, "parameter-group-name", "Cancelled", cx);
    click(&h, "parameter-management-cancel", cx);
    h.view.read_with(cx, |v, _| {
        let d = v.editor.document().unwrap();
        assert!(d.parameter_groups().is_empty());
        assert_eq!(d.history_lengths().0, 0);
    });
}
