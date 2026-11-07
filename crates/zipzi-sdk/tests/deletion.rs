use zipzi_core::BindingAxisEditing;
use zipzi_core::{document::*, *};
mod business;
mod memory;
mod review;
mod scene;
use zipzi_contract::authoring::ObjectKind;

use zipzi_sdk::{rectangle_mesh, AuthoringSession, ConversionControl, ConversionLimits};

fn id(n: u32) -> String {
    format!("30000000-0000-4000-8000-{n:012x}")
}
fn fixture() -> AuthoringSession {
    let mut s = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                &id(1),
                (Canvas::new(100., 100., Vec2::default(), 1.)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_image(
                    e,
                    crate::support::image_fixture(
                        &id(2),
                        "shared",
                        &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                            .join("tests/fixtures/asymmetric-2x2.png"),
                    )?,
                )?;
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(3),
                        minimum: 0.,
                        maximum: 1.,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.create_part({
                    let mut value = Part {
                        id: id(4),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.create_scene_binding(SceneBinding {
                    id: id(5),
                    axes: vec![BindingAxis::new(id(3), vec![0., 1.])],
                    track: SceneTrack::Part {
                        target_id: id(4).into(),
                        keyforms: [0., 1.]
                            .into_iter()
                            .map(|v| PartKeyform {
                                keys: vec![v],
                                samples: vec![],
                                draw_order: v,
                            })
                            .collect(),
                    },
                })?;
                for n in 10..=12 {
                    e.create_mesh(rectangle_mesh(
                        &id(n),
                        "mesh",
                        &id(2),
                        Vec2::default(),
                        Vec2::new(10., 10.),
                    )?)?;
                    let positions = e
                        .read_document()
                        .unwrap()
                        .mesh(&id(n))
                        .unwrap()
                        .base_positions
                        .clone();
                    e.create_binding(MeshBinding {
                        id: id(n + 10),
                        mesh_id: id(n),
                        axes: vec![BindingAxis {
                            id: String::new(),
                            samples: vec![],
                            parameter_id: id(3),
                            keys: vec![0., 1.],
                        }],
                        keyforms: [0., 1.]
                            .into_iter()
                            .map(|v| MeshKeyform {
                                keys: vec![v],
                                samples: vec![],
                                positions: positions.clone(),
                                appearance: Appearance::default(),
                                draw_order: None,
                            })
                            .collect(),
                    })?;
                }
                let mut b = e.read_document().unwrap().mesh(&id(11)).unwrap().clone();
                b.masks = vec![id(12), id(10)];
                b.inverted_mask = true;
                e.replace_mesh(b)?;
                crate::support::insert_glue(
                    e,
                    Glue {
                        id: id(30),
                        runtime_id: "seam".into(),
                        mesh_a_id: id(10).into(),
                        mesh_b_id: id(12).into(),
                        pairs: vec![GlueVertexPair {
                            id: id(31),
                            vertex_a: Some(0),
                            vertex_b: Some(0),
                            ..Default::default()
                        }],
                        binding: Some(GlueBinding {
                            axes: vec![BindingAxis::new(id(3), vec![0., 1.])],
                            keyforms: vec![
                                GlueKeyform {
                                    samples: vec![],
                                    intensity: 1.
                                };
                                2
                            ],
                        }),
                        ..Default::default()
                    },
                )?;
                crate::support::insert_offscreen(
                    e,
                    Offscreen {
                        id: id(40),
                        runtime_id: "composite".into(),
                        part_id: id(4),
                        masks: vec![id(10), id(12)],
                        ..Default::default()
                    },
                )?;
                e.replace_model3_settings(Model3Settings {
                    hit_areas: Some(vec![
                        ModelHitArea {
                            id: id(50),
                            name: "area".into(),
                            mesh: ModelTargetRef::Resolved { object_id: id(10) },
                            extensions: Default::default(),
                        },
                        ModelHitArea {
                            id: id(51),
                            name: "imported".into(),
                            mesh: ModelTargetRef::Unresolved {
                                runtime_id: "missing".into(),
                            },
                            extensions: Default::default(),
                        },
                    ]),
                    ..Default::default()
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("fixture")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    s
}
fn request(ids: &[u32]) -> DeletionRequest {
    DeletionRequest {
        roots: ids.iter().map(|n| DeletionTarget::Mesh(id(*n))).collect(),
        ..Default::default()
    }
}
fn lock(s: &mut AuthoringSession, id: &str) {
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut flags = e.read_document().unwrap().editor_state().clone();
                flags.objects.insert(
                    id.into(),
                    ObjectEditorState {
                        locked: true,
                        ..Default::default()
                    },
                );
                e.replace_editor_state(flags)
            })
            .and_then(|value| {
                let prepared = draft.prepare("lock")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}

fn prepare_deletion(
    s: &AuthoringSession,
    request: &DeletionRequest,
) -> Result<
    (
        zipzi_core::state::DeletionOutcome,
        zipzi_sdk::PreparedChange,
    ),
    Status,
> {
    crate::support::prepared(s, |c| {
        c.edit_deletion(request, Default::default(), &Default::default())
    })
}
fn analyze(
    s: &AuthoringSession,
    request: &DeletionRequest,
) -> Result<
    (
        DeletionReport,
        std::collections::BTreeSet<zipzi_project::OpaqueExportIssue>,
    ),
    Status,
> {
    let (outcome, prepared) = crate::support::prepared(s, |c| {
        c.review_deletion(request, Default::default(), &Default::default())
    })?;
    let before =
        zipzi_project::preflight_opaque_exports(s.snapshot().document(), &Default::default())?;
    let after = zipzi_project::preflight_opaque_exports(
        prepared.snapshot().document(),
        &Default::default(),
    )?;
    Ok((outcome.report, after.difference(&before).cloned().collect()))
}

#[test]
fn cleans_mask_glue_hit_area_and_owned_identities_in_one_history_step() {
    let mut s = fixture();
    assert_eq!(
        crate::support::frame(s.snapshot(), &PreviewValues::default())
            .unwrap()
            .drawables
            .len(),
        3
    );
    let before = s.snapshot().document().clone();
    let mesh = {
        let kind = ObjectKind::Mesh;
        let object_id = (&id(10)).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    let sample_id = s.snapshot().document().binding(&id(20)).unwrap().axes[0].samples[0]
        .id
        .clone();
    let sample = {
        let kind = ObjectKind::BindingSample;
        let object_id = (&sample_id).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    let request = request(&[10]);
    let plan = prepare_deletion(&s, &request).unwrap();
    assert!(plan.0.report.requires_confirmation());
    assert_eq!(s.snapshot().content_id(), before.content_id());
    let stale = s.begin_draft();
    let receipt = s.publish(plan.1).unwrap();
    assert!(!receipt.writes.is_empty());
    assert!(receipt
        .writes
        .records()
        .iter()
        .any(|r| r.id == id(10) && r.change == zipzi_core::state::RecordChange::Deleted));
    assert!(receipt
        .writes
        .records()
        .iter()
        .any(|r| r.id == id(30) && r.change == zipzi_core::state::RecordChange::Deleted));
    assert!(receipt.writes.records().iter().any(|r| r.id == id(11)));
    assert!(s.snapshot().document().mesh(&id(10)).is_none());
    assert!(s.snapshot().document().glue(&id(30)).is_none());
    assert!(s.snapshot().document().binding(&id(20)).is_none());
    assert_eq!(
        s.snapshot().document().mesh(&id(11)).unwrap().masks,
        vec![id(12)]
    );
    assert!(s.snapshot().document().mesh(&id(11)).unwrap().inverted_mask);
    assert_eq!(
        s.snapshot().document().offscreen(&id(40)).unwrap().masks,
        vec![id(12)]
    );
    assert_eq!(
        s.snapshot()
            .document()
            .model3_settings()
            .hit_areas
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(s.snapshot().document().mesh(&id(12)), before.mesh(&id(12)));
    assert_eq!(
        s.snapshot().document().binding(&id(22)),
        before.binding(&id(22))
    );
    assert_eq!(s.snapshot().document().asset(&id(2)), before.asset(&id(2)));
    assert!(s.snapshot().document().builder().freeze().is_ok());
    assert_eq!(
        crate::support::frame(s.snapshot(), &PreviewValues::default())
            .unwrap()
            .drawables
            .len(),
        2
    );
    assert_ne!(s.read_snapshot().source, mesh.2);
    assert_ne!(s.read_snapshot().source, sample.2);
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
    assert_eq!(
        crate::support::frame(s.snapshot(), &PreviewValues::default())
            .unwrap()
            .drawables
            .len(),
        3
    );
    assert_ne!(s.read_snapshot().source, mesh.2);
    assert_ne!(s.read_snapshot().source, sample.2);
    assert_eq!(
        s.publish(stale.prepare("old deletion").unwrap())
            .unwrap_err()
            .code
            .as_str(),
        "STALE_SOURCE"
    );
    s.redo(s.version()).unwrap();
    assert!(s.snapshot().document().mesh(&id(10)).is_none());
}

#[test]
fn multi_root_order_duplicates_cycles_and_mask_normalization() {
    let mut s = fixture();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut a = e.read_document().unwrap().mesh(&id(10)).unwrap().clone();
                a.masks = vec![id(12)];
                e.replace_mesh(a)?;
                let mut c = e.read_document().unwrap().mesh(&id(12)).unwrap().clone();
                c.masks = vec![id(10)];
                e.replace_mesh(c)
            })
            .and_then(|value| {
                let prepared = draft.prepare("cycle")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let a = analyze(&s, &request(&[10, 12, 10])).unwrap();
    let b = analyze(&s, &request(&[12, 10])).unwrap();
    assert_eq!(a.0, b.0);
    assert_eq!(
        a.0.removed
            .iter()
            .filter(|v| v.object.id == id(30) && v.object.kind == DeletionKind::Glue)
            .count(),
        1
    );
    // A removed Mask consumer gets no useless external unbind operation.
    assert!(!a
        .0
        .rewritten
        .iter()
        .any(|r| r.owner.id == id(10) || r.owner.id == id(12)));
    let plan = prepare_deletion(&s, &request(&[10, 12])).unwrap();
    s.publish(plan.1).unwrap();
    let b = s.snapshot().document().mesh(&id(11)).unwrap();
    assert!(b.masks.is_empty());
    assert!(!b.inverted_mask);
    assert!(s
        .snapshot()
        .document()
        .offscreen(&id(40))
        .unwrap()
        .masks
        .is_empty());
}

#[test]
fn cleanup_checks_consumer_pair_sample_and_hit_area_locks_without_mutating_history() {
    for target in [
        id(11),
        id(31),
        id(50),
        "sample".into(),
        "glue_sample".into(),
    ] {
        let mut s = fixture();
        let target = if target == "sample" {
            s.snapshot().document().binding(&id(20)).unwrap().axes[0].samples[0]
                .id
                .clone()
        } else if target == "glue_sample" {
            s.snapshot()
                .document()
                .glue(&id(30))
                .unwrap()
                .binding
                .as_ref()
                .unwrap()
                .axes[0]
                .samples[0]
                .id
                .clone()
        } else {
            target
        };
        lock(&mut s, &target);
        let before = s.snapshot().document().clone();
        let history = s.history_state();
        let analysis = analyze(&s, &request(&[10])).unwrap();
        assert!(analysis.0.blockers.iter().any(|b| b.source_id == target));
        assert_eq!(
            prepare_deletion(&s, &request(&[10]))
                .unwrap_err()
                .code
                .as_str(),
            "OBJECT_LOCKED"
        );
        assert_eq!(s.snapshot().content_id(), before.content_id());
        assert_eq!(s.history_state(), history);
    }
}

#[test]
fn locked_read_only_shared_resources_and_unrelated_binding_do_not_block() {
    let mut s = fixture();
    for target in [id(2), id(3), id(22)] {
        lock(&mut s, &target);
    }
    let plan = prepare_deletion(&s, &request(&[10])).unwrap();
    s.publish(plan.1).unwrap();
    assert!(s.snapshot().document().mesh(&id(12)).is_some());
}

#[test]
fn glue_and_offscreen_roots_preserve_endpoint_meshes_and_part_binding() {
    let mut s = fixture();
    let before = s.snapshot().document().clone();
    let request = DeletionRequest {
        roots: vec![
            DeletionTarget::Glue(id(30)),
            DeletionTarget::Offscreen(id(40)),
        ],
        ..Default::default()
    };
    let plan = prepare_deletion(&s, &request).unwrap();
    s.publish(plan.1).unwrap();
    for n in 10..=12 {
        assert_eq!(s.snapshot().document().mesh(&id(n)), before.mesh(&id(n)));
    }
    assert_eq!(s.snapshot().document().part(&id(4)), before.part(&id(4)));
    assert_eq!(
        s.snapshot().document().scene_binding(&id(5)),
        before.scene_binding(&id(5))
    );
    assert!(s.snapshot().document().glue(&id(30)).is_none());
    assert!(s.snapshot().document().offscreen(&id(40)).is_none());
}

#[test]
fn plans_reject_changed_candidate_even_when_formal_version_has_not_changed() {
    let s = fixture();
    let mut draft = s.begin_draft();
    let source = draft.source();
    let (preview, _) = draft
        .prepare_preview(source, |c| {
            c.edit_deletion(&request(&[10]), Default::default(), &Default::default())
        })
        .unwrap();
    draft
        .batch(source, |c| {
            c.rename(zipzi_sdk::NameTarget::Mesh(&id(11)), "changed".into())
        })
        .unwrap();
    assert_eq!(
        draft.install_preview(preview).unwrap_err().code.as_str(),
        "STALE_SOURCE"
    );
    assert!(draft.is_active());
}

#[test]
fn candidate_plan_cannot_escape_to_another_edit_with_identical_source() {
    let s = fixture();
    let a = s.begin_draft();
    let (preview, _) = a
        .prepare_preview(a.source(), |c| {
            c.edit_deletion(&request(&[10]), Default::default(), &Default::default())
        })
        .unwrap();
    let mut b = s.begin_draft();
    assert_eq!(
        b.install_preview(preview).unwrap_err().code.as_str(),
        "STALE_SOURCE"
    );
}

#[test]
fn background_candidate_plan_retains_the_edit_identity() {
    let s = fixture();
    let a = s.begin_draft();
    let owner = a.source();
    let (a, preview) = std::thread::spawn(move || {
        let (p, _) = a
            .prepare_preview(owner, |c| {
                c.edit_deletion(&request(&[10]), Default::default(), &Default::default())
            })
            .unwrap();
        (a, p)
    })
    .join()
    .unwrap();
    let mut b = s.begin_draft();
    assert_eq!(
        b.install_preview(preview).unwrap_err().code.as_str(),
        "STALE_SOURCE"
    );
    assert_eq!(a.source(), owner);
}

#[test]
fn accepted_noop_advances_candidate_sequence_and_expires_deletion_plans() {
    let s = fixture();
    let mut draft = s.begin_draft();
    let (p, _) = draft
        .prepare_preview(draft.source(), |c| {
            c.edit_deletion(&request(&[10]), Default::default(), &Default::default())
        })
        .unwrap();
    let before = draft.read_snapshot().unwrap();
    draft.batch(draft.source(), |_| Ok(())).unwrap();
    assert_eq!(
        before.snapshot.content_id(),
        draft.read_snapshot().unwrap().snapshot.content_id()
    );
    assert_eq!(
        draft.install_preview(p).unwrap_err().code.as_str(),
        "STALE_SOURCE"
    );
}

#[test]
fn can_plan_after_an_earlier_edit_and_publish_both_atomically() {
    let mut s = fixture();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut mesh = e.read_document().unwrap().mesh(&id(11)).unwrap().clone();
                mesh.name = "kept".into();
                e.replace_mesh(mesh)?;
                e.edit_deletion(&request(&[10]), Default::default(), &Default::default())
            })
            .and_then(|value| {
                let prepared = draft.prepare("composed")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(s.snapshot().document().mesh(&id(11)).unwrap().name, "kept");
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().mesh(&id(11)).unwrap().name, "mesh");
    assert!(s.snapshot().document().mesh(&id(10)).is_some());
}

#[test]
fn budget_rejection_and_cancel_preserve_source_and_redo() {
    let mut s = fixture();
    let (_, p) = crate::support::prepared(&s, |c| {
        c.rename(zipzi_sdk::NameTarget::Mesh(&id(12)), "redo".into())
    })
    .unwrap();
    s.publish(p).unwrap();
    s.undo(s.version()).unwrap();
    let source = s.read_snapshot().source;
    let h = s.history_state();
    assert_eq!(
        crate::support::prepared(&s, |c| c.edit_deletion(
            &request(&[10]),
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
    let control = ConversionControl::with_progress(|n| n < 5);
    assert_eq!(
        crate::support::prepared(&s, |c| c.edit_deletion(
            &request(&[10]),
            Default::default(),
            &control
        ))
        .unwrap_err()
        .code
        .as_str(),
        "AUTHORING_CANCELLED"
    );
    let (_, p) = prepare_deletion(&s, &request(&[10])).unwrap();
    assert_eq!(
        s.publish_with_control(p, || false)
            .unwrap_err()
            .code
            .as_str(),
        "CANCELLED"
    );
    assert_eq!(s.read_snapshot().source, source);
    assert_eq!(s.history_state(), h);
}

#[test]
fn model3_namespace_regression_requires_explicit_acceptance_and_keeps_provenance() {
    let mut s = fixture();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut settings = e.read_document().unwrap().model3_settings().clone();
                settings
                    .extensions
                    .insert("Vendor".into(), serde_json::json!({"Mesh": "external"}));
                settings.source_runtime_ids = Some(zipzi_project::model3::runtime_namespace(
                    &e.read_document().unwrap(),
                ));
                e.replace_model3_settings(settings)
            })
            .and_then(|value| {
                let prepared = draft.prepare("opaque provenance")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.snapshot().document().clone();
    let analysis = analyze(&s, &request(&[10])).unwrap();
    assert_eq!(
        analysis.1,
        [zipzi_project::OpaqueExportIssue::Model3NamespaceChanged].into()
    );
    assert!(!analysis.1.is_empty());
    let plan = prepare_deletion(&s, &request(&[10])).unwrap();
    s.publish(plan.1).unwrap();
    assert_eq!(
        s.snapshot().document().model3_settings().source_runtime_ids,
        before.model3_settings().source_runtime_ids
    );
    assert_eq!(
        s.snapshot().document().model3_settings().extensions,
        before.model3_settings().extensions
    );
    assert_eq!(
        zipzi_project::model3::export_settings(s.snapshot().document())
            .unwrap_err()
            .code,
        "MODEL3_NAMESPACE_CHANGED"
    );
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
    assert_eq!(
        zipzi_project::model3::export_settings(s.snapshot().document())
            .unwrap_err()
            .code,
        "UNSUPPORTED_MODEL3_FIELD"
    );
}

#[test]
fn userdata_namespace_loss_is_reported_even_without_a_hit_area_and_existing_loss_is_not_new() {
    let mut s = fixture();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut settings = e.read_document().unwrap().model3_settings().clone();
                settings.hit_areas = None;
                settings.user_data = Some(AttachmentRef::Unresolved {
                    path: "model.userdata3.json".into(),
                });
                settings.source_runtime_ids = Some(zipzi_project::model3::runtime_namespace(
                    &e.read_document().unwrap(),
                ));
                e.replace_model3_settings(settings)
            })
            .and_then(|value| {
                let prepared = draft.prepare("userdata")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(zipzi_project::model3::export_settings(s.snapshot().document()).is_ok());
    assert_eq!(
        analyze(&s, &request(&[10])).unwrap().1,
        [zipzi_project::OpaqueExportIssue::Model3NamespaceChanged].into()
    );
    let plan = prepare_deletion(&s, &request(&[10])).unwrap();
    s.publish(plan.1).unwrap();
    assert_eq!(
        zipzi_project::model3::export_settings(s.snapshot().document())
            .unwrap_err()
            .code,
        "MODEL3_NAMESPACE_CHANGED"
    );
    // A second deletion does not ask to accept the already broken namespace again.
    assert!(analyze(&s, &request(&[12])).unwrap().1.is_empty());
    let second = prepare_deletion(&s, &request(&[12])).unwrap();
    s.publish(second.1).unwrap();
    assert_eq!(
        zipzi_project::model3::export_settings(s.snapshot().document())
            .unwrap_err()
            .code,
        "MODEL3_NAMESPACE_CHANGED"
    );
}

#[test]
fn missing_and_wrong_kind_roots_are_rejected_and_strict_deletion_stays_strict() {
    let mut s = fixture();
    assert_eq!(
        prepare_deletion(&s, &request(&[999]))
            .unwrap_err()
            .code
            .as_str(),
        "MISSING_DELETION_TARGET"
    );
    assert_eq!(
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    e.edit_scene(
                        SceneEdit::Delete {
                            ids: vec![id(10)],
                            scope: Default::default(),
                        },
                        Default::default(),
                        &Default::default(),
                    )
                })
                .and_then(|value| {
                    let prepared = draft.prepare("strict")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_REFERENCED"
    );
    let wrong = DeletionRequest {
        roots: vec![DeletionTarget::Glue(id(10))],
        ..Default::default()
    };
    assert_eq!(
        prepare_deletion(&s, &wrong).unwrap_err().code.as_str(),
        "MISSING_DELETION_TARGET"
    );
}

#[test]
fn native_plan_rejects_a_different_branch_with_equal_revision() {
    let s = fixture();
    let mut a = s.begin_draft();
    let mut b = s.begin_draft();
    a.batch(a.source(), |c| {
        c.rename(zipzi_sdk::NameTarget::Mesh(&id(11)), "left".into())
    })
    .unwrap();
    b.batch(b.source(), |c| {
        c.rename(zipzi_sdk::NameTarget::Mesh(&id(11)), "right".into())
    })
    .unwrap();
    let (p, _) = a
        .prepare_preview(a.source(), |c| {
            c.edit_deletion(&request(&[10]), Default::default(), &Default::default())
        })
        .unwrap();
    assert_eq!(a.source().sequence, b.source().sequence);
    assert_eq!(
        b.install_preview(p).unwrap_err().code.as_str(),
        "STALE_SOURCE"
    );
    assert!(b.document().unwrap().mesh(&id(10)).is_some());
}

#[test]
fn removes_owned_blend_bindings_but_keeps_locked_shared_tables_and_constraints() {
    let mut s = fixture();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(60),
                        minimum: 0.,
                        maximum: 1.,
                        kind: ParameterKind::BlendShape,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                e.edit_blend_shape(
                    zipzi_sdk::BlendAuthoringEdit::CreateTable {
                        table: BlendShapeKeyTable {
                            id: id(61),
                            parameter_id: id(60),
                            keys: vec![0., 1.],
                            base_key_idx: 0,
                            samples: vec![],
                            base_sample_id: String::new(),
                        },
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())?;
                e.edit_blend_shape(
                    zipzi_sdk::BlendAuthoringEdit::CreateConstraint {
                        constraint: BlendShapeConstraint {
                            id: id(62),
                            parameter_id: id(3),
                            keys: vec![0., 1.],
                            weights: vec![1., 1.],
                            samples: vec![],
                        },
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())?;
                for (binding, mesh) in [(63, 10), (64, 12)] {
                    {
                        let binding = BlendShapeBinding {
                            id: id(binding),
                            target_id: id(mesh),
                            target_kind: BlendShapeTargetKind::Mesh,
                            key_table_id: id(61),
                            constraint_ids: vec![id(62)],
                            keyforms: DeltaKeyforms::Mesh(vec![
                                DeltaMeshKeyform {
                                    positions: vec![Vec2::default(); 4],
                                    ..Default::default()
                                };
                                2
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
                }
                Ok(())
            })
            .and_then(|value| {
                let prepared = draft.prepare("blend")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    lock(&mut s, &id(61));
    lock(&mut s, &id(62));
    let before = s.snapshot().document().clone();
    let plan = prepare_deletion(&s, &request(&[10])).unwrap();
    assert!(plan
        .0
        .report
        .removed
        .iter()
        .any(|r| r.object.id == id(63) && r.reason == DeletionReason::Owned));
    s.publish(plan.1).unwrap();
    assert!(s.snapshot().document().blend_binding(&id(63)).is_none());
    assert_eq!(
        s.snapshot().document().blend_binding(&id(64)),
        before.blend_binding(&id(64))
    );
    assert_eq!(
        s.snapshot().document().blend_key_table(&id(61)),
        before.blend_key_table(&id(61))
    );
    assert_eq!(
        s.snapshot().document().blend_constraint(&id(62)),
        before.blend_constraint(&id(62))
    );
}

#[test]
fn inherited_offscreen_lock_blocks_mask_rewrite() {
    let mut s = fixture();
    lock(&mut s, &id(4));
    let analysis = analyze(&s, &request(&[10])).unwrap();
    assert!(analysis
        .0
        .blockers
        .iter()
        .any(|b| b.object.id == id(40) && b.source_id == id(4)));
    assert_eq!(
        prepare_deletion(&s, &request(&[10]))
            .unwrap_err()
            .code
            .as_str(),
        "OBJECT_LOCKED"
    );
}
