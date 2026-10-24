use super::{document_fixture, setup_with_document};
use gpui_kit::{AppContext as _, TestAppContext, px, size, test::TestWindowExt};
use zipzi_editor_core::{EditorCommand, NewParameter, ObjectKind, ObjectRef};

fn id(index: u128) -> String {
    uuid::Uuid::from_u128(index).to_string()
}

#[gpui_kit::test]
fn titlebar_toggle_folds_all_groups_and_keeps_root_parameters_visible(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    document.set_preview_parameter(&id(3), 0.7).unwrap();
    let before = document.parameters();
    let history = document.history_lengths();
    let selection = document.context().selections.clone();
    let harness = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(harness.window, size(px(1100.), px(1400.)));
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            (window
                .find("parameters-toggle-expansion")
                .bounds()
                .size
                .height
                - px(20.))
            .abs()
                <= px(0.5)
        );
        // A partly collapsed hierarchy expands fully on the next click.
        window.click(format!("parameter-group-toggle-{}", id(102)), cx);
        window.click("parameters-toggle-expansion", cx);
        for parameter in 1..=5 {
            assert!(
                window
                    .find(format!("parameter-name-{}", id(parameter)))
                    .visible()
            );
        }
        window.click("parameters-toggle-expansion", cx);
        for parameter in 1..=4 {
            assert!(
                window
                    .try_find(format!("parameter-name-{}", id(parameter)))
                    .is_none()
            );
        }
        assert!(window.find(format!("parameter-name-{}", id(5))).visible());
        assert_eq!(
            window
                .find(format!("parameter-group-{}", id(101)))
                .expanded(),
            Some(false)
        );
        window.click("parameters-toggle-expansion", cx);
        for parameter in 1..=5 {
            assert!(
                window
                    .find(format!("parameter-name-{}", id(parameter)))
                    .visible()
            );
        }
        assert_eq!(
            window
                .find(format!("parameter-group-{}", id(102)))
                .expanded(),
            Some(true)
        );
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let document = view.editor.document().unwrap();
        assert_eq!(document.parameters(), before);
        assert_eq!(document.history_lengths(), history);
        assert_eq!(document.context().selections, selection);
    });
    let (document, assets) = document_fixture::sample_document_with_parameters(1).unwrap();
    let flat = setup_with_document(cx, document, assets);
    cx.update_window(flat.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("parameters-toggle-expansion", cx);
        assert!(window.find(format!("parameter-name-{}", id(1))).visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn grouped_parameters_preserve_hierarchy_values_and_independent_collapse(cx: &mut TestAppContext) {
    let (mut document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    document.set_preview_parameter(&id(3), 0.7).unwrap();
    let before = document.parameters();
    let history = document.history_lengths();
    let harness = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(harness.window, size(px(1100.), px(1400.)));
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("parameter-groups-toggle").is_none());
        assert_eq!(
            window
                .find(format!("parameter-group-count-{}", id(101)))
                .value(),
            Some("3")
        );
        assert_eq!(
            window
                .find(format!("parameter-group-count-{}", id(102)))
                .value(),
            Some("2")
        );
        assert_eq!(
            window
                .find(format!("parameter-group-count-{}", id(104)))
                .value(),
            Some("0")
        );
        assert!(window.try_find("parameter-group-ungrouped").is_none());
        let row_y = |window: &mut gpui_kit::Window, index| {
            window
                .find(format!("parameter-name-{}", id(index)))
                .bounds()
                .top()
        };
        assert!(row_y(window, 1) < row_y(window, 3));
        assert!(row_y(window, 4) < row_y(window, 2));
        assert!(row_y(window, 2) < row_y(window, 5));
        let face = window
            .find(format!("parameter-group-toggle-{}", id(101)))
            .bounds();
        let eyes = window
            .find(format!("parameter-group-toggle-{}", id(102)))
            .bounds();
        assert!(eyes.top() > face.top());

        window.click(format!("parameter-group-toggle-{}", id(102)), cx);
        assert_eq!(
            window
                .find(format!("parameter-group-{}", id(102)))
                .expanded(),
            Some(false)
        );
        assert!(
            window
                .try_find(format!("parameter-name-{}", id(3)))
                .is_none()
        );
        assert!(
            window
                .try_find(format!("parameter-name-{}", id(1)))
                .is_some()
        );
        window.click(format!("parameter-group-toggle-{}", id(101)), cx);
        assert!(
            window
                .try_find(format!("parameter-group-{}", id(102)))
                .is_none()
        );
        window.click(format!("parameter-group-toggle-{}", id(101)), cx);
        assert_eq!(
            window
                .find(format!("parameter-group-{}", id(102)))
                .expanded(),
            Some(false)
        );

        window.click(format!("parameter-group-toggle-{}", id(102)), cx);
        assert!(
            window
                .try_find("parameter-group-toggle-ungrouped")
                .is_none()
        );
        assert!(
            window
                .try_find(format!("parameter-name-{}", id(5)))
                .is_some()
        );
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        assert_eq!(doc.parameters(), before);
        assert_eq!(doc.history_lengths(), history);
        assert!(!doc.modified());
    });
}

