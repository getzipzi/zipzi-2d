use super::{document_fixture, properties::fill, setup, setup_with_document};
use editor_core::{
    CommandId, CommandSource, Document, EditorCommand, ObjectKind, property as schema,
};
use gpui_kit::{AppContext as _, TestAppContext, point, px, test::TestWindowExt};

#[gpui_kit::test]
fn locking_a_tree_object_settles_its_text_before_making_it_readonly(cx: &mut TestAppContext) {
    let h = setup(cx);
    let object = h.view.read_with(cx, |view, _| {
        view.editor.active_inspection().unwrap().object().unwrap()
    });
    fill(&h, "property-input-name", "Renamed before locking", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.hover(format!("mesh-item-Mesh:{}", object.id), cx);
        window.click(format!("mesh-lock-{}", object.id), cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert!(doc.object_state_index().is_locked(&object));
        assert_eq!(doc.object_index().name(&object), "Renamed before locking");
        assert_eq!(doc.history_lengths(), (2, 0));
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
    });
}

#[gpui_kit::test]
fn deleting_a_parameter_settles_text_once_and_undo_restores_the_committed_name(
    cx: &mut TestAppContext,
) {
    let (mut document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let parameter =
        editor_core::ObjectRef::new(ObjectKind::Parameter, &document.parameters()[0].id);
    document.navigate(parameter.clone()).unwrap();
    let h = setup_with_document(cx, document, assets);
    fill(&h, "property-input-name", "Committed before delete", cx);
    cx.update_window(h.window, |_, window, cx| {
        let panel = h.view.read(cx).dock.as_ref().unwrap().parameters.clone();
        panel.update(cx, |panel, cx| {
            panel.open_management(
                crate::panels::parameters::ParameterAction::Delete(parameter.id.clone()),
                window,
                cx,
            );
        });
    })
    .unwrap();
    cx.run_until_parked();
    h.view.update(cx, |view, _| {
        assert!(
            view.editor.document().unwrap().parameters().is_empty(),
            "{:?}",
            view.edit_error
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (2, 0));
        view.editor.execute(EditorCommand::Undo).unwrap();
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .object_index()
                .name(&parameter),
            "Committed before delete"
        );
    });
}

