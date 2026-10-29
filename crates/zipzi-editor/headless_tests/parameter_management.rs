use super::{document_fixture, setup_with_document};
use gpui_kit::component::WindowExt;
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};
use zipzi_editor_core::{CommandId, Document, EditorCommand, NewParameter, ObjectKind, ObjectRef};

fn command(h: &super::Harness, id: CommandId, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        h.view
            .update(cx, |view, cx| view.run_command(id, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
}
fn submit(h: &super::Harness, field: &str, value: &str, cx: &mut TestAppContext) {
    super::properties::fill(h, field, value, cx);
    cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn property_auto_commit_preserves_navigation_between_parameter_and_group(cx: &mut TestAppContext) {
    let mut doc = Document::new_blank().unwrap();
    let parameter = doc.create_parameter(NewParameter::default()).unwrap();
    let group = doc
        .create_parameter_group(
            zipzi_editor_core::ParameterGroupDraft {
                name: "Group".into(),
                runtime_id: "Group1".into(),
                parent_id: None,
            },
            doc.parameter_model_revision(),
        )
        .unwrap();
    let h = setup_with_document(cx, doc, tempfile::tempdir().unwrap());
    for (next, name) in [
        (
            ObjectRef::new(ObjectKind::Parameter, &parameter),
            "Renamed group",
        ),
        (
            ObjectRef::new(ObjectKind::ParameterGroup, &group),
            "Renamed parameter",
        ),
    ] {
        let (previous, history) = h.view.read_with(cx, |view, _| {
            (
                view.editor.active_inspection().unwrap().object().unwrap(),
                view.editor.document().unwrap().history_lengths().0,
            )
        });
        super::properties::fill(&h, "property-input-name", name, cx);
        cx.update_window(h.window, |_, window, cx| {
            h.view.update(cx, |view, cx| {
                view.editor
                    .execute(EditorCommand::Navigate(next.clone()))
                    .unwrap();
                view.redraw(cx);
            });
            window.render_frame(cx);
        })
        .unwrap();
        cx.run_until_parked();
        h.view.read_with(cx, |view, _| {
            let doc = view.editor.document().unwrap();
            assert_eq!(doc.object_index().name(&previous), name);
            assert_eq!(
                view.editor.active_inspection().unwrap().object(),
                Some(next)
            );
            assert_eq!(doc.history_lengths(), (history + 1, 0));
        });
    }
}

#[gpui_kit::test]
fn edit_command_uses_properties_and_delete_is_undoable_without_dialog(cx: &mut TestAppContext) {
    let mut doc = Document::new_blank().unwrap();
    let id = doc.create_parameter(NewParameter::default()).unwrap();
    let h = setup_with_document(cx, doc, tempfile::tempdir().unwrap());
    command(&h, CommandId::EditParameter, cx);
    submit(&h, "property-input-name", "Edited", cx);
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .parameter_draft(&id)
                .unwrap()
                .name,
            "Edited"
        );
    });
    command(&h, CommandId::DeleteParameter, cx);
    h.view.read_with(cx, |view, _| {
        assert!(view.editor.document().unwrap().parameters().is_empty())
    });
    cx.update_window(h.window, |_, window, cx| {
        assert!(!window.has_active_dialog(cx))
    })
    .unwrap();
    command(&h, CommandId::Undo, cx);
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .parameter_draft(&id)
                .unwrap()
                .name,
            "Edited"
        );
    });
}

#[gpui_kit::test]
fn group_properties_validate_identity_and_delete_keeps_contents(cx: &mut TestAppContext) {
    let h = setup_with_document(
        cx,
        Document::new_blank().unwrap(),
        tempfile::tempdir().unwrap(),
    );
    cx.update_window(h.window, |_, window, cx| {
        window.click("new-parameter-group-action", cx);
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
        assert!(window.try_find("property-edit-parameter").is_none());
    })
    .unwrap();
    submit(&h, "property-input-name", "Eyes", cx);
    submit(&h, "property-input-runtime_id", "EyeGroup", cx);
    submit(&h, "property-input-name", "   ", cx);
    let group = h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        let group = &doc.parameter_groups()[0];
        assert_eq!(group.name, "Eyes");
        assert_eq!(
            doc.parameter_group_draft(&group.id).unwrap().runtime_id,
            "EyeGroup"
        );
        assert_eq!(doc.history_lengths(), (3, 0));
        group.id.clone()
    });
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.editor
                .execute(EditorCommand::CreateParameter(NewParameter {
                    group_id: Some(group.clone()),
                    ..Default::default()
                }))
                .unwrap();
            view.editor
                .execute(EditorCommand::Navigate(ObjectRef::new(
                    ObjectKind::ParameterGroup,
                    &group,
                )))
                .unwrap();
            view.redraw(cx);
        });
        window.render_frame(cx);
        assert!(window.try_find("property-delete-parameter").is_none());
        assert!(!window.has_active_dialog(cx));
    })
    .unwrap();
    command(&h, CommandId::DeleteParameterGroup, cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert!(doc.parameter_groups().is_empty());
        assert_eq!(doc.parameters().len(), 1);
        assert!(doc.parameters()[0].group_id.is_none());
    });
    command(&h, CommandId::Undo, cx);
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.parameter_groups()[0].name, "Eyes");
        assert_eq!(
            doc.parameters()[0].group_id.as_deref(),
            Some(group.as_str())
        );
    });
}

