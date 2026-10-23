use super::support;
use editor_core::{Document, ObjectKind, ObjectRef, ParameterDestination, ReorderPosition};
use uuid::Uuid;

fn id(value: u128) -> String {
    Uuid::from_u128(value).to_string()
}

fn members(document: &Document, group: Option<u128>) -> Vec<String> {
    document
        .parameters()
        .into_iter()
        .filter(|parameter| parameter.group_id == group.map(id))
        .map(|parameter| parameter.id)
        .collect()
}

#[test]
fn deformation_self_reorder_is_a_noop_and_preserves_redo() -> anyhow::Result<()> {
    let (mut document, _assets) = support::sample_document_with_two_layers()?;
    let original = document.deformer_tree();
    let first = original[0].children[0].reference().unwrap();
    let second = original[0].children[1].reference().unwrap();
    document.reorder_tree(true, &second, &first, ReorderPosition::Before)?;
    document.undo()?;
    let history = document.history_lengths();
    for position in [ReorderPosition::Before, ReorderPosition::After] {
        document.reorder_tree(true, &first, &first, position)?;
        assert_eq!(document.deformer_tree(), original);
        assert_eq!(document.history_lengths(), history);
    }
    Ok(())
}

#[test]
fn hierarchy_order_is_independent_of_other_tree_and_rendering_and_survives_history_and_save()
-> anyhow::Result<()> {
    let (mut document, _assets) = support::sample_document_with_two_layers()?;
    let original = document.mesh_tree();
    let deformation = document.deformer_tree();
    let children = &original[0].children;
    let first = children[0].reference().unwrap();
    let second = children[1].reference().unwrap();
    let before = document.snapshot()?.viewport_frame;
    document.reorder_tree(false, &second, &first, ReorderPosition::Before)?;
    assert_eq!(document.mesh_tree()[0].children[0].id, second.id);
    assert_eq!(document.deformer_tree(), deformation);
    let after = document.snapshot()?.viewport_frame;
    assert_eq!(before.drawables.len(), after.drawables.len());
    for (a, b) in before.drawables.iter().zip(&after.drawables) {
        assert_eq!(a.id, b.id);
        assert_eq!(a.positions, b.positions);
        assert_eq!(a.render_order, b.render_order);
    }
    assert_eq!(document.history_lengths(), (1, 0));
    document.reorder_tree(true, &second, &first, ReorderPosition::Before)?;
    assert_eq!(document.deformer_tree()[0].children[0].id, second.id);
    assert_eq!(document.mesh_tree()[0].children[0].id, second.id);
    document.undo()?;
    assert_eq!(document.deformer_tree(), deformation);
    document.undo()?;
    assert_eq!(document.mesh_tree(), original);
    document.redo()?;
    let output = tempfile::tempdir()?;
    let saved = document.save(&output.path().join("sorted.kasane"))?;
    let reopened = Document::open(&saved.manifest)?;
    assert_eq!(reopened.mesh_tree(), document.mesh_tree());
    assert_eq!(reopened.deformer_tree(), deformation);
    Ok(())
}

#[test]
fn parameter_moves_and_group_order_preserve_values_identity_and_nested_membership()
-> anyhow::Result<()> {
    let (mut document, _assets) = support::sample_document_with_parameter_groups()?;
    document.set_preview_parameter(&id(3), 0.7)?;
    let context = document.context().selections.clone();
    document.move_parameter(
        &id(4),
        ParameterDestination::Parameter {
            id: id(3),
            position: ReorderPosition::Before,
        },
    )?;
    assert_eq!(members(&document, Some(102)), vec![id(4), id(3)]);
    document.move_parameter(&id(3), ParameterDestination::Group(Some(id(104))))?;
    assert_eq!(members(&document, Some(102)), vec![id(4)]);
    assert_eq!(members(&document, Some(104)), vec![id(3)]);
    assert_eq!(
        document
            .parameters()
            .iter()
            .find(|p| p.id == id(3))
            .unwrap()
            .value,
        0.7
    );
    assert_eq!(document.context().selections, context);
    document.reorder_parameter_group(&id(104), &id(101), ReorderPosition::Before)?;
    assert_eq!(document.parameter_groups()[0].id, id(104));
    assert_eq!(
        document
            .parameter_groups()
            .iter()
            .find(|g| g.id == id(102))
            .unwrap()
            .parent_id,
        Some(id(101))
    );
    assert_eq!(document.history_lengths(), (3, 0));
    let output = tempfile::tempdir()?;
    let saved = document.save(&output.path().join("sorted.kasane"))?;
    let reopened = Document::open(&saved.manifest)?;
    assert_eq!(members(&reopened, Some(104)), vec![id(3)]);
    assert_eq!(reopened.parameter_groups(), document.parameter_groups());
    document.undo()?;
    document.undo()?;
    assert_eq!(members(&document, Some(102)), vec![id(4), id(3)]);
    document.undo()?;
    assert_eq!(members(&document, Some(102)), vec![id(3), id(4)]);
    document.redo()?;
    assert_eq!(members(&document, Some(102)), vec![id(4), id(3)]);
    Ok(())
}