#[gpui_kit::test]
fn creation_focuses_name_and_keeps_rename_and_creation_in_separate_history(
    cx: &mut TestAppContext,
) {
    let h = setup_with_document(
        cx,
        Document::new_blank().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    cx.update_window(h.window, |_, window, cx| {
        window.click("new-parameter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        // Creation selected the name; typing replaces it without another click.
        window.input("Renamed", cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        let doc = h.view.read(cx).editor.document().unwrap();
        assert_eq!(doc.parameters()[0].name, "Renamed");
        assert_eq!(doc.history_lengths(), (2, 0));
        h.view.update(cx, |view, cx| {
            window.focus(&view.focus, cx);
            view.run_command(CommandId::Undo, window, cx);
        });
        window.render_frame(cx);
        let doc = h.view.read(cx).editor.document().unwrap();
        assert_eq!(doc.parameters().len(), 1);
        assert_ne!(doc.parameters()[0].name, "Renamed");
        h.view
            .update(cx, |view, cx| view.run_command(CommandId::Undo, window, cx));
        window.render_frame(cx);
        assert!(
            h.view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .parameters()
                .is_empty()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn all_parameter_command_sources_use_the_captured_insertion_context(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(3).unwrap();
    let h = setup_with_document(cx, document, assets);
    for source in [
        CommandSource::Direct,
        CommandSource::NativeMenu,
        CommandSource::Shortcut,
    ] {
        cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |view, cx| {
                let first = view.editor.document().unwrap().parameters()[0].id.clone();
                view.editor
                    .execute(EditorCommand::FocusParameter(first))
                    .unwrap();
                let command = view
                    .editor
                    .command(CommandId::CreateParameter)
                    .with_source(source);
                view.run_invocation(command, window, cx);
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(h.window, |_, window, cx| {
            window.render_frame(cx);
            let doc = h.view.read(cx).editor.document().unwrap();
            assert_eq!(
                doc.parameters()[1].id,
                doc.context().inspected.as_ref().unwrap().id
            );
        })
        .unwrap();
    }
    cx.update_window(h.window, |_, window, cx| {
        let (panel, anchor) = {
            let view = h.view.read(cx);
            (
                view.dock.as_ref().unwrap().parameters.clone(),
                view.editor.document().unwrap().parameters()[0].id.clone(),
            )
        };
        // This is the action captured by an ordinary row's context menu.
        panel.update(cx, |panel, cx| {
            panel.open_management(
                crate::panels::parameters::ParameterAction::CreateAt {
                    kind: ObjectKind::Parameter,
                    selection: vec![editor_core::ObjectRef::new(ObjectKind::Parameter, anchor)],
                },
                window,
                cx,
            )
        });
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.parameters()[1].id,
            doc.context().inspected.as_ref().unwrap().id
        );
    });
}

#[gpui_kit::test]
fn save_boundary_cancels_an_unfinished_number_gesture_and_late_release(cx: &mut TestAppContext) {
    use super::slide_number_input::{press, release, slide};
    let h = setup(cx);
    let object = h.view.read_with(cx, |view, _| {
        view.editor.active_inspection().unwrap().object().unwrap()
    });
    let (baseline, history) = h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        (
            doc.property_values(&object).unwrap()[&schema::mesh::OPACITY.key].clone(),
            doc.history_lengths(),
        )
    });
    let origin = cx
        .update_window(h.window, |_, window, cx| {
            window.activate_window();
            window.render_frame(cx);
            let origin = window
                .within("property-input-opacity")
                .find("slide-number-input")
                .bounds()
                .center();
            press(window, origin, cx);
            origin
        })
        .unwrap();
    cx.update_window(h.window, |_, window, cx| {
        slide(window, origin + point(px(-30.), px(0.)), false, cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        assert!(
            h.view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .has_pending_edit()
        );
        h.view.update(cx, |view, cx| {
            assert!(view.prepare_property_inputs(view.editor.active_id().unwrap(), window, cx));
        });
        release(window, origin + point(px(-30.), px(0.)), cx);
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.property_values(&object).unwrap()[&schema::mesh::OPACITY.key],
            baseline
        );
        assert_eq!(doc.history_lengths(), history);
        assert!(!doc.has_pending_edit());
    });
}

#[gpui_kit::test]
fn object_creation_commits_valid_text_and_recovers_invalid_text(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let object = editor_core::ObjectRef::new(ObjectKind::Parameter, &document.parameters()[0].id);
    document.navigate(object.clone()).unwrap();
    let h = setup_with_document(cx, document, assets);
    fill(&h, "property-input-name", "Before creation", cx);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.run_command(CommandId::CreateParameter, window, cx)
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let doc = h.view.read(cx).editor.document().unwrap();
        assert_eq!(
            doc.parameter_draft(&object.id).unwrap().name,
            "Before creation"
        );
        assert_eq!(doc.history_lengths(), (2, 0));
    })
    .unwrap();
    fill(&h, "property-input-minimum", "999", cx);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.run_command(CommandId::CreateParameter, window, cx)
        });
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let view = h.view.read(cx);
        assert!(view.edit_error.is_some());
        assert_eq!(view.editor.document().unwrap().parameters().len(), 3);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (3, 0));
        assert!(
            view.property_forms
                .entries
                .values()
                .all(|form| !form.dirty())
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn delayed_authoring_context_cannot_write_after_a_workspace_round_trip(cx: &mut TestAppContext) {
    let h = setup(cx);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            let context = crate::authoring::AuthoringContext::capture(&view.editor).unwrap();
            view.switch_workspace(editor_core::WorkspaceKind::Animation, window, cx);
            view.switch_workspace(editor_core::WorkspaceKind::Modeling, window, cx);
            let history = view.editor.document().unwrap().history_lengths();
            let result =
                view.execute_authoring(context, window, cx, |_, _| panic!("stale operation ran"));
            assert!(result.is_err());
            assert_eq!(view.editor.document().unwrap().history_lengths(), history);
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn redo_cancels_the_candidate_before_checking_history_availability(cx: &mut TestAppContext) {
    use super::slide_number_input::{press, release, slide};
    let h = setup(cx);
    let object = h.view.read_with(cx, |view, _| {
        view.editor.active_inspection().unwrap().object().unwrap()
    });
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.editor
                .execute(EditorCommand::PreviewOpacity(0.5))
                .unwrap();
            view.editor.execute(EditorCommand::FinishEdit).unwrap();
            view.editor.execute(EditorCommand::Undo).unwrap();
            view.redraw(cx);
        });
        window.activate_window();
        window.render_frame(cx);
    })
    .unwrap();
    let origin = cx
        .update_window(h.window, |_, window, cx| {
            let origin = window
                .within("property-input-opacity")
                .find("slide-number-input")
                .bounds()
                .center();
            press(window, origin, cx);
            origin
        })
        .unwrap();
    cx.update_window(h.window, |_, window, cx| {
        slide(window, origin + point(px(-30.), px(0.)), false, cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        assert!(
            h.view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .has_pending_edit()
        );
        h.view
            .update(cx, |view, cx| view.run_command(CommandId::Redo, window, cx));
        release(window, origin + point(px(-30.), px(0.)), cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.history_lengths(), (1, 0));
        assert_eq!(
            doc.property_values(&object).unwrap()[&schema::mesh::OPACITY.key],
            editor_core::PropertyValue::Float(0.5)
        );
        assert!(!doc.has_pending_edit());
    });
}

#[gpui_kit::test]
fn new_folder_inside_a_collapsed_folder_is_visible_and_escape_keeps_it(cx: &mut TestAppContext) {
    let mut document = Document::new_blank().unwrap();
    let parent = document
        .create_parameter_group(
            editor_core::ParameterGroupDraft {
                name: "Parent".into(),
                runtime_id: "Parent".into(),
                parent_id: None,
            },
            document.parameter_model_revision(),
        )
        .unwrap();
    let h = setup_with_document(cx, document, tempfile::tempdir().unwrap());
    cx.update_window(h.window, |_, window, cx| {
        window.click(format!("parameter-group-toggle-{parent}"), cx);
        window.click("new-parameter-group-action", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let doc = h.view.read(cx).editor.document().unwrap();
        let child = doc.context().inspected.as_ref().unwrap().id.clone();
        assert_eq!(
            doc.parameter_group_draft(&child).unwrap().parent_id,
            Some(parent)
        );
        assert!(window.find(format!("parameter-group-{child}")).visible());
        window.input("Draft", cx);
        window.press("escape", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.parameter_groups().len(), 2);
        assert_eq!(doc.history_lengths(), (2, 0));
    });
}
