use super::*;
use zipzi_core::{evaluation::evaluate_frame_including_hidden, PreciseVec2};
fn edit(s: &mut AuthoringSession, run: impl FnOnce(&mut Commands<'_>) -> Result<(), Status>) {
    let mut d = s.begin_draft();
    d.batch(d.source(), run).unwrap();
    s.publish(d.prepare("scene").unwrap()).unwrap();
}
fn create_rotation(c: &mut Commands<'_>) -> Result<(), Status> {
    c.edit_scene(
        SceneEdit::Create {
            id: id(10),
            name: "parent".into(),
            object: SceneCreate::Rotation {
                origin: Some(PreciseVec2::new(10., 20.)),
            },
            part_id: Some(id(3)),
            parent_id: None,
            before: None,
            deformation_before: None,
        },
        Default::default(),
        &Default::default(),
    )?;
    Ok(())
}
fn positions(s: &Snapshot) -> Vec<Vec2> {
    let mut f = zipzi_core::DrawableFrame::default();
    let status = evaluate_frame_including_hidden(s.document(), &Default::default(), &mut f);
    assert!(status.is_ok(), "{status:?}");
    f.drawables
        .iter()
        .find(|d| d.id == id(4))
        .unwrap()
        .positions
        .clone()
}
#[test]
fn immutable_evaluation_reparent_and_original_display_survive_undo() {
    let mut s = AuthoringSession::new(snapshot());
    let before = positions(s.snapshot());
    edit(&mut s, create_rotation);
    let t = s.snapshot().document().transform(&id(10)).unwrap();
    assert!(t.original_shape.as_ref().unwrap().snapshot.is_some());
    assert!(s
        .snapshot()
        .document()
        .editor_state()
        .rotation_bone_lengths
        .contains_key(&id(10)));
    let mut reparent = ReparentEdit::keep_local(vec![id(4)], id(10));
    reparent.strategy = ReparentStrategy::PreserveEditState;
    reparent.context = Some(Default::default());
    edit(&mut s, |c| {
        let outcome = c.edit_scene(
            SceneEdit::Reparent { edit: reparent },
            Default::default(),
            &Default::default(),
        )?;
        assert!(outcome
            .conversion
            .unwrap()
            .report
            .reparent_guarantee
            .is_some());
        Ok(())
    });
    let after = positions(s.snapshot());
    for (a, b) in before.iter().zip(&after) {
        assert!((a.x - b.x).abs() < 0.001 && (a.y - b.y).abs() < 0.001);
    }
    let moved = s.snapshot().clone();
    assert_eq!(moved.document().mesh(&id(4)).unwrap().deformer_id, id(10));
    s.undo(s.version()).unwrap();
    assert!(s
        .snapshot()
        .document()
        .mesh(&id(4))
        .unwrap()
        .deformer_id
        .is_empty());
    s.redo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), moved.content_id());
    assert_eq!(positions(&moved), after);
}
#[test]
fn topology_edit_updates_all_shapes_and_invalid_delete_aborts() {
    let mut s = AuthoringSession::new(snapshot());
    edit(&mut s, |c| {
        c.create_mesh_binding_spec(MeshBindingCreateSpec {
            id: Some(id(20)),
            mesh_id: id(4),
            axes: vec![MeshBindingAxisSpec {
                parameter_id: id(2),
                keys: vec![0., 1.],
            }],
            keyforms: [0., 1.]
                .into_iter()
                .map(|v| MeshKeyformSpec {
                    keys: vec![v],
                    positions: mesh()
                        .base_positions
                        .iter()
                        .map(|p| Vec2::new(p.x + v * 10., p.y))
                        .collect(),
                    opacity: None,
                    multiply: None,
                    screen: None,
                    draw_order: None,
                })
                .collect(),
        })?;
        Ok(())
    });
    edit(&mut s, |c| {
        c.edit_scene_content(
            SceneContentEdit::InsertMeshVertices {
                mesh_id: id(4),
                before: Some(3),
                vertices: vec![NewMeshVertex {
                    vertex_id: 4,
                    position: Vec2::new(0.5, 0.5),
                    uv: Vec2::new(0.5, 0.5),
                    seed: VertexSeed::Interpolate {
                        weights: vec![
                            VertexWeight {
                                vertex_id: 2,
                                weight: 0.5,
                            },
                            VertexWeight {
                                vertex_id: 3,
                                weight: 0.5,
                            },
                        ],
                    },
                }],
            },
            Default::default(),
            &Default::default(),
        )?;
        Ok(())
    });
    let doc = s.snapshot().document();
    assert_eq!(doc.mesh(&id(4)).unwrap().vertex_ids, [1, 2, 4, 3]);
    assert_eq!(
        doc.binding(&id(20)).unwrap().keyforms[1].positions[2],
        Vec2::new(10.5, 0.5)
    );
    let initial = s.snapshot().clone();
    let mut d = s.begin_draft();
    assert!(d
        .batch(d.source(), |c| {
            c.edit_scene_content(
                SceneContentEdit::DeleteMeshVertices {
                    mesh_id: id(4),
                    vertices: vec![2],
                    policy: VertexDeletePolicy::RejectReferences,
                },
                Default::default(),
                &Default::default(),
            )?;
            Ok(())
        })
        .is_err());
    assert_eq!(s.snapshot().content_id(), initial.content_id());
    assert_eq!(positions(&initial).len(), 4);
}
#[test]
fn organization_copy_remaps_owned_bindings_and_delete_cleans_metadata() {
    let mut s = AuthoringSession::new(snapshot());
    edit(&mut s, |c| {
        c.edit_binding_grid(
            BindingGridEdit::Create {
                id: id(20),
                target: BindingGridTarget::Mesh(id(4)),
            },
            Default::default(),
            &Default::default(),
        )?;
        Ok(())
    });
    let mut copied = String::new();
    edit(&mut s, |c| {
        let result = c.edit_organization(OrganizationEdit::Duplicate {
            ids: vec![id(3)],
            destination: OrganizationCollection::PartMembers { part_id: None },
            before: None,
        })?;
        copied = result.copied_ids[&id(3)].clone();
        let next_mesh = &result.copied_ids[&id(4)];
        assert_ne!(next_mesh, &id(4));
        assert!(result.copied_ids.contains_key(&id(20)));
        Ok(())
    });
    let ids = s
        .snapshot()
        .document()
        .organization_subtree(&[copied.clone()])
        .unwrap();
    assert_eq!(ids.len(), 2);
    let copied_mesh = ids
        .iter()
        .find(|id| s.snapshot().document().mesh(id).is_some())
        .unwrap()
        .clone();
    assert_eq!(
        s.snapshot()
            .document()
            .binding_for_mesh(&copied_mesh)
            .unwrap()
            .mesh_id,
        copied_mesh
    );
    let mut state = s.snapshot().document().editor_state().clone();
    state.objects.insert(
        copied_mesh.clone(),
        zipzi_core::document::ObjectEditorState {
            hidden_in_editor: true,
            locked: false,
        },
    );
    edit(&mut s, |c| c.replace_editor_state(state));
    edit(&mut s, |c| {
        c.edit_organization(OrganizationEdit::Delete {
            ids: vec![copied],
            policy: OrganizationDeletePolicy::Contents,
        })?;
        Ok(())
    });
    assert!(s.snapshot().document().mesh(&copied_mesh).is_none());
    assert!(!s
        .snapshot()
        .document()
        .editor_state()
        .objects
        .contains_key(&copied_mesh));
    s.undo(s.version()).unwrap();
    assert!(
        s.snapshot()
            .document()
            .editor_state()
            .get(&copied_mesh)
            .hidden_in_editor
    );
}
#[test]
fn scene_field_preconditions_and_inherited_locks_are_enforced() {
    let mut s = AuthoringSession::new(snapshot());
    let mut d = s.begin_draft();
    assert_eq!(
        d.batch(d.source(), |c| {
            c.edit_scene(
                SceneEdit::Patch {
                    id: id(4),
                    expected: SceneFieldValue::Label("wrong".into()),
                    value: SceneFieldValue::Label("new".into()),
                    clear_glue_pairs: false,
                },
                Default::default(),
                &Default::default(),
            )?;
            Ok(())
        })
        .unwrap_err()
        .code,
        "STALE_FIELD"
    );
    let mut editor = s.snapshot().document().editor_state().clone();
    editor.objects.insert(
        id(3),
        zipzi_core::document::ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    edit(&mut s, |c| c.replace_editor_state(editor));
    let mut d = s.begin_draft();
    assert_eq!(
        d.batch(d.source(), |c| {
            c.edit_scene_content(
                SceneContentEdit::MeshVertices {
                    mesh_id: id(4),
                    target: ShapeTarget::Base,
                    updates: vec![VertexContentPatch {
                        vertex_id: 1,
                        position: Some(Vec2::new(1., 1.)),
                        uv: None,
                    }],
                },
                Default::default(),
                &Default::default(),
            )?;
            Ok(())
        })
        .unwrap_err()
        .code,
        "OBJECT_LOCKED"
    );
}
#[test]
fn grouping_preserves_animated_mesh_shapes_for_rotation_and_warp() {
    for warp in [false, true] {
        let mut s = AuthoringSession::new(snapshot());
        edit(&mut s, |c| {
            c.create_mesh_binding_spec(MeshBindingCreateSpec {
                id: Some(id(20)),
                mesh_id: id(4),
                axes: vec![MeshBindingAxisSpec {
                    parameter_id: id(2),
                    keys: vec![0., 1.],
                }],
                keyforms: [0., 1.]
                    .into_iter()
                    .map(|v| MeshKeyformSpec {
                        keys: vec![v],
                        positions: mesh()
                            .base_positions
                            .iter()
                            .map(|p| Vec2::new(p.x + 5. * v, p.y + 3. * v))
                            .collect(),
                        opacity: None,
                        multiply: None,
                        screen: None,
                        draw_order: None,
                    })
                    .collect(),
            })?;
            Ok(())
        });
        let before = s.snapshot().clone();
        edit(&mut s, |c| {
            c.group_deformer(
                SceneEdit::Create {
                    id: id(10),
                    name: "group".into(),
                    object: if warp {
                        SceneCreate::Warp { bounds: None }
                    } else {
                        SceneCreate::Rotation { origin: None }
                    },
                    part_id: Some(id(3)),
                    parent_id: None,
                    before: None,
                    deformation_before: None,
                },
                vec![id(4)],
                Default::default(),
                &Default::default(),
            )?;
            Ok(())
        });
        for v in [0., 0.25, 1.] {
            let values = [(id(2), v)].into();
            let mut a = zipzi_core::DrawableFrame::default();
            let mut b = a.clone();
            assert!(evaluate_frame_including_hidden(before.document(), &values, &mut a).is_ok());
            assert!(
                evaluate_frame_including_hidden(s.snapshot().document(), &values, &mut b).is_ok()
            );
            for (a, b) in a.drawables[0]
                .positions
                .iter()
                .zip(&b.drawables[0].positions)
            {
                assert!(
                    (a.x - b.x).abs() < 0.001 && (a.y - b.y).abs() < 0.001,
                    "{a:?} {b:?}"
                );
            }
        }
        assert!(s
            .snapshot()
            .document()
            .transform(&id(10))
            .unwrap()
            .original_shape
            .as_ref()
            .unwrap()
            .snapshot
            .is_some());
    }
}