#[test]
fn invalid_moves_and_noops_preserve_redo_and_document_identity() -> anyhow::Result<()> {
    let (mut document, _assets) = support::sample_document_with_parameter_groups()?;
    document.move_parameter(
        &id(4),
        ParameterDestination::Parameter {
            id: id(3),
            position: ReorderPosition::Before,
        },
    )?;
    document.undo()?;
    let history = document.history_lengths();
    let before = document.parameters();
    // Adjacent visible siblings can be separated by another group's entries.
    document.move_parameter(
        &id(4),
        ParameterDestination::Parameter {
            id: id(3),
            position: ReorderPosition::After,
        },
    )?;
    document.reorder_parameter_group(&id(101), &id(103), ReorderPosition::Before)?;
    assert!(
        document
            .move_parameter(&id(3), ParameterDestination::Group(Some(id(999))))
            .is_err()
    );
    assert!(
        document
            .reorder_parameter_group(&id(101), &id(102), ReorderPosition::Before)
            .is_err()
    );
    document.move_parameter(
        &id(3),
        ParameterDestination::Parameter {
            id: id(3),
            position: ReorderPosition::Before,
        },
    )?;
    assert_eq!(document.parameters(), before);
    assert_eq!(document.history_lengths(), history);
    let plan = document.plan_palette_move(editor_core::PaletteMove {
        projection: editor_core::PaletteProjection::Parameters,
        sources: vec![ObjectRef::new(ObjectKind::Parameter, id(3))],
        target: None,
        position: editor_core::PalettePosition::Inside,
    })?;
    let mut editor = crate::support::editor_from_document(document);
    let first = editor.active_id().unwrap();
    editor.new_document()?;
    assert!(editor.execute_palette_move(first, &plan).is_err());
    // Missing or differently typed tree targets cannot become a write destination.
    let (mut document, _assets) = support::sample_document_with_two_layers()?;
    let source = document.mesh_tree()[0].children[0].reference().unwrap();
    assert!(
        document
            .reorder_tree(
                false,
                &source,
                &ObjectRef::new(ObjectKind::Part, source.id.clone()),
                ReorderPosition::Before
            )
            .is_err()
    );
    assert!(!document.modified());
    Ok(())
}

