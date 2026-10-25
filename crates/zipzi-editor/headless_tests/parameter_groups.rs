use super::{document_fixture, setup_with_document};
use gpui_kit::{AppContext as _, TestAppContext, px, size, test::TestWindowExt};
use zipzi_editor_core::{EditorCommand, NewParameter, ObjectKind, ObjectRef};

fn id(index: u128) -> String {
    uuid::Uuid::from_u128(index).to_string()
}

#[gpui_kit::test]
fn related_filter_tracks_selection_and_preserves_matching_group_ancestors(cx: &mut TestAppContext) {
    use zipzi_editor_core::{OrdinaryBindingAction as A, ParameterGroupDraft};

    let (mut document, assets) = document_fixture::sample_document_with_scene_objects().unwrap();
    let meshes: Vec<_> = document
        .layers()
        .iter()
        .map(|mesh| mesh.id.clone())
        .collect();
    let mut groups: Vec<String> = Vec::new();
    for (name, parent_id) in [("Parent", None), ("Nested", Some(0)), ("Unrelated", None)] {
        let group = document
            .create_parameter_group(
                ParameterGroupDraft {
                    name: name.into(),
                    runtime_id: name.into(),
                    parent_id: parent_id.map(|i| groups[i].clone()),
                },
                document.parameter_model_revision(),
            )
            .unwrap();
        groups.push(group);
    }
    let mut parameters = Vec::new();
    for (name, group_id) in [
        ("Direct", Some(groups[1].clone())),
        ("Inherited", None),
        ("Unrelated", Some(groups[2].clone())),
    ] {
        parameters.push(
            document
                .create_parameter(NewParameter {
                    name: name.into(),
                    runtime_id: name.into(),
                    group_id,
                    ..Default::default()
                })
                .unwrap(),
        );
    }
    for (target, parameter) in [
        (ObjectRef::new(ObjectKind::Mesh, &meshes[0]), &parameters[0]),
        (
            ObjectRef::new(ObjectKind::Deformer, id(501)),
            &parameters[1],
        ),
    ] {
        let plan = document.plan_ordinary_binding(&A::Create(target)).unwrap();
        document.execute_ordinary_binding(&plan).unwrap();
        let plan = document
            .plan_ordinary_binding(&A::AddAxis {
                binding: plan.binding,
                parameter: parameter.clone(),
            })
            .unwrap();
        document.execute_ordinary_binding(&plan).unwrap();
    }
    document
        .select_objects(vec![ObjectRef::new(ObjectKind::Mesh, &meshes[0])])
        .unwrap();
    let before = document.parameters();
    let history = document.history_lengths();
    let harness = setup_with_document(cx, document, assets);
    cx.simulate_window_resize(harness.window, size(px(1100.), px(1400.)));
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        for parameter in &parameters {
            assert!(window.find(format!("parameter-name-{parameter}")).visible());
        }
        window.click(format!("parameter-group-toggle-{}", groups[1]), cx);
        window.click("parameters-filter-related", cx);
        // A collapsed matching group and its ancestors survive the filter.
        for group in &groups[..2] {
            assert!(window.find(format!("parameter-group-{group}")).visible());
        }
        assert!(
            window
                .try_find(format!("parameter-group-{}", groups[2]))
                .is_none()
        );
        assert!(
            window
                .try_find(format!("parameter-name-{}", parameters[0]))
                .is_none()
        );
        assert!(
            window
                .find(format!("parameter-name-{}", parameters[1]))
                .visible()
        );
        assert!(
            window
                .try_find(format!("parameter-name-{}", parameters[2]))
                .is_none()
        );
        window.click(format!("parameter-group-toggle-{}", groups[1]), cx);
        assert!(
            window
                .find(format!("parameter-name-{}", parameters[0]))
                .visible()
        );
        // Inspecting a parameter retains the scene selection used by the filter.
        window.click(format!("parameter-name-{}", parameters[0]), cx);
        assert!(
            window
                .find(format!("parameter-name-{}", parameters[1]))
                .visible()
        );
    })
    .unwrap();

    for (selection, expected) in [
        (vec![ObjectRef::new(ObjectKind::Mesh, &meshes[1])], vec![1]),
        (
            meshes
                .iter()
                .map(|mesh| ObjectRef::new(ObjectKind::Mesh, mesh))
                .collect(),
            vec![0, 1],
        ),
        (Vec::new(), Vec::new()),
    ] {
        harness.view.update(cx, |view, cx| {
            let result = view.editor.execute(EditorCommand::SelectObjects(selection));
            view.complete_edit(result, cx);
        });
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            for (index, parameter) in parameters.iter().enumerate() {
                assert_eq!(
                    window
                        .try_find(format!("parameter-name-{parameter}"))
                        .is_some(),
                    expected.contains(&index),
                    "parameter {index}, expected visible indices {expected:?}"
                );
            }
            assert_eq!(
                window
                    .try_find(format!("parameter-group-{}", groups[0]))
                    .is_some(),
                expected.contains(&0)
            );
        })
        .unwrap();
    }
    cx.update_window(harness.window, |_, window, cx| {
        window.click("parameters-filter-related", cx);
        for parameter in &parameters {
            assert!(window.find(format!("parameter-name-{parameter}")).visible());
        }
        assert!(
            window
                .find(format!("parameter-group-{}", groups[2]))
                .visible()
        );
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        let document = view.editor.document().unwrap();
        assert_eq!(document.parameters(), before);
        assert_eq!(document.history_lengths(), history);
    });
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
        for group in [101, 102, 104] {
            assert!(
                window
                    .try_find(format!("parameter-group-count-{}", id(group)))
                    .is_none()
            );
        }
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
fn creating_inside_a_collapsed_nested_group_reveals_the_parameter(cx: &mut TestAppContext) {
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
        window.click(format!("parameter-group-name-{}", id(101)), cx);
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
