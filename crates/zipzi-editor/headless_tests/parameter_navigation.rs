use super::{Harness, document_fixture, properties::fill, setup_with_document};
use gpui_kit::{AppContext as _, TestAppContext, px, size, test::TestWindowExt};
use zipzi_editor_core::{EditorCommand, ObjectKind as K, ObjectRef, WorkspaceKind};

fn id(n: u128) -> String {
    uuid::Uuid::from_u128(n).to_string()
}
fn object(kind: K, n: u128) -> ObjectRef {
    ObjectRef::new(kind, id(n))
}
fn fixture(cx: &mut TestAppContext) -> Harness {
    let (document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(h.window, size(px(1100.), px(1400.)));
    h
}
fn cursor(h: &Harness, cx: &TestAppContext) -> Option<ObjectRef> {
    h.view.read_with(cx, |view, cx| {
        view.dock
            .as_ref()
            .unwrap()
            .parameters
            .read(cx)
            .movement_object()
    })
}
fn inspected(h: &Harness, cx: &TestAppContext) -> Option<ObjectRef> {
    h.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().context().inspected.clone()
    })
}
fn click_group(h: &Harness, n: u128, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        window.click(format!("parameter-group-name-{}", id(n)), cx)
    })
    .unwrap();
}

#[gpui_kit::test]
fn folder_name_selects_and_arrow_only_folds_without_changing_preview_or_history(
    cx: &mut TestAppContext,
) {
    let h = fixture(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(format!("parameter-name-{}", id(3)), cx)
    })
    .unwrap();
    click_group(&h, 102, cx);
    assert_eq!(inspected(&h, cx), Some(object(K::ParameterGroup, 102)));
    cx.update_window(h.window, |_, window, cx| {
        assert_eq!(
            window
                .find(format!("parameter-group-{}", id(102)))
                .expanded(),
            Some(true)
        );
        window.click(format!("parameter-group-toggle-{}", id(101)), cx);
        assert!(
            window
                .try_find(format!("parameter-name-{}", id(3)))
                .is_none()
        );
    })
    .unwrap();
    assert_eq!(inspected(&h, cx), Some(object(K::ParameterGroup, 102)));
    assert_eq!(cursor(&h, cx), Some(object(K::ParameterGroup, 101)));
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.context().parameter_focus, Some(object(K::Parameter, 3)));
        assert_eq!(doc.history_lengths(), (0, 0));
        assert!(doc.parameters().iter().all(|p| p.value == 0.));
    });
}

#[gpui_kit::test]
fn arrows_browse_enter_inspects_and_left_right_follow_visible_hierarchy(cx: &mut TestAppContext) {
    let h = fixture(cx);
    click_group(&h, 101, cx);
    cx.update_window(h.window, |_, window, cx| window.press("down", cx))
        .unwrap();
    assert_eq!(cursor(&h, cx), Some(object(K::Parameter, 1)));
    assert_eq!(inspected(&h, cx), Some(object(K::ParameterGroup, 101)));
    cx.update_window(h.window, |_, window, cx| {
        window.press("enter", cx);
        window.press("down", cx); // Eyes
        window.press("left", cx); // collapse Eyes
        assert!(
            window
                .try_find(format!("parameter-name-{}", id(3)))
                .is_none()
        );
        window.press("right", cx); // expand Eyes
        window.press("right", cx); // first child
    })
    .unwrap();
    assert_eq!(cursor(&h, cx), Some(object(K::Parameter, 3)));
    assert_eq!(inspected(&h, cx), Some(object(K::Parameter, 1)));
    cx.update_window(h.window, |_, window, cx| {
        window.press("left", cx); // parent cursor
        window.press("enter", cx);
    })
    .unwrap();
    assert_eq!(inspected(&h, cx), Some(object(K::ParameterGroup, 102)));
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0))
    });
}

#[gpui_kit::test]
fn movement_uses_browsed_folder_instead_of_inspected_parameter_and_pseudo_group_is_noop(
    cx: &mut TestAppContext,
) {
    let h = fixture(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click(format!("parameter-name-{}", id(1)), cx);
        window.press("down", cx); // Eyes
        window.press("left", cx); // collapse Eyes
        window.press("down", cx); // Body
        window.press("alt-up", cx);
    })
    .unwrap();
    assert_eq!(cursor(&h, cx), Some(object(K::ParameterGroup, 103)));
    assert_eq!(inspected(&h, cx), Some(object(K::Parameter, 1)));
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.parameter_groups()[0].id, id(103));
        assert_eq!(doc.history_lengths(), (1, 0));
        assert_eq!(doc.context().parameter_focus, Some(object(K::Parameter, 1)));
    });
    cx.update_window(h.window, |_, window, cx| {
        window.press("end", cx); // Breath
        window.press("left", cx); // Ungrouped pseudo header
        window.press("alt-up", cx);
        window.press("enter", cx);
        window.press("left", cx); // collapse pseudo header
        assert!(
            window
                .try_find(format!("parameter-name-{}", id(5)))
                .is_none()
        );
    })
    .unwrap();
    assert_eq!(cursor(&h, cx), None);
    assert_eq!(inspected(&h, cx), Some(object(K::Parameter, 1)));
    h.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0))
    });
}