#[test]
fn sorting_imported_cdi_keeps_unresolved_entries_extensions_and_omitted_parameters()
-> anyhow::Result<()> {
    use kasane_core::{
        Canvas, Parameter, Vec2,
        document::{CdiParameterEntry, DisplayInfo, DisplayInfoOrigin},
    };
    use kasane_sdk::AuthoringSession;
    let mut session =
        AuthoringSession::new(&id(900), Canvas::new(100., 100., Vec2::default(), 1.))?;
    let unresolved = CdiParameterEntry::Unresolved {
        runtime_id: "Missing".into(),
        name: "Missing".into(),
        group_id: None,
        extensions: [("note".into(), serde_json::json!({"preserved": true}))].into(),
    };
    session.edit("Fixture", None, |edit| {
        for index in 1..=3 {
            edit.create_parameter(Parameter {
                id: id(index),
                runtime_id: format!("Param{index}"),
                ..Default::default()
            })?;
        }
        edit.replace_display_info(DisplayInfo {
            origin: DisplayInfoOrigin::Imported,
            parameters: Some(vec![
                CdiParameterEntry::Resolved {
                    parameter_id: id(2),
                    group_id: None,
                    extensions: [("entryNote".into(), serde_json::json!("kept"))].into(),
                },
                unresolved.clone(),
                CdiParameterEntry::Resolved {
                    parameter_id: id(1),
                    group_id: None,
                    extensions: Default::default(),
                },
            ]),
            ..Default::default()
        })
    })?;
    let output = tempfile::tempdir()?;
    let saved = session.save_project(&output.path().join("imported.kasane"), None)?;
    let mut document = Document::open(&saved.manifest)?;
    assert_eq!(members(&document, None), vec![id(2), id(1), id(3)]);
    document.move_parameter(
        &id(3),
        ParameterDestination::Parameter {
            id: id(2),
            position: ReorderPosition::Before,
        },
    )?;
    assert_eq!(members(&document, None), vec![id(3), id(2), id(1)]);
    let saved = document.save(&saved.manifest)?;
    session.open_project(&saved.manifest, None)?;
    let info = session.document().display_info();
    assert_eq!(info.origin, DisplayInfoOrigin::Imported);
    assert_eq!(info.parameters.as_ref().unwrap()[1], unresolved);
    assert!(info.parameters.as_ref().unwrap().iter().any(|entry| matches!(entry, CdiParameterEntry::Resolved { parameter_id, extensions, .. } if parameter_id == &id(2) && extensions["entryNote"] == "kept")));
    assert_eq!(
        members(&Document::open(&saved.manifest)?, None),
        vec![id(3), id(2), id(1)]
    );
    Ok(())
}

#[test]
fn part_moves_keep_subtrees_deformation_and_history_atomic() -> anyhow::Result<()> {
    use editor_core::OrganizationDestination as Destination;
    let (mut document, assets) = support::sample_document_with_parts()?;
    let original = document.mesh_tree();
    let deformer = document.deformer_tree();
    let part = |value| ObjectRef::new(ObjectKind::Part, id(value));
    let face = &original[0];
    let eyes = face
        .children
        .iter()
        .find(|node| node.id == id(103))
        .unwrap()
        .clone();
    let mesh = face
        .children
        .iter()
        .find(|node| node.kind == editor_core::TreeNodeKind::Mesh)
        .unwrap()
        .reference()
        .unwrap();
    // Mixed types may be ordered under the same Part without changing the rendering.
    document.move_organization(
        &mesh,
        Destination::Sibling {
            target: part(103),
            position: ReorderPosition::Before,
        },
    )?;
    assert_eq!(document.mesh_tree()[0].children[0].id, mesh.id);
    document.move_organization(&part(103), Destination::Part(Some(id(102))))?;
    assert_eq!(document.mesh_tree()[1].children, vec![eyes]);
    assert_eq!(document.deformer_tree(), deformer);
    assert_eq!(document.history_lengths(), (2, 0));
    // Across-parent placement updates both ownership and position in one step.
    document.move_organization(
        &mesh,
        Destination::Sibling {
            target: part(103),
            position: ReorderPosition::After,
        },
    )?;
    assert_eq!(document.mesh_tree()[1].children[1].id, mesh.id);
    assert!(document.mesh_tree()[0].children.is_empty());
    assert_eq!(document.history_lengths(), (3, 0));
    let saved = document.save(&assets.path().join("moved.kasane"))?;
    assert_eq!(
        Document::open(&saved.manifest)?.mesh_tree(),
        document.mesh_tree()
    );
    document.undo()?;
    assert_eq!(document.mesh_tree()[0].children[0].id, mesh.id);
    document.undo()?;
    document.undo()?;
    assert_eq!(document.mesh_tree(), original);
    document.redo()?;
    document.move_organization(&mesh, Destination::Part(None))?;
    assert_eq!(document.mesh_tree().last().unwrap().children[0].id, mesh.id);
    document.move_organization(&part(103), Destination::Part(None))?;
    assert!(document.mesh_tree().iter().any(|node| node.id == id(103)));
    assert_eq!(document.deformer_tree(), deformer);
    Ok(())
}

