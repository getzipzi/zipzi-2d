use super::{Harness, document_fixture, setup_with_document};
use editor_core::{Document, EditorCommand};
use gpui_kit::component::WindowExt;
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};

fn open(harness: &Harness, cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(harness.window, |_, window, cx| {
        window.activate_window();
        window.click("new-parameter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
        assert!(window.try_find("new-parameter-form").is_some());
    })
    .unwrap();
}

fn fill(harness: &Harness, field: &'static str, text: &str, cx: &mut TestAppContext) {
    cx.update_window(harness.window, |_, window, cx| {
        window.click(field, cx);
        window.press("cmd-a", cx);
        window.input(text, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn new_parameter_empty_document_creates_once_and_supports_history(cx: &mut TestAppContext) {
    let harness = setup_with_document(
        cx,
        Document::new_blank().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    open(&harness, cx);
    // The initial keyboard focus goes to the name input.
    cx.update_window(harness.window, |_, window, cx| {
        window.press("cmd-a", cx);
        window.input("Exact parameter", cx);
    })
    .unwrap();
    fill(&harness, "new-parameter-default", "0.375", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let id = harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.parameters().len(), 1);
        assert_eq!(doc.parameters()[0].name, "Exact parameter");
        assert_eq!(doc.parameters()[0].default_value, 0.375);
        assert_eq!(doc.history_lengths(), (1, 0));
        doc.parameters()[0].id.clone()
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
        assert!(window.try_find(format!("parameter-value-{id}")).is_some());
        harness.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::Undo);
            view.complete_edit(result, cx);
        });
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.document().unwrap().parameters().is_empty())
    });
    cx.update_window(harness.window, |_, _, cx| {
        harness.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::Redo);
            view.complete_edit(result, cx);
        });
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters()[0].id, id)
    });
}

#[gpui_kit::test]
fn new_parameter_invalid_input_can_be_corrected_and_cancel_has_no_history(cx: &mut TestAppContext) {
    let harness = setup_with_document(
        cx,
        Document::new_blank().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    open(&harness, cx);
    fill(&harness, "new-parameter-minimum", "1", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("new-parameter-create", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.document().unwrap().parameters().is_empty());
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
    });
    fill(&harness, "new-parameter-minimum", "-1", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("new-parameter-create", cx)
    })
    .unwrap();
    cx.run_until_parked();
    let history = harness.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().history_lengths()
    });
    open(&harness, cx);
    fill(&harness, "new-parameter-id", "ParamNew", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("new-parameter-create", cx);
        assert!(window.has_active_dialog(cx));
        window.click("new-parameter-cancel", cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters().len(), 1);
        assert_eq!(view.editor.document().unwrap().history_lengths(), history);
    });
    open(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| window.press("escape", cx))
        .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        assert!(!window.has_active_dialog(cx))
    })
    .unwrap();
}

#[gpui_kit::test]
fn new_parameter_stale_dialog_never_writes_to_another_document(cx: &mut TestAppContext) {
    let harness = setup_with_document(
        cx,
        Document::new_blank().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    let original = harness
        .view
        .read_with(cx, |view, _| view.editor.active_id().unwrap());
    open(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.editor.new_document().unwrap();
            cx.notify();
        });
        window.render_frame(cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        assert!(window.has_active_dialog(cx));
        assert!(window.try_find("new-parameter-error").is_some());
        window.click("new-parameter-cancel", cx);
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.document().unwrap().parameters().is_empty())
    });
    harness
        .view
        .update(cx, |view, _| view.editor.activate(original).unwrap());
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.document().unwrap().parameters().is_empty())
    });
}

#[gpui_kit::test]
fn new_parameter_creation_reveals_last_row_of_a_long_list(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(128).unwrap();
    let harness = setup_with_document(cx, document, assets);
    open(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("new-parameter-create", cx);
    })
    .unwrap();
    cx.run_until_parked();
    let id = harness.view.read_with(cx, |view, _| {
        view.editor
            .document()
            .unwrap()
            .parameters()
            .last()
            .unwrap()
            .id
            .clone()
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find(format!("parameter-value-{id}")).is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn new_parameter_advanced_options_use_native_controls_without_submitting_the_dialog(
    cx: &mut TestAppContext,
) {
    use kasane_core::{Canvas, Vec2, document::CdiParameterGroup};
    use kasane_sdk::AuthoringSession;
    let assets = tempfile::tempdir().unwrap();
    let group_id = uuid::Uuid::new_v4().to_string();
    let mut session = AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::default(), 1.),
    )
    .unwrap();
    session
        .edit("group", None, |edit| {
            edit.create_parameter_group(CdiParameterGroup {
                id: group_id.clone(),
                runtime_id: "Face".into(),
                name: "Face".into(),
                parent_id: None,
                extensions: Default::default(),
            })
        })
        .unwrap();
    let saved = session
        .save_project(&assets.path().join("groups.kasane"), None)
        .unwrap();
    let harness = setup_with_document(cx, Document::open(&saved.manifest).unwrap(), assets);
    open(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("new-parameter-advanced", cx);
        window.click("new-parameter-kind", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.document().unwrap().parameters().is_empty())
    });
    cx.update_window(harness.window, |_, window, cx| {
        assert_eq!(
            window.find("new-parameter-kind").value(),
            Some("BlendShape")
        );
        window.click("new-parameter-repeat", cx);
    })
    .unwrap();
    fill(&harness, "new-parameter-precision", "4", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.within("new-parameter-group").click("input", cx);
        window.input("Face", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.document().unwrap().parameters().is_empty())
    });
    cx.update_window(harness.window, |_, window, cx| {
        assert_eq!(window.find("new-parameter-group").value(), Some("Face"));
        window.click("new-parameter-create", cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.parameters().len(), 1);
        let p = &doc.parameters()[0];
        assert_eq!(p.decimal_places, 4);
        assert_eq!(
            doc.parameter_visuals()[&p.id].kind,
            kasane_core::ParameterKind::BlendShape
        );
        assert!(doc.parameter_visuals()[&p.id].repeat);
        assert!(
            doc.object_index()
                .relations
                .iter()
                .any(|edge| edge.source.id == p.id && edge.target.id == group_id)
        );
    });
}