#[gpui_kit::test]
fn creating_inside_a_collapsed_nested_group_reveals_the_parameter_and_updates_counts(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(harness.window, size(px(1100.), px(1400.)));
    let created = cx
        .update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            window.click(format!("parameter-group-toggle-{}", id(102)), cx);
            window.click(format!("parameter-group-toggle-{}", id(101)), cx);
            harness.view.update(cx, |view, cx| {
                let result = view
                    .editor
                    .execute(EditorCommand::CreateParameter(NewParameter {
                        name: "New eye".into(),
                        group_id: Some(id(102)),
                        ..Default::default()
                    }));
                view.complete_edit(result, cx);
            });
            let panel = harness
                .view
                .read(cx)
                .dock
                .as_ref()
                .unwrap()
                .parameters
                .clone();
            let created = harness
                .view
                .read(cx)
                .editor
                .active_inspection()
                .unwrap()
                .object()
                .unwrap();
            let document = harness.view.read(cx).editor.active_id().unwrap();
            panel.update(cx, |panel, cx| {
                panel.locate_moved_object(document, &created, window, cx)
            });
            window.render_frame(cx);
            let created = harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .parameters()
                .last()
                .unwrap()
                .id
                .clone();
            assert!(window.find(format!("parameter-name-{created}")).visible());
            assert_eq!(
                window
                    .find(format!("parameter-group-{}", id(101)))
                    .expanded(),
                Some(true)
            );
            assert_eq!(
                window
                    .find(format!("parameter-group-{}", id(102)))
                    .expanded(),
                Some(true)
            );
            assert_eq!(
                window
                    .find(format!("parameter-group-count-{}", id(101)))
                    .value(),
                Some("4")
            );
            assert_eq!(
                window
                    .find(format!("parameter-group-count-{}", id(102)))
                    .value(),
                Some("3")
            );
            harness.view.update(cx, |view, cx| {
                let result = view.editor.execute(EditorCommand::Undo);
                view.complete_edit(result, cx);
            });
            created
        })
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(
            window
                .try_find(format!("parameter-name-{created}"))
                .is_none()
        );
        assert_eq!(
            window
                .find(format!("parameter-group-count-{}", id(101)))
                .value(),
            Some("3")
        );
        harness.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::Redo);
            view.complete_edit(result, cx);
        });
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find(format!("parameter-name-{created}")).visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn group_inspection_keeps_the_group_expanded_and_modal_blocks_group_controls(
    cx: &mut TestAppContext,
) {
    let (document, assets) = document_fixture::sample_document_with_parameter_groups().unwrap();
    let harness = setup_with_document(cx, document, assets);
    cx.update_window(harness.window, |_, window, cx| {
        window.click(format!("parameter-group-inspect-{}", id(101)), cx);
        harness.view.read_with(cx, |view, _| {
            assert_eq!(
                view.editor.document().unwrap().context().inspected,
                Some(ObjectRef::new(ObjectKind::ParameterGroup, id(101)))
            );
        });
        assert_eq!(
            window
                .find(format!("parameter-group-{}", id(101)))
                .expanded(),
            Some(true)
        );
        harness.view.update(cx, |view, cx| {
            view.modal = true;
            cx.notify();
        });
        window.render_frame(cx);
        window.click(format!("parameter-group-toggle-{}", id(101)), cx);
        assert_eq!(
            window
                .find(format!("parameter-group-{}", id(101)))
                .expanded(),
            Some(true)
        );
        assert!(window.try_find("parameter-groups-toggle").is_none());
    })
    .unwrap();
}