#[test]
fn invalid_part_destinations_and_noops_do_not_discard_redo() -> anyhow::Result<()> {
    use editor_core::OrganizationDestination as Destination;
    let (mut document, _) = support::sample_document_with_parts()?;
    let part = |value| ObjectRef::new(ObjectKind::Part, id(value));
    document.move_organization(&part(103), Destination::Part(Some(id(102))))?;
    document.undo()?;
    let original = document.mesh_tree();
    let history = document.history_lengths();
    for (source, destination) in [
        (part(101), Destination::Part(Some(id(103)))),
        (part(101), Destination::Part(Some(id(101)))),
        (part(103), Destination::Part(Some(id(999)))),
        (part(999), Destination::Part(None)),
        (
            part(101),
            Destination::Sibling {
                target: part(103),
                position: ReorderPosition::Before,
            },
        ),
    ] {
        assert!(document.move_organization(&source, destination).is_err());
        assert_eq!(document.mesh_tree(), original);
        assert_eq!(document.history_lengths(), history);
    }
    document.move_organization(&part(102), Destination::Part(None))?;
    document.move_organization(
        &part(103),
        Destination::Sibling {
            target: part(103),
            position: ReorderPosition::Before,
        },
    )?;
    assert_eq!(document.history_lengths(), history);
    assert_eq!(document.mesh_tree(), original);
    Ok(())
}

fn drawing_fixture(explicit: bool) -> anyhow::Result<(Document, tempfile::TempDir, Vec<String>)> {
    use kasane_core::{BindingAxis, Canvas, MeshBinding, MeshKeyform, Parameter, Part, Vec2};
    use kasane_sdk::AuthoringSession;
    let (mut document, assets) = support::sample_document_with_two_layers()?;
    let saved = document.save(&assets.path().join("drawing.kasane"))?;
    let mut session =
        AuthoringSession::new(&id(900), Canvas::new(256., 256., Vec2::default(), 1.))?;
    session.open_project(&saved.manifest, None)?;
    let mut meshes: Vec<_> = session
        .document()
        .mesh_order()
        .iter()
        .map(|id| session.document().get_mesh(id).unwrap().clone())
        .collect();
    let ids: Vec<_> = meshes.iter().map(|mesh| mesh.id.clone()).collect();
    meshes[0].part_id = id(101);
    meshes[0].draw_order = Some(900.);
    meshes[1].part_id = id(102);
    meshes[1].draw_order = Some(10.);
    session.edit("Drawing fixture", None, |edit| {
        for (value, order) in [(101, 100.), (102, 200.)] {
            edit.create_part(Part {
                id: id(value),
                draw_order: order,
                ..Default::default()
            })?;
        }
        for mesh in &meshes {
            edit.replace_mesh(mesh.clone())?;
        }
        edit.create_parameter(Parameter {
            id: id(201),
            minimum: 0.,
            maximum: 1.,
            default_value: 0.,
            ..Default::default()
        })?;
        edit.create_binding(MeshBinding {
            id: id(202),
            mesh_id: ids[0].clone(),
            axes: vec![BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: id(201),
                keys: vec![0., 1.],
            }],
            keyforms: [(0., 900.), (1., 0.)]
                .into_iter()
                .map(|(key, draw_order)| MeshKeyform {
                    samples: Default::default(),
                    keys: vec![key],
                    positions: meshes[0].base_positions.clone(),
                    appearance: Default::default(),
                    draw_order: Some(draw_order),
                })
                .collect(),
        })?;
        if explicit {
            // An imported drawing layout can differ completely from organization Parts.
            edit.replace_draw_order_groups(vec![kasane_core::draw_order::DrawOrderGroup {
                owner: String::new(),
                items: vec![ids[1].clone(), ids[0].clone()],
                min_order: 0,
                max_order: 1000,
            }])?;
        }
        Ok(())
    })?;
    let saved = session.save_project(&saved.manifest, None)?;
    Ok((Document::open(&saved.manifest)?, assets, ids))
}