#[gpui_kit::test]
fn referenced_parameter_delete_is_disabled_with_native_reason_without_mutating_document(
    cx: &mut TestAppContext,
) {
    let (mut doc, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let parameter = ObjectRef::new(ObjectKind::Parameter, &doc.parameters()[0].id);
    doc.navigate(parameter).unwrap();
    let before = doc.history_lengths();
    let h = setup_with_document(cx, doc, assets);
    command(&h, CommandId::DeleteParameter, cx);
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().parameters().len(), 1);
        assert_eq!(view.editor.document().unwrap().history_lengths(), before);
        assert!(
            !view
                .editor
                .command_available(view.editor.command(CommandId::DeleteParameter))
        );
        assert_eq!(
            view.editor
                .authoring_command_capability(CommandId::DeleteParameter)
                .unwrap(),
            zipzi_sdk::CapabilityState::Unavailable {
                code: "OBJECT_REFERENCED"
            }
        );
        assert!(view.edit_error.is_none());
    });
    cx.update_window(h.window, |_, window, cx| {
        assert!(!window.has_active_dialog(cx))
    })
    .unwrap();
}

#[gpui_kit::test]
fn parameter_type_and_group_are_edited_in_the_common_properties_panel(cx: &mut TestAppContext) {
    let (mut doc, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    let id = doc.create_parameter(NewParameter::default()).unwrap();
    let h = setup_with_document(cx, doc, assets);
    cx.simulate_window_resize(
        h.window,
        gpui_kit::size(gpui_kit::px(1100.), gpui_kit::px(1400.)),
    );
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.click("property-select-kind", cx);
        window.press("down", cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("property-select-group_id", cx);
        window.render_frame(cx);
        window.input("Body", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(150));
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    h.view.read_with(cx, |view, _| {
        let draft = view
            .editor
            .document()
            .unwrap()
            .parameter_draft(&id)
            .unwrap();
        assert_eq!(draft.kind, zipzi_core::ParameterKind::BlendShape);
        assert_eq!(
            draft.group_id.as_deref(),
            Some(uuid::Uuid::from_u128(103).to_string().as_str())
        );
    });
    command(&h, CommandId::Undo, cx);
    h.view.read_with(cx, |view, _| {
        assert!(
            view.editor
                .document()
                .unwrap()
                .parameter_draft(&id)
                .unwrap()
                .group_id
                .is_none()
        )
    });
}

#[gpui_kit::test]
fn menu_feedback_reports_referenced_parameter_without_opening_a_dialog(cx: &mut TestAppContext) {
    use gpui_kit::{MouseButton, test::ClickOptions};
    let (doc, assets) = document_fixture::sample_document_with_binding(true).unwrap();
    let parameter = doc.parameters()[0].id.clone();
    let history = doc.history_lengths();
    let h = setup_with_document(cx, doc, assets);
    h.view.update(cx, |view, cx| {
        view._frames = None;
        view.requests = Some(crate::preview::RequestSender::deferred());
        view.reset_viewport_images();
        view.clear_edit_error(cx);
    });
    cx.update_window(h.window, |_, window, cx| {
        window.click_with_options(
            format!("parameter-name-{parameter}"),
            ClickOptions::new().with_button(MouseButton::Right),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        let i = (0..16usize)
            .find(|&i| {
                window
                    .within("popup-menu")
                    .try_find(i)
                    .is_some_and(|item| item.label() == Some("Delete Parameter"))
            })
            .unwrap();
        window.within("popup-menu").click(i, cx);
        assert!(!window.has_active_dialog(cx));
        let view = h.view.read(cx);
        assert_eq!(view.message, crate::t!("authoring.referenced").as_ref());
        assert!(view.edit_error.is_none());
        assert_eq!(view.editor.document().unwrap().history_lengths(), history);
        assert_eq!(
            view.status_bar.read(cx).displayed_message(),
            Some((view.message.as_str(), false))
        );
    })
    .unwrap();
}