#[gpui_kit::test]
fn folder_activation_settles_old_properties_and_text_keys_do_not_navigate(cx: &mut TestAppContext) {
    let h = fixture(cx);
    click_group(&h, 101, cx);
    fill(&h, "property-input-name", "Edited Face", cx);
    cx.update_window(h.window, |_, window, cx| {
        window.press("down", cx);
        window.press("alt-down", cx);
    })
    .unwrap();
    assert_eq!(cursor(&h, cx), Some(object(K::ParameterGroup, 101)));
    click_group(&h, 104, cx);
    assert_eq!(inspected(&h, cx), Some(object(K::ParameterGroup, 104)));
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(
            doc.parameter_group_draft(&id(101)).unwrap().name,
            "Edited Face"
        );
        assert_eq!(
            doc.parameter_group_draft(&id(104)).unwrap().name,
            "Expressions"
        );
        assert_eq!(doc.history_lengths(), (1, 0));
    });
}

#[gpui_kit::test]
fn empty_folder_and_context_switch_reset_cursor_without_dropping_scene_selection(
    cx: &mut TestAppContext,
) {
    let (mut document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    let plan = document
        .plan_scene_object_creation(zipzi_editor_core::SceneObjectType::Mesh, "Mesh".into(), &[])
        .unwrap();
    let mesh = document
        .execute_scene_object_creation(&plan)
        .unwrap()
        .suggested_selection[0]
        .clone();
    document.select_object(mesh.clone()).unwrap();
    let h = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(h.window, size(px(1100.), px(1400.)));
    click_group(&h, 104, cx);
    assert_eq!(inspected(&h, cx), Some(object(K::ParameterGroup, 104)));
    cx.update_window(h.window, |_, window, cx| {
        window.press("left", cx);
        window.press("right", cx); // empty folder expands
        window.press("right", cx); // no child; does not enter next root
    })
    .unwrap();
    assert_eq!(cursor(&h, cx), Some(object(K::ParameterGroup, 104)));
    h.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor.document().unwrap().context().selection.as_ref(),
            Some(&mesh)
        )
    });
    cx.update_window(h.window, |_, window, cx| {
        window.press("home", cx); // browse Face without selecting
        h.view.update(cx, |view, cx| {
            view.editor.set_workspace(WorkspaceKind::Animation).unwrap();
            view.editor.set_workspace(WorkspaceKind::Modeling).unwrap();
            view.refresh_document(window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cursor(&h, cx), Some(object(K::ParameterGroup, 104)));
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.editor.new_document().unwrap();
            view.refresh_document(window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(cursor(&h, cx), None);
}

#[gpui_kit::test]
fn modal_blocks_keyboard_activation_and_folder_collapse(cx: &mut TestAppContext) {
    let h = fixture(cx);
    click_group(&h, 101, cx);
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            view.modal = true;
            cx.notify();
        });
        window.press("down", cx);
        window.press("enter", cx);
        window.press("left", cx);
        window.click(format!("parameter-group-toggle-{}", id(101)), cx);
        assert_eq!(
            window
                .find(format!("parameter-group-{}", id(101)))
                .expanded(),
            Some(true)
        );
    })
    .unwrap();
    assert_eq!(inspected(&h, cx), Some(object(K::ParameterGroup, 101)));
    assert_eq!(cursor(&h, cx), Some(object(K::ParameterGroup, 101)));
}

#[gpui_kit::test]
fn deleting_a_browsed_folder_reconciles_cursor_and_undo_does_not_revive_the_old_cursor(
    cx: &mut TestAppContext,
) {
    let h = fixture(cx);
    click_group(&h, 101, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.press("end", cx);
        window.press("up", cx); // Ungrouped
        window.press("up", cx); // Expressions
    })
    .unwrap();
    assert_eq!(cursor(&h, cx), Some(object(K::ParameterGroup, 104)));
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            let expected_version = view.editor.document().unwrap().parameter_model_revision();
            let result = view.editor.execute(EditorCommand::DeleteParameterGroup {
                id: id(104),
                expected_version,
            });
            view.complete_edit(result, cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(cursor(&h, cx), Some(object(K::ParameterGroup, 101)));
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::Undo);
            view.complete_edit(result, cx);
        });
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(cursor(&h, cx), Some(object(K::ParameterGroup, 101)));
    cx.update_window(h.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    assert_eq!(inspected(&h, cx), Some(object(K::ParameterGroup, 101)));
}

#[gpui_kit::test]
fn home_end_scroll_virtualized_rows_without_activating_them(cx: &mut TestAppContext) {
    let (document, assets) = document_fixture::sample_document_with_parameters(80).unwrap();
    let parameters = document.parameters();
    let first = parameters.first().unwrap().id.clone();
    let last = parameters.last().unwrap().id.clone();
    let h = setup_with_document(cx, document, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.click(format!("parameter-name-{first}"), cx);
        window.press("end", cx);
        assert!(window.find(format!("parameter-name-{last}")).visible());
    })
    .unwrap();
    assert_eq!(
        inspected(&h, cx),
        Some(ObjectRef::new(K::Parameter, &first))
    );
    assert_eq!(cursor(&h, cx), Some(ObjectRef::new(K::Parameter, &last)));
    cx.update_window(h.window, |_, window, cx| {
        window.press("enter", cx);
        window.press("home", cx);
        assert!(window.find(format!("parameter-name-{first}")).visible());
    })
    .unwrap();
    assert_eq!(inspected(&h, cx), Some(ObjectRef::new(K::Parameter, &last)));
    assert_eq!(cursor(&h, cx), Some(ObjectRef::new(K::Parameter, &first)));
    h.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.parameters(), parameters);
        assert_eq!(doc.history_lengths(), (0, 0));
    });
}