fn stored_drawing_groups(document: &Document) -> serde_json::Value {
    document
        .object_index()
        .objects
        .iter()
        .find(|object| object.reference.kind == ObjectKind::Document)
        .unwrap()
        .properties["draw_groups"]
        .clone()
}

fn parameterized_render_orders(
    document: &mut Document,
) -> anyhow::Result<Vec<Vec<(String, i32, i32)>>> {
    [0., 0.5, 1.]
        .into_iter()
        .map(|value| {
            document.set_preview_parameter(&id(201), value)?;
            Ok(document
                .snapshot()?
                .viewport_frame
                .drawables
                .iter()
                .map(|mesh| (mesh.id.clone(), mesh.draw_order, mesh.render_order))
                .collect())
        })
        .collect()
}

#[test]
fn organization_moves_preserve_drawing_across_parameters_history_and_save() -> anyhow::Result<()> {
    use editor_core::OrganizationDestination as Destination;
    for explicit in [false, true] {
        for mode in 0..4 {
            let (mut document, assets, ids) = drawing_fixture(explicit)?;
            let original_tree = document.mesh_tree();
            let original_groups = stored_drawing_groups(&document);
            assert_eq!(original_groups.is_null(), !explicit);
            let groups = document.object_index().draw_groups.clone();
            let baseline = parameterized_render_orders(&mut document)?;
            let mesh = ObjectRef::new(ObjectKind::Mesh, &ids[0]);
            let (source, destination) = match mode {
                0 => (mesh, Destination::Part(Some(id(102)))),
                1 => (
                    mesh,
                    Destination::Sibling {
                        target: ObjectRef::new(ObjectKind::Mesh, &ids[1]),
                        position: ReorderPosition::Before,
                    },
                ),
                2 => (mesh, Destination::Part(None)),
                _ => (
                    ObjectRef::new(ObjectKind::Part, id(102)),
                    Destination::Part(Some(id(101))),
                ),
            };
            document.move_organization(&source, destination)?;
            assert_ne!(document.mesh_tree(), original_tree);
            assert_eq!(
                parameterized_render_orders(&mut document)?,
                baseline,
                "explicit={explicit}, mode={mode}"
            );
            assert_eq!(document.object_index().draw_groups, groups);
            assert_eq!(document.history_lengths(), (1, 0));
            let persisted_groups = stored_drawing_groups(&document);
            assert!(!persisted_groups.is_null());
            if explicit {
                assert_eq!(persisted_groups, original_groups);
            }
            document.undo()?;
            assert_eq!(document.mesh_tree(), original_tree);
            assert_eq!(stored_drawing_groups(&document), original_groups);
            assert_eq!(parameterized_render_orders(&mut document)?, baseline);
            document.redo()?;
            assert_eq!(stored_drawing_groups(&document), persisted_groups);
            let saved = document.save(&assets.path().join("moved.kasane"))?;
            let mut reopened = Document::open(&saved.manifest)?;
            assert_eq!(reopened.mesh_tree(), document.mesh_tree());
            assert_eq!(stored_drawing_groups(&reopened), persisted_groups);
            assert_eq!(parameterized_render_orders(&mut reopened)?, baseline);
        }
    }
    Ok(())
}

#[test]
fn palette_reorders_and_invalid_moves_do_not_materialize_drawing_groups() -> anyhow::Result<()> {
    use editor_core::OrganizationDestination as Destination;
    let (mut document, _assets, _) = drawing_fixture(false)?;
    let part = |value| ObjectRef::new(ObjectKind::Part, id(value));
    document.move_organization(
        &part(102),
        Destination::Sibling {
            target: part(101),
            position: ReorderPosition::Before,
        },
    )?;
    assert!(stored_drawing_groups(&document).is_null());
    document.undo()?;
    let history = document.history_lengths();
    document.move_organization(&part(102), Destination::Part(None))?;
    assert!(
        document
            .move_organization(&part(101), Destination::Part(Some(id(101))))
            .is_err()
    );
    assert!(stored_drawing_groups(&document).is_null());
    assert_eq!(document.history_lengths(), history);
    Ok(())
}
