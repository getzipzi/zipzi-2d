use zipzi_contract::authoring::ObjectKind;
use zipzi_core::document::{
    SceneContentEdit as E, SceneContentTarget as T, SceneContentValue as V,
};
use zipzi_core::BindingAxisEditing;
use zipzi_core::WarpBezierEvaluation;
use zipzi_core::{document::*, *};
use zipzi_sdk::{AuthoringSession, ConversionControl, ConversionLimits};
#[path = "scene_content/deformer_original.rs"]
mod deformer_original;
#[path = "scene_content/warp_preservation.rs"]
mod warp_preservation;
fn id(n: u32) -> String {
    format!("20000000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(1),
                (Canvas::new(100., 100., Vec2::default(), 1.)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    )
}

// Query values carry the complete formal source; Commands also checks the field value.
type ContentQuery = (zipzi_sdk::Version, String, T, V);
fn read_content(
    s: &AuthoringSession,
    owner: &str,
    target: T,
) -> Result<ContentQuery, zipzi_sdk::Status> {
    let value = s
        .snapshot()
        .document()
        .scene_content_value(owner, &target)?;
    Ok((s.version(), owner.into(), target, value))
}
fn prepare_content(
    s: &AuthoringSession,
    query: &ContentQuery,
    value: V,
) -> Result<
    (
        zipzi_core::state::SceneContentOutcome,
        zipzi_sdk::PreparedChange,
    ),
    zipzi_sdk::Status,
> {
    let mut draft = s.begin_draft();
    let mut source = draft.source();
    source.base = query.0;
    let outcome = draft.batch(source, |c| {
        c.patch_scene_content(
            &query.1,
            &query.2,
            query.3.clone(),
            value,
            Default::default(),
            &Default::default(),
        )
    })?;
    Ok((outcome, draft.prepare("Content")?))
}

#[test]
fn warp_content_addresses_expire_after_organization_context_roundtrip() {
    let mut s = session();
    let create = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            SceneEdit::Create {
                id: id(2),
                name: "Warp".into(),
                object: SceneCreate::Warp { bounds: None },
                part_id: None,
                parent_id: None,
                before: None,
                deformation_before: None,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    s.publish(create.1).unwrap();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_part({
                    let mut value = Part {
                        id: id(3),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Part")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let address = read_content(
        &s,
        &id(2),
        T::WarpPoint {
            row: 0,
            column: 0,
            shape: ShapeTarget::Base,
        },
    )
    .unwrap();
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_organization(OrganizationEdit::Move {
            ids: vec![id(2)],
            destination: OrganizationCollection::PartMembers {
                part_id: Some(id(3)),
            },
            before: None,
        })
    })
    .unwrap();
    s.publish(plan.1).unwrap();
    assert!(prepare_content(&s, &address, address.3.clone()).is_err());
    s.undo(s.version()).unwrap();
    assert!(prepare_content(&s, &address, address.3.clone()).is_err());
}

#[test]
fn unchanged_synthetic_bezier_control_does_not_materialize_controller() {
    let mut s = session();
    let create = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            SceneEdit::Create {
                id: id(2),
                name: "Warp".into(),
                object: SceneCreate::Warp { bounds: None },
                part_id: None,
                parent_id: None,
                before: None,
                deformation_before: None,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    s.publish(create.1).unwrap();
    mesh(&mut s, 3);
    s.undo(s.version()).unwrap();
    let history = s.history_state();
    let version = s.version();
    let address = read_content(
        &s,
        &id(2),
        T::WarpBezierControl {
            node: 0,
            component: 0,
            shape: ShapeTarget::Base,
        },
    )
    .unwrap();
    let plan = prepare_content(&s, &address, address.3.clone()).unwrap();
    assert!(plan.1.writes().is_empty());
    assert!(s.publish(plan.1).unwrap().writes.is_empty());
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    assert!(s
        .snapshot()
        .document()
        .transform(&id(2))
        .unwrap()
        .warp()
        .unwrap()
        .bezier
        .is_none());
    let plan = prepare_content(&s, &address, V::Point(Vec2::new(2., 3.))).unwrap();
    assert!(!plan.1.writes().is_empty());
    s.publish(plan.1).unwrap();
    assert!(s
        .snapshot()
        .document()
        .transform(&id(2))
        .unwrap()
        .warp()
        .unwrap()
        .bezier
        .is_some());
}

#[test]
fn unchanged_implicit_offscreen_mapping_preserves_redo() {
    let mut s = session();
    offscreen(&mut s, false);
    mesh(&mut s, 30);
    s.undo(s.version()).unwrap();
    let history = s.history_state();
    let version = s.version();
    let address = read_content(
        &s,
        &id(23),
        T::OffscreenMapping {
            slot: PartKeyformSlot::Static,
        },
    )
    .unwrap();
    let plan = prepare_content(&s, &address, address.3.clone()).unwrap();
    assert!(plan.1.writes().is_empty());
    assert!(s.publish(plan.1).unwrap().writes.is_empty());
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    assert!(s
        .snapshot()
        .document()
        .offscreen(&id(23))
        .unwrap()
        .part_keyform_indices
        .is_empty());
    s.redo(s.version()).unwrap();
    assert!(s.snapshot().document().mesh(&id(30)).is_some());
}

#[test]
fn glue_endpoint_changes_publish_pair_references_in_both_history_directions() {
    let mut s = session();
    for n in [2, 3, 4] {
        mesh(&mut s, n);
    }
    glue(&mut s, 5, 2, 3, vec![pair(6, Some(1), Some(2))]);
    let expected = s
        .snapshot()
        .document()
        .scene_field(&id(5), SceneField::GlueEndpoints)
        .unwrap();
    let plan = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            SceneEdit::Patch {
                id: id(5),
                expected,
                value: SceneFieldValue::GlueEndpoints {
                    a: id(4).into(),
                    b: id(3).into(),
                },
                clear_glue_pairs: false,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    let receipt = s.publish(plan.1).unwrap();
    assert!(receipt.writes.records().iter().any(|r| r.id == id(5)));
    assert_eq!(
        s.snapshot()
            .document()
            .glue(&id(5))
            .unwrap()
            .mesh_a_id
            .resolved_id(),
        Some(id(4).as_str())
    );
    let receipt = s.undo(s.version()).unwrap();
    assert!(receipt.writes.records().iter().any(|r| r.id == id(5)));
    assert_eq!(
        s.snapshot()
            .document()
            .glue(&id(5))
            .unwrap()
            .mesh_a_id
            .resolved_id(),
        Some(id(2).as_str())
    );
    let receipt = s.redo(s.version()).unwrap();
    assert!(receipt.writes.records().iter().any(|r| r.id == id(5)));
    assert_eq!(
        s.snapshot()
            .document()
            .glue(&id(5))
            .unwrap()
            .mesh_a_id
            .resolved_id(),
        Some(id(4).as_str())
    );
}

#[test]
fn shape_address_rejects_recreated_binding_with_identical_content() {
    let mut s = session();
    mesh(&mut s, 2);
    let positions = s
        .snapshot()
        .document()
        .mesh(&id(2))
        .unwrap()
        .base_positions
        .clone();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(3),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.create_binding(MeshBinding {
                    id: id(4),
                    mesh_id: id(2),
                    axes: vec![BindingAxis::new(id(3), vec![0.])],
                    keyforms: vec![MeshKeyform {
                        keys: vec![0.],
                        positions,
                        ..Default::default()
                    }],
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Binding")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let binding = s.snapshot().document().binding(&id(4)).unwrap().clone();
    let address = read_content(
        &s,
        &id(2),
        T::VertexPosition {
            vertex_id: 1,
            shape: ShapeTarget::Ordinary {
                binding_id: id(4),
                samples: binding.keyforms[0].samples.clone(),
            },
        },
    )
    .unwrap();
    let handle = {
        let kind = ObjectKind::MeshBinding;
        let object_id = (&id(4)).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut binding = binding;
                binding.keyforms[0].positions[0].x += 1.;
                e.replace_binding(binding)
            })
            .and_then(|value| {
                let prepared = draft.prepare("Recreate binding")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!((s.read_snapshot().source != handle.2));
    assert!(prepare_content(&s, &address, V::Point(Vec2::new(8., 9.))).is_err());
}
fn run(s: &mut AuthoringSession, e: E) -> zipzi_sdk::Publication {
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene_content(e, Default::default(), &Default::default())
    })
    .unwrap();
    s.publish(p.1).unwrap()
}
fn mesh(s: &mut AuthoringSession, n: u32) {
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_mesh({
                    let mut value = Mesh {
                        id: id(n),
                        vertex_ids: vec![1, 2, 3],
                        base_positions: vec![
                            Vec2::new(0., 0.),
                            Vec2::new(10., 0.),
                            Vec2::new(0., 10.),
                        ],
                        uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                        triangles: vec![[1, 2, 3]],
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Mesh")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}
fn lock(s: &mut AuthoringSession, id: &str) {
    let mut state = s.snapshot().document().editor_state().clone();
    state.objects.insert(
        id.into(),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(state))
            .and_then(|value| {
                let prepared = draft.prepare("Lock")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}
fn glue(s: &mut AuthoringSession, n: u32, a: u32, b: u32, pairs: Vec<GlueVertexPair>) {
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_glue(
                    e,
                    Glue {
                        id: id(n),
                        mesh_a_id: id(a).into(),
                        mesh_b_id: id(b).into(),
                        pairs,
                        ..Default::default()
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Glue")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}
fn pair(n: u32, a: Option<u32>, b: Option<u32>) -> GlueVertexPair {
    GlueVertexPair {
        id: id(n),
        vertex_a: a,
        vertex_b: b,
        ..Default::default()
    }
}
fn offscreen(s: &mut AuthoringSession, bound: bool) {
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_part({
                    let mut value = Part {
                        id: id(20),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                if bound {
                    e.create_parameter({
                        let mut value = Parameter {
                            id: id(21),
                            minimum: 0.,
                            maximum: 1.,
                            ..Default::default()
                        };
                        value.runtime_id = value.id.clone();
                        value
                    })?;
                    e.create_scene_binding(SceneBinding {
                        id: id(22),
                        axes: vec![BindingAxis::new(id(21), vec![0., 1.])],
                        track: SceneTrack::Part {
                            target_id: id(20).into(),
                            keyforms: vec![
                                PartKeyform {
                                    keys: vec![0.],
                                    ..Default::default()
                                },
                                PartKeyform {
                                    keys: vec![1.],
                                    ..Default::default()
                                },
                            ],
                        },
                    })?;
                }
                crate::support::insert_offscreen(
                    e,
                    Offscreen {
                        id: id(23),
                        part_id: id(20),
                        keyforms: vec![
                            OffscreenKeyform {
                                opacity: 0.2,
                                ..Default::default()
                            },
                            OffscreenKeyform {
                                opacity: 0.8,
                                ..Default::default()
                            },
                        ],
                        part_keyform_indices: if bound { vec![0, 1] } else { vec![] },
                        ..Default::default()
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Offscreen")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}

#[test]
fn warp_point_edits_preserve_controller_and_later_bezier_delta_keeps_the_residual() {
    let mut s = session();
    let points: Vec<_> = (0..=2)
        .flat_map(|y| (0..=2).map(move |x| Vec2::new(x as f32 / 2., y as f32 / 2.)))
        .collect();
    let controller = WarpBezier::from_lattice(2, 2, false, &points);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                crate::support::insert_transform(
                    edit,
                    Transform {
                        id: id(2),
                        data: TransformData::Warp(WarpTransform {
                            rows: 2,
                            columns: 2,
                            quad: false,
                            points: points.clone(),
                            bezier: Some(controller.clone()),
                        }),
                        ..Default::default()
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Warp")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let mut detail = points.clone();
    detail[4].y += 0.25;
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                update_points(edit, &id(2), detail.clone())
            })
            .and_then(|value| {
                let prepared = draft.prepare("Lattice detail")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let warp = s
        .snapshot()
        .document()
        .transform(&id(2))
        .unwrap()
        .warp()
        .unwrap();
    assert_eq!(warp.bezier.as_ref(), Some(&controller));
    assert_eq!(warp.points, detail);
    let mut changed = controller.clone();
    changed.nodes[0].handles[1].y += 0.2;
    let expected = changed.apply_delta(&controller, 2, 2, &detail);
    run(
        &mut s,
        E::WarpBezier {
            transform_id: id(2),
            target: ShapeTarget::Base,
            controller: Some(changed.clone()),
        },
    );
    assert_eq!(
        s.snapshot()
            .document()
            .transform(&id(2))
            .unwrap()
            .warp()
            .unwrap()
            .points,
        expected
    );
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .transform(&id(2))
            .unwrap()
            .warp()
            .unwrap()
            .points,
        detail
    );
    assert_eq!(
        s.snapshot()
            .document()
            .transform(&id(2))
            .unwrap()
            .warp()
            .unwrap()
            .bezier
            .as_ref(),
        Some(&controller)
    );
    s.redo(s.version()).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .transform(&id(2))
            .unwrap()
            .warp()
            .unwrap()
            .bezier
            .as_ref(),
        Some(&changed)
    );
}

#[test]
fn warp_authoring_density_refits_all_stored_controllers_and_roundtrips_without_changing_lattices() {
    let mut s = session();
    let points = (0..=4)
        .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32, y as f32)))
        .collect::<Vec<_>>();
    let controller = WarpBezier::from_lattice(4, 4, false, &points);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                crate::support::insert_transform(
                    edit,
                    Transform {
                        id: id(2),
                        data: TransformData::Warp(WarpTransform {
                            rows: 4,
                            columns: 4,
                            quad: false,
                            points: points.clone(),
                            bezier: Some(controller.clone()),
                        }),
                        ..Default::default()
                    },
                )?;
                edit.create_parameter({
                    let mut value = Parameter {
                        id: id(3),
                        minimum: 0.,
                        maximum: 1.,
                        default_value: 0.,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                edit.create_scene_binding(SceneBinding {
                    id: id(4),
                    axes: vec![BindingAxis::new(id(3), vec![0., 1.])],
                    track: SceneTrack::Warp {
                        target_id: id(2).into(),
                        keyforms: vec![
                            WarpKeyform {
                                keys: vec![0.],
                                positions: points.clone(),
                                ..Default::default()
                            },
                            WarpKeyform {
                                keys: vec![1.],
                                positions: points.clone(),
                                bezier: Some(controller.clone()),
                                ..Default::default()
                            },
                        ],
                    },
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Warp and shapes")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let original = s.snapshot().document().transform(&id(2)).cloned().unwrap();
    let binding = s
        .snapshot()
        .document()
        .scene_binding(&id(4))
        .cloned()
        .unwrap();
    let settings = WarpAuthoringSettings {
        auto_reform: true,
        edit_mode: WarpBezierEditMode::SmoothLocal,
        bezier_grid: Some(WarpBezierGrid {
            rows: 3,
            columns: 2,
        }),
    };
    run(
        &mut s,
        E::WarpAuthoring {
            transform_id: id(2),
            settings: settings.clone(),
        },
    );
    let changed = s.snapshot().document().transform(&id(2)).cloned().unwrap();
    assert_eq!(changed.warp().unwrap().points, points);
    assert_eq!(changed.warp_authoring.as_ref(), Some(&settings));
    assert_eq!(
        changed.warp().unwrap().bezier.as_ref().unwrap().nodes.len(),
        12
    );
    let changed_binding = s
        .snapshot()
        .document()
        .scene_binding(&id(4))
        .cloned()
        .unwrap();
    let SceneTrack::Warp { keyforms, .. } = &changed_binding.track else {
        unreachable!()
    };
    assert!(keyforms[0].bezier.is_none());
    assert_eq!(keyforms[1].bezier.as_ref().unwrap().nodes.len(), 12);
    for form in keyforms {
        assert_eq!(form.positions, points);
    }
    let version = s.version();
    assert!(s
        .publish(
            crate::support::prepared(&s, |candidate_commands| candidate_commands
                .edit_scene_content(
                    E::WarpAuthoring {
                        transform_id: id(2),
                        settings: settings.clone()
                    },
                    Default::default(),
                    &Default::default()
                ))
            .unwrap()
            .1
        )
        .unwrap()
        .writes
        .is_empty());
    assert_eq!(s.version(), version);
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.snapshot().document().transform(&id(2)).cloned(),
        Some(original)
    );
    assert_eq!(
        s.snapshot().document().scene_binding(&id(4)).cloned(),
        Some(binding)
    );
    s.redo(s.version()).unwrap();
    assert_eq!(
        s.snapshot().document().transform(&id(2)).cloned(),
        Some(changed.clone())
    );
    let directory =
        std::env::temp_dir().join(format!("zipzi-warp-authoring-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&directory).unwrap();
    let mut reopened = AuthoringSession::new(crate::support::roundtrip(s.snapshot()));
    assert_eq!(
        reopened.snapshot().document().transform(&id(2)).cloned(),
        Some(changed)
    );
    assert_eq!(
        reopened
            .snapshot()
            .document()
            .scene_binding(&id(4))
            .cloned(),
        Some(changed_binding)
    );
    let copy = crate::support::prepared(&reopened, |c| {
        c.edit_scene(
            SceneEdit::Duplicate {
                ids: vec![id(2)],
                scope: SceneCopyScope::Selected,
                destination: None,
                before: None,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    let copied = copy.0.copied_ids[&id(2)].clone();
    reopened.publish(copy.1).unwrap();
    assert_eq!(
        reopened
            .snapshot()
            .document()
            .transform(&copied)
            .cloned()
            .unwrap()
            .warp_authoring
            .as_ref(),
        Some(&settings)
    );
    let before = reopened
        .snapshot()
        .document()
        .transform(&id(2))
        .cloned()
        .unwrap();
    let old_binding = reopened
        .snapshot()
        .document()
        .scene_binding(&id(4))
        .cloned()
        .unwrap();
    let mut locks = reopened.snapshot().document().editor_state().clone();
    locks.objects.insert(
        id(4),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    {
        let mut draft = reopened.begin_draft();
        draft
            .batch(draft.source(), |edit| edit.replace_editor_state(locks))
            .and_then(|value| {
                let prepared = draft.prepare("Lock controller owner")?;
                reopened.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let version = reopened.version();
    assert!(
        crate::support::prepared(&reopened, |c| c.edit_scene_content(
            E::WarpAuthoring {
                transform_id: id(2),
                settings: WarpAuthoringSettings {
                    bezier_grid: Some(WarpBezierGrid {
                        rows: 2,
                        columns: 2
                    }),
                    ..settings.clone()
                }
            },
            Default::default(),
            &Default::default()
        ))
        .is_err()
    );
    assert!(
        crate::support::prepared(&reopened, |c| c.edit_scene_content(
            E::WarpAuthoring {
                transform_id: id(2),
                settings: WarpAuthoringSettings {
                    bezier_grid: Some(WarpBezierGrid {
                        rows: 0,
                        columns: 2
                    }),
                    ..settings.clone()
                }
            },
            Default::default(),
            &Default::default()
        ))
        .is_err()
    );
    assert_eq!(
        reopened.snapshot().document().transform(&id(2)).cloned(),
        Some(before)
    );
    assert_eq!(
        reopened
            .snapshot()
            .document()
            .scene_binding(&id(4))
            .cloned(),
        Some(old_binding)
    );
    assert_eq!(reopened.version(), version);
    assert!(
        crate::support::prepared(&reopened, |c| c.edit_scene_content(
            E::WarpShape {
                transform_id: id(2),
                target: ShapeTarget::Blend {
                    binding_id: id(4),
                    sample_id: id(5)
                },
                action: WarpShapeAction::Reform
            },
            Default::default(),
            &Default::default()
        ))
        .is_err()
    );
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn warp_reset_reform_and_auto_reform_have_distinct_geometry_effects() {
    let mut s = session();
    let points = (0..=4)
        .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32, y as f32)))
        .collect::<Vec<_>>();
    let controller = WarpBezier::from_lattice(4, 4, false, &points);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                crate::support::insert_transform(
                    edit,
                    Transform {
                        id: id(2),
                        data: TransformData::Warp(WarpTransform {
                            rows: 4,
                            columns: 4,
                            quad: false,
                            points: points.clone(),
                            bezier: Some(controller),
                        }),
                        ..Default::default()
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Warp")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let mut detail = points;
    detail[12].y += 0.5;
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                update_points(edit, &id(2), detail.clone())
            })
            .and_then(|value| {
                let prepared = draft.prepare("Detail")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    run(
        &mut s,
        E::WarpShape {
            transform_id: id(2),
            target: ShapeTarget::Base,
            action: WarpShapeAction::ResetController,
        },
    );
    let reset = s.snapshot().document().transform(&id(2)).cloned().unwrap();
    assert_eq!(reset.warp().unwrap().points, detail);
    let before = reset.warp().unwrap().bezier.as_ref().unwrap();
    let address = read_content(
        &s,
        &id(2),
        T::WarpBezierControl {
            node: 0,
            component: 2,
            shape: ShapeTarget::Base,
        },
    )
    .unwrap();
    let point = Vec2::new(
        before.nodes[0].handles[1].x,
        before.nodes[0].handles[1].y + 0.3,
    );
    let plan = prepare_content(&s, &address, V::Point(point)).unwrap();
    s.publish(plan.1).unwrap();
    let edited = s.snapshot().document().transform(&id(2)).cloned().unwrap();
    let next = edited.warp().unwrap().bezier.as_ref().unwrap();
    assert_eq!(
        edited.warp().unwrap().points,
        next.apply_delta(before, 4, 4, &detail)
    );
    run(
        &mut s,
        E::WarpShape {
            transform_id: id(2),
            target: ShapeTarget::Base,
            action: WarpShapeAction::Reform,
        },
    );
    assert_eq!(
        s.snapshot()
            .document()
            .transform(&id(2))
            .cloned()
            .unwrap()
            .warp()
            .unwrap()
            .points,
        next.lattice(4, 4)
    );
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.snapshot().document().transform(&id(2)).cloned(),
        Some(edited.clone())
    );
    run(
        &mut s,
        E::WarpAuthoring {
            transform_id: id(2),
            settings: WarpAuthoringSettings {
                auto_reform: true,
                ..Default::default()
            },
        },
    );
    assert_eq!(
        s.snapshot()
            .document()
            .transform(&id(2))
            .cloned()
            .unwrap()
            .warp()
            .unwrap()
            .points,
        edited.warp().unwrap().points
    );
    let address = read_content(
        &s,
        &id(2),
        T::WarpBezierControl {
            node: 0,
            component: 2,
            shape: ShapeTarget::Base,
        },
    )
    .unwrap();
    let version = s.version();
    let unchanged = prepare_content(&s, &address, address.3.clone()).unwrap();
    assert!(unchanged.1.writes().is_empty());
    assert!(s.publish(unchanged.1).unwrap().writes.is_empty());
    assert_eq!(s.version(), version);
    assert_eq!(
        s.snapshot()
            .document()
            .transform(&id(2))
            .cloned()
            .unwrap()
            .warp()
            .unwrap()
            .points,
        edited.warp().unwrap().points
    );
    let plan = prepare_content(&s, &address, V::Point(Vec2::new(point.x, point.y + 0.2))).unwrap();
    s.publish(plan.1).unwrap();
    let automatic = s.snapshot().document().transform(&id(2)).cloned().unwrap();
    let warp = automatic.warp().unwrap();
    assert_eq!(warp.points, warp.bezier.as_ref().unwrap().lattice(4, 4));
    assert_eq!((warp.rows, warp.columns), (4, 4));
}

#[test]
fn unchanged_warp_points_preserve_bezier_controller_and_redo() {
    let mut s = session();
    let points = vec![
        Vec2::new(0., 0.),
        Vec2::new(1., 0.),
        Vec2::new(0., 1.),
        Vec2::new(1., 1.),
    ];
    let controller = WarpBezier::from_lattice(1, 1, false, &points);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: id(2),
                        data: TransformData::Warp(WarpTransform {
                            rows: 1,
                            columns: 1,
                            quad: false,
                            points: points.clone(),
                            bezier: Some(controller.clone()),
                        }),
                        ..Default::default()
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Warp")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    mesh(&mut s, 3);
    s.undo(s.version()).unwrap();
    let history = s.history_state();
    let version = s.version();
    let (_, receipt) = {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| update_points(e, &id(2), points))
            .and_then(|value| {
                let prepared = draft.prepare("Unchanged points")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(receipt.writes.is_empty());
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    assert_eq!(
        s.snapshot()
            .document()
            .transform(&id(2))
            .unwrap()
            .warp()
            .unwrap()
            .bezier
            .as_ref(),
        Some(&controller)
    );
}
#[test]
fn mesh_topology_seeds_every_shape_and_keeps_vertex_identity() {
    let mut s = session();
    mesh(&mut s, 2);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(3),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.create_binding(MeshBinding {
                    id: id(4),
                    mesh_id: id(2),
                    axes: vec![BindingAxis::new(id(3), vec![-1., 1.])],
                    keyforms: vec![
                        MeshKeyform {
                            keys: vec![-1.],
                            positions: vec![
                                Vec2::new(0., 2.),
                                Vec2::new(20., 2.),
                                Vec2::new(0., 20.),
                            ],
                            ..Default::default()
                        },
                        MeshKeyform {
                            keys: vec![1.],
                            positions: vec![
                                Vec2::new(4., 2.),
                                Vec2::new(24., 2.),
                                Vec2::new(4., 20.),
                            ],
                            ..Default::default()
                        },
                    ],
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("Binding")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let old = s.snapshot().document().binding(&id(4)).unwrap().clone();
    run(
        &mut s,
        E::InsertMeshVertices {
            mesh_id: id(2),
            vertices: vec![NewMeshVertex {
                vertex_id: 40,
                position: Vec2::new(5., 0.),
                uv: Vec2::new(0.5, 0.),
                seed: VertexSeed::Interpolate {
                    weights: vec![
                        VertexWeight {
                            vertex_id: 1,
                            weight: 0.5,
                        },
                        VertexWeight {
                            vertex_id: 2,
                            weight: 0.5,
                        },
                    ],
                },
            }],
            before: Some(3),
        },
    );
    assert_eq!(
        s.snapshot().document().mesh(&id(2)).unwrap().vertex_ids,
        [1, 2, 40, 3]
    );
    assert_eq!(
        s.snapshot().document().mesh(&id(2)).unwrap().base_positions[2],
        Vec2::new(5., 0.)
    );
    assert_eq!(
        s.snapshot().document().binding(&id(4)).unwrap().keyforms[0].positions[2],
        Vec2::new(10., 2.)
    );
    assert_eq!(
        s.snapshot().document().binding(&id(4)).unwrap().axes,
        old.axes
    );
    run(
        &mut s,
        E::ReorderMeshVertices {
            mesh_id: id(2),
            order: vec![40, 3, 2, 1],
        },
    );
    assert_eq!(
        s.snapshot().document().mesh(&id(2)).unwrap().triangles,
        [[1, 2, 3]]
    );
    assert_eq!(
        s.snapshot().document().binding(&id(4)).unwrap().keyforms[1].positions[0],
        Vec2::new(14., 2.)
    );
    s.undo(s.version()).unwrap();
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().binding(&id(4)), Some(&old));
}
#[test]
fn vertex_deletion_requires_policy_and_cleans_dependent_pairs() {
    let mut s = session();
    mesh(&mut s, 2);
    mesh(&mut s, 3);
    glue(&mut s, 4, 2, 3, vec![pair(5, Some(1), Some(2))]);
    let old = {
        let kind = ObjectKind::GlueVertexPair;
        let object_id = (&id(5)).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    let version = s.version();
    assert_eq!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_scene_content(
                E::DeleteMeshVertices {
                    mesh_id: id(2),
                    vertices: vec![1],
                    policy: VertexDeletePolicy::RejectReferences
                },
                Default::default(),
                &Default::default()
            ))
        .unwrap_err()
        .code
        .as_str(),
        "VERTEX_REFERENCED"
    );
    assert_eq!(s.version(), version);
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene_content(
            E::DeleteMeshVertices {
                mesh_id: id(2),
                vertices: vec![1],
                policy: VertexDeletePolicy::RemoveDependentContent,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    assert_eq!(p.0.report.removed_pair_ids, [id(5)]);
    assert_eq!(p.0.report.removed_triangle_slots, [0]);
    s.publish(p.1).unwrap();
    assert!(s
        .snapshot()
        .document()
        .glue(&id(4))
        .unwrap()
        .pairs
        .is_empty());
    assert!(s
        .snapshot()
        .document()
        .mesh(&id(2))
        .unwrap()
        .triangles
        .is_empty());
    s.undo(s.version()).unwrap();
    assert!((s.read_snapshot().source != old.2));
    assert!(s.snapshot().document().glue_pair(&id(5)).is_some());
}
#[test]
fn pair_pending_configuration_and_reordering_keep_ids_and_runtime_slots() {
    let mut s = session();
    mesh(&mut s, 2);
    mesh(&mut s, 3);
    glue(
        &mut s,
        4,
        2,
        3,
        vec![pair(5, None, None), pair(6, Some(1), Some(1))],
    );
    let h = {
        let kind = ObjectKind::GlueVertexPair;
        let object_id = (&id(5)).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    assert_eq!(
        s.snapshot().document().scene_projection().pair_slots[&id(6)].source_slot,
        1
    );
    assert!(!s
        .snapshot()
        .document()
        .scene_projection()
        .pair_slots
        .contains_key(&id(5)));
    assert!(s
        .snapshot()
        .document()
        .scene_projection()
        .disabled
        .iter()
        .any(|d| d.id == id(5) && d.usage == SceneUse::GluePair));
    assert!(s
        .snapshot()
        .document()
        .scene_export_omissions()
        .iter()
        .any(|d| d.id == id(5)));
    let address = read_content(&s, &id(4), T::GluePair { pair_id: id(5) }).unwrap();
    run(
        &mut s,
        E::MoveGluePairs {
            glue_id: id(4),
            ids: vec![id(5)],
            before: None,
        },
    );
    s.snapshot().document().require_object(h.0, &h.1).unwrap();
    assert!(prepare_content(&s, &address, V::GluePair(pair(5, Some(2), Some(2)))).is_err());
    let a = read_content(&s, &id(4), T::GluePair { pair_id: id(5) }).unwrap();
    let p = prepare_content(&s, &a, V::GluePair(pair(5, Some(2), Some(2)))).unwrap();
    s.publish(p.1).unwrap();
    assert!(!s
        .snapshot()
        .document()
        .scene_projection()
        .disabled
        .iter()
        .any(|d| d.id == id(5)));
    assert!(crate::support::frame(s.snapshot(), &Default::default()).is_ok());
    let restored = crate::support::roundtrip(s.snapshot());
    let restored = restored.document();
    assert_eq!(
        restored.glue_pair(&id(5)),
        s.snapshot().document().glue_pair(&id(5))
    );
    run(
        &mut s,
        E::DuplicateGluePairs {
            glue_id: id(4),
            ids: vec![id(5)],
            before: None,
        },
    );
    assert_eq!(s.snapshot().document().glue(&id(4)).unwrap().pairs.len(), 3);
    assert!(s.snapshot().document().builder().freeze().is_ok());
}
#[test]
fn pair_transfer_keeps_handle_but_expires_owner_address() {
    let mut s = session();
    mesh(&mut s, 2);
    mesh(&mut s, 3);
    glue(&mut s, 4, 2, 3, vec![pair(5, Some(1), Some(2))]);
    glue(&mut s, 6, 3, 2, vec![]);
    let h = {
        let kind = ObjectKind::GlueVertexPair;
        let object_id = (&id(5)).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    let a = read_content(&s, &id(4), T::GluePair { pair_id: id(5) }).unwrap();
    run(
        &mut s,
        E::TransferGluePairs {
            glue_id: id(4),
            destination: id(6),
            pairs: vec![pair(5, Some(2), Some(1))],
            before: None,
        },
    );
    s.snapshot().document().require_object(h.0, &h.1).unwrap();
    assert_eq!(
        s.snapshot().document().glue_pair_owner(&id(5)),
        Some(id(6).as_str())
    );
    s.undo(s.version()).unwrap();
    assert!(prepare_content(&s, &a, a.3.clone()).is_err());
}
#[test]
fn uv_and_triangle_addresses_expire_after_topology_and_undo() {
    let mut s = session();
    mesh(&mut s, 2);
    let a = read_content(&s, &id(2), T::VertexUv { vertex_id: 2 }).unwrap();
    let p = prepare_content(&s, &a, V::Point(Vec2::new(0.6, 0.3))).unwrap();
    s.publish(p.1).unwrap();
    assert_eq!(
        s.snapshot().document().mesh(&id(2)).unwrap().uvs[1],
        Vec2::new(0.6, 0.3)
    );
    let a = read_content(&s, &id(2), T::Triangle { slot: 0 }).unwrap();
    run(
        &mut s,
        E::DeleteTriangles {
            mesh_id: id(2),
            slots: vec![0],
        },
    );
    s.undo(s.version()).unwrap();
    assert!(prepare_content(&s, &a, V::Triangle([1, 3, 2])).is_err());
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_scene_content(
                E::ReplaceTriangles {
                    mesh_id: id(2),
                    triangles: vec![[1, 2, 99]]
                },
                Default::default(),
                &Default::default()
            ))
        .is_err()
    );
    let a = read_content(
        &s,
        &id(2),
        T::VertexPosition {
            vertex_id: 1,
            shape: ShapeTarget::Base,
        },
    )
    .unwrap();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.rename(zipzi_sdk::NameTarget::Mesh(&id(2)), ("Face").into())
            })
            .and_then(|value| {
                let prepared = draft.prepare("Label")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let current = read_content(&s, &a.1, a.2.clone()).unwrap();
    assert!(prepare_content(&s, &current, V::Point(Vec2::new(2., 3.))).is_ok());
}
#[test]
fn offscreen_reorders_insertions_and_deletion_remap_part_sampling() {
    let mut s = session();
    offscreen(&mut s, true);
    let a = read_content(&s, &id(23), T::OffscreenKeyform { slot: 0 }).unwrap();
    run(
        &mut s,
        E::ReorderOffscreenKeyforms {
            offscreen_id: id(23),
            order: vec![1, 0],
        },
    );
    assert_eq!(
        s.snapshot()
            .document()
            .offscreen(&id(23))
            .unwrap()
            .part_keyform_indices,
        [1, 0]
    );
    assert!(prepare_content(&s, &a, a.3.clone()).is_err());
    run(
        &mut s,
        E::InsertOffscreenKeyform {
            offscreen_id: id(23),
            value: OffscreenKeyform {
                opacity: 0.4,
                ..Default::default()
            },
            before: Some(0),
        },
    );
    assert_eq!(
        s.snapshot()
            .document()
            .offscreen(&id(23))
            .unwrap()
            .part_keyform_indices,
        [2, 1]
    );
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_scene_content(
                E::DeleteOffscreenKeyforms {
                    offscreen_id: id(23),
                    slots: vec![2],
                    policy: OffscreenDeletePolicy::RejectReferences
                },
                Default::default(),
                &Default::default()
            ))
        .is_err()
    );
    run(
        &mut s,
        E::DeleteOffscreenKeyforms {
            offscreen_id: id(23),
            slots: vec![2],
            policy: OffscreenDeletePolicy::Remap {
                replacements: [(2, Some(0))].into(),
            },
        },
    );
    assert_eq!(
        s.snapshot()
            .document()
            .offscreen(&id(23))
            .unwrap()
            .part_keyform_indices,
        [0, 1]
    );
    let SceneTrack::Part { keyforms, .. } = &s
        .snapshot()
        .document()
        .scene_binding(&id(22))
        .unwrap()
        .track
    else {
        unreachable!()
    };
    let samples = keyforms[1].samples.clone();
    run(
        &mut s,
        E::SetOffscreenMapping {
            offscreen_id: id(23),
            updates: vec![OffscreenMappingPatch {
                slot: PartKeyformSlot::Samples { samples },
                keyform: -7,
            }],
        },
    );
    assert_eq!(
        s.snapshot()
            .document()
            .offscreen(&id(23))
            .unwrap()
            .part_keyform_indices,
        [0, -7]
    );
    assert!(crate::support::frame(s.snapshot(), &Default::default()).is_ok());
}
#[test]
fn static_offscreen_implicit_zero_is_preserved_by_structural_edits() {
    let mut s = session();
    offscreen(&mut s, false);
    run(
        &mut s,
        E::InsertOffscreenKeyform {
            offscreen_id: id(23),
            value: OffscreenKeyform {
                opacity: 1.,
                ..Default::default()
            },
            before: Some(0),
        },
    );
    assert_eq!(
        s.snapshot()
            .document()
            .offscreen(&id(23))
            .unwrap()
            .part_keyform_indices,
        [1]
    );
    run(
        &mut s,
        E::DeleteOffscreenKeyforms {
            offscreen_id: id(23),
            slots: vec![1],
            policy: OffscreenDeletePolicy::Neutral,
        },
    );
    assert_eq!(
        s.snapshot()
            .document()
            .offscreen(&id(23))
            .unwrap()
            .part_keyform_indices,
        [-1]
    );
}
#[test]
fn warp_control_resize_updates_grid_shapes_and_expires_old_controls() {
    let mut s = session();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
                        rotation_scale_unit: None,
                        warp_authoring: None,
                        original_shape: None,
                        id: id(2),
                        runtime_id: "Warp".into(),
                        name: "Warp".into(),
                        part_id: None,
                        parent_id: None,
                        appearance: Default::default(),
                        enabled: true,
                        data: TransformData::Warp(WarpTransform {
                            bezier: None,
                            rows: 1,
                            columns: 1,
                            quad: false,
                            points: vec![
                                Vec2::new(0., 0.),
                                Vec2::new(10., 0.),
                                Vec2::new(0., 10.),
                                Vec2::new(10., 10.),
                            ],
                        }),
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("Warp")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let a = read_content(
        &s,
        &id(2),
        T::WarpBezierControl {
            node: 0,
            component: 2,
            shape: ShapeTarget::Base,
        },
    )
    .unwrap();
    let p = prepare_content(&s, &a, V::Point(Vec2::new(6., 2.))).unwrap();
    s.publish(p.1).unwrap();
    assert!(s
        .snapshot()
        .document()
        .transform(&id(2))
        .unwrap()
        .warp()
        .unwrap()
        .bezier
        .is_some());
    let a = read_content(
        &s,
        &id(2),
        T::WarpPoint {
            row: 1,
            column: 1,
            shape: ShapeTarget::Base,
        },
    )
    .unwrap();
    let controller = s
        .snapshot()
        .document()
        .transform(&id(2))
        .unwrap()
        .warp()
        .unwrap()
        .bezier
        .clone();
    let point_plan = prepare_content(&s, &a, V::Point(Vec2::new(12., 12.))).unwrap();
    s.publish(point_plan.1).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .transform(&id(2))
            .unwrap()
            .warp()
            .unwrap()
            .bezier,
        controller
    );
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene_content(
            E::ResizeWarp {
                transform_id: id(2),
                rows: 3,
                columns: 2,
                quad: true,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    assert_eq!(p.0.report.resampled_shapes, 1);
    s.publish(p.1).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .transform(&id(2))
            .unwrap()
            .warp()
            .unwrap()
            .points
            .len(),
        12
    );
    s.undo(s.version()).unwrap();
    assert!(prepare_content(&s, &a, a.3.clone()).is_err());
}
#[test]
fn content_failures_locks_noops_budget_and_cancellation_are_atomic() {
    let mut s = session();
    mesh(&mut s, 2);
    let v = s.version();
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene_content(
            E::ReorderMeshVertices {
                mesh_id: id(2),
                order: vec![1, 2, 3],
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    assert!(p.1.writes().is_empty());
    assert!(s.publish(p.1).unwrap().writes.is_empty());
    assert_eq!(s.version(), v);
    let control = ConversionControl::default();
    control.cancel();
    assert_eq!(
        crate::support::prepared(&s, |c| c.edit_scene_content(
            E::DeleteTriangles {
                mesh_id: id(2),
                slots: vec![0]
            },
            Default::default(),
            &control
        ))
        .unwrap_err()
        .code
        .as_str(),
        "AUTHORING_CANCELLED"
    );
    assert_eq!(
        crate::support::prepared(&s, |c| c.edit_scene_content(
            E::DeleteTriangles {
                mesh_id: id(2),
                slots: vec![0]
            },
            ConversionLimits {
                max_workspace_bytes: 1,
                ..Default::default()
            },
            &Default::default()
        ))
        .unwrap_err()
        .code
        .as_str(),
        "AUTHORING_MEMORY_LIMIT"
    );
    assert_eq!(s.version(), v);
    lock(&mut s, &id(2));
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_scene_content(
                E::DeleteTriangles {
                    mesh_id: id(2),
                    slots: vec![0]
                },
                Default::default(),
                &Default::default()
            ))
        .is_err()
    );
}

#[test]
fn ordinary_and_blend_shapes_follow_topology_without_recreating_shared_tables() {
    let mut s = session();
    mesh(&mut s, 2);
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            SceneEdit::Create {
                id: id(9),
                name: "Warp".into(),
                object: SceneCreate::Warp {
                    bounds: Some([0., 0., 10., 10.]),
                },
                part_id: None,
                parent_id: None,
                before: None,
                deformation_before: None,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    s.publish(p.1).unwrap();
    let points = s
        .snapshot()
        .document()
        .transform(&id(9))
        .unwrap()
        .warp()
        .unwrap()
        .points
        .clone();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(3),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.create_scene_binding(SceneBinding {
                    id: id(4),
                    axes: vec![BindingAxis::new(id(3), vec![-1., 1.])],
                    track: SceneTrack::Warp {
                        target_id: id(9).into(),
                        keyforms: vec![
                            WarpKeyform {
                                keys: vec![-1.],
                                positions: points.clone(),
                                ..Default::default()
                            },
                            WarpKeyform {
                                keys: vec![1.],
                                positions: points
                                    .iter()
                                    .map(|p| Vec2::new(p.x + 4., p.y + 3.))
                                    .collect(),
                                ..Default::default()
                            },
                        ],
                    },
                })?;
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(5),
                        kind: ParameterKind::BlendShape,
                        minimum: 0.,
                        maximum: 1.,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.edit_blend_shape(
                    zipzi_sdk::BlendAuthoringEdit::CreateTable {
                        table: BlendShapeKeyTable {
                            id: id(6),
                            parameter_id: id(5),
                            samples: vec![],
                            base_sample_id: String::new(),
                            keys: vec![0., 1.],
                            base_key_idx: 0,
                        },
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())?;
                {
                    let binding = BlendShapeBinding {
                        id: id(7),
                        target_id: id(2),
                        target_kind: BlendShapeTargetKind::Mesh,
                        key_table_id: id(6),
                        constraint_ids: vec![],
                        keyforms: DeltaKeyforms::Mesh(vec![
                            DeltaMeshKeyform {
                                positions: vec![Vec2::default(); 3],
                                ..Default::default()
                            },
                            DeltaMeshKeyform {
                                positions: vec![
                                    Vec2::new(2., 3.),
                                    Vec2::new(6., 7.),
                                    Vec2::new(10., 11.),
                                ],
                                ..Default::default()
                            },
                        ]),
                    };
                    e.edit_blend_shape(
                        zipzi_sdk::BlendAuthoringEdit::CreateBinding {
                            id: binding.id.clone(),
                            target_id: binding.target_id.clone(),
                            target_kind: binding.target_kind,
                            key_table_id: binding.key_table_id.clone(),
                        },
                        Default::default(),
                        &Default::default(),
                    )?;
                    e.replace_blend_binding(binding)
                }?;
                {
                    let binding = BlendShapeBinding {
                        id: id(8),
                        target_id: id(9),
                        target_kind: BlendShapeTargetKind::Warp,
                        key_table_id: id(6),
                        constraint_ids: vec![],
                        keyforms: DeltaKeyforms::Warp(vec![
                            DeltaWarpKeyform {
                                points: vec![Vec2::default(); points.len()],
                                ..Default::default()
                            },
                            DeltaWarpKeyform {
                                points: vec![Vec2::new(2., 3.); points.len()],
                                ..Default::default()
                            },
                        ]),
                    };
                    e.edit_blend_shape(
                        zipzi_sdk::BlendAuthoringEdit::CreateBinding {
                            id: binding.id.clone(),
                            target_id: binding.target_id.clone(),
                            target_kind: binding.target_kind,
                            key_table_id: binding.key_table_id.clone(),
                        },
                        Default::default(),
                        &Default::default(),
                    )?;
                    e.replace_blend_binding(binding)
                }
            })
            .and_then(|value| {
                let prepared = draft.prepare("Shapes")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let table = s
        .snapshot()
        .document()
        .blend_key_table(&id(6))
        .unwrap()
        .clone();
    run(
        &mut s,
        E::InsertMeshVertices {
            mesh_id: id(2),
            vertices: vec![NewMeshVertex {
                vertex_id: 4,
                position: Vec2::new(5., 0.),
                uv: Vec2::new(0.5, 0.),
                seed: VertexSeed::Interpolate {
                    weights: vec![
                        VertexWeight {
                            vertex_id: 1,
                            weight: 0.5,
                        },
                        VertexWeight {
                            vertex_id: 2,
                            weight: 0.5,
                        },
                    ],
                },
            }],
            before: None,
        },
    );
    let DeltaKeyforms::Mesh(forms) = &s
        .snapshot()
        .document()
        .blend_binding(&id(7))
        .unwrap()
        .keyforms
    else {
        unreachable!()
    };
    assert_eq!(forms[1].positions[3], Vec2::new(4., 5.));
    let address = read_content(
        &s,
        &id(2),
        T::VertexPosition {
            vertex_id: 4,
            shape: ShapeTarget::Blend {
                binding_id: id(7),
                sample_id: table.samples[1].id.clone(),
            },
        },
    )
    .unwrap();
    let p = prepare_content(&s, &address, V::Point(Vec2::new(8., 9.))).unwrap();
    s.publish(p.1).unwrap();
    run(
        &mut s,
        E::ResizeWarp {
            transform_id: id(9),
            rows: 3,
            columns: 2,
            quad: false,
        },
    );
    let SceneTrack::Warp { keyforms, .. } =
        &s.snapshot().document().scene_binding(&id(4)).unwrap().track
    else {
        unreachable!()
    };
    assert_eq!(keyforms[1].positions.len(), 12);
    assert_eq!(keyforms[1].positions[11], Vec2::new(14., 13.));
    let samples = keyforms[1].samples.clone();
    let DeltaKeyforms::Warp(forms) = &s
        .snapshot()
        .document()
        .blend_binding(&id(8))
        .unwrap()
        .keyforms
    else {
        unreachable!()
    };
    assert_eq!(forms[1].points.len(), 12);
    assert!(forms[1]
        .points
        .iter()
        .all(|p| (p.x - 2.).abs() < 1e-5 && (p.y - 3.).abs() < 1e-5));
    assert_eq!(
        s.snapshot().document().blend_key_table(&id(6)),
        Some(&table)
    );
    run(
        &mut s,
        E::WarpPoints {
            transform_id: id(9),
            target: ShapeTarget::Ordinary {
                binding_id: id(4),
                samples,
            },
            updates: vec![WarpPointPatch {
                row: 3,
                column: 2,
                point: Vec2::new(16., 17.),
            }],
            clear_bezier: false,
        },
    );
    assert!(s.snapshot().document().builder().freeze().is_ok());
}
#[test]
fn content_reference_receipts_invert_and_locked_pair_flags_are_not_dropped() {
    let mut s = session();
    mesh(&mut s, 2);
    mesh(&mut s, 3);
    glue(&mut s, 4, 2, 3, vec![pair(5, Some(1), Some(2))]);
    let r = run(
        &mut s,
        E::PatchGluePair {
            glue_id: id(4),
            pair: pair(5, Some(2), Some(3)),
        },
    );
    assert!(r.writes.records().iter().any(|r| r.id == id(4)));
    assert_eq!(
        s.snapshot().document().glue(&id(4)).unwrap().pairs[0].vertex_a,
        Some(2)
    );
    let undo = s.undo(s.version()).unwrap();
    assert!(undo.writes.records().iter().any(|r| r.id == id(4)));
    assert_eq!(
        s.snapshot().document().glue(&id(4)).unwrap().pairs[0].vertex_a,
        Some(1)
    );
    lock(&mut s, &id(5));
    let v = s.version();
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_scene_content(
                E::DeleteMeshVertices {
                    mesh_id: id(2),
                    vertices: vec![1],
                    policy: VertexDeletePolicy::RemoveDependentContent
                },
                Default::default(),
                &Default::default()
            ))
        .is_err()
    );
    assert_eq!(s.version(), v);
    let mut g = s.snapshot().document().glue(&id(4)).unwrap().clone();
    g.pairs.clear();
    assert_eq!(
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_glue(g))
                .and_then(|value| {
                    let prepared = draft.prepare("Clear locked")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    assert!(
        crate::support::prepared(&s, |candidate_commands| candidate_commands
            .edit_scene_content(
                E::InsertGluePair {
                    glue_id: id(4),
                    pair: pair(2, None, None),
                    before: None
                },
                Default::default(),
                &Default::default()
            ))
        .is_err()
    );
}
#[test]
fn bezier_controller_topology_can_change_without_resetting_the_runtime_lattice() {
    let mut s = session();
    let p = crate::support::prepared(&s, |candidate_commands| {
        candidate_commands.edit_scene(
            SceneEdit::Create {
                id: id(2),
                name: "Warp".into(),
                object: SceneCreate::Warp {
                    bounds: Some([0., 0., 10., 10.]),
                },
                part_id: None,
                parent_id: None,
                before: None,
                deformation_before: None,
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    s.publish(p.1).unwrap();
    let old = s
        .snapshot()
        .document()
        .transform(&id(2))
        .unwrap()
        .warp()
        .unwrap()
        .points
        .clone();
    let address = read_content(
        &s,
        &id(2),
        T::WarpBezierControl {
            node: 0,
            component: 2,
            shape: ShapeTarget::Base,
        },
    )
    .unwrap();
    let grid: Vec<_> = (0..=4)
        .flat_map(|y| (0..=4).map(move |x| Vec2::new(x as f32 * 2.5, y as f32 * 2.5)))
        .collect();
    let controller = WarpBezier::from_lattice_grid(
        4,
        4,
        false,
        &grid,
        WarpBezierGrid {
            rows: 2,
            columns: 2,
        },
    );
    assert_eq!(controller.rows, 2);
    run(
        &mut s,
        E::WarpBezier {
            transform_id: id(2),
            target: ShapeTarget::Base,
            controller: Some(controller),
        },
    );
    let w = s
        .snapshot()
        .document()
        .transform(&id(2))
        .unwrap()
        .warp()
        .unwrap();
    assert_eq!(w.bezier.as_ref().unwrap().rows, 2);
    assert_eq!(w.points, old);
    let value = read_content(
        &s,
        &id(2),
        T::WarpBezierControl {
            node: 4,
            component: 2,
            shape: ShapeTarget::Base,
        },
    )
    .unwrap();
    assert!(matches!(&value.3, V::Point(_)));
    s.undo(s.version()).unwrap();
    assert!(prepare_content(&s, &address, address.3.clone()).is_err());
}

fn update_points(
    c: &mut zipzi_sdk::Commands<'_>,
    id: &str,
    points: Vec<Vec2>,
) -> Result<(), zipzi_sdk::Status> {
    let state = c.read_document()?;
    let warp = state.transform(id).unwrap().warp().unwrap();
    let columns = warp.columns as usize + 1;
    let updates = points
        .into_iter()
        .enumerate()
        .map(|(i, position)| WarpPointPatch {
            row: (i / columns) as u32,
            column: (i % columns) as u32,
            point: position,
        })
        .collect();
    c.edit_scene_content(
        E::WarpPoints {
            transform_id: id.into(),
            target: ShapeTarget::Base,
            updates,
            clear_bezier: false,
        },
        Default::default(),
        &Default::default(),
    )?;
    Ok(())
}
