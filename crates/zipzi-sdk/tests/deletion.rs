use zipzi_core::{document::*, *};
mod business;
mod review;
mod scene;
use zipzi_sdk::{
    rectangle_mesh, AuthoringSession, DeletionExportFailure, DeletionOptions, ObjectKind,
};

fn id(n: u32) -> String {
    format!("30000000-0000-4000-8000-{n:012x}")
}
fn fixture() -> AuthoringSession {
    let mut s =
        AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 1.)).unwrap();
    s.edit("fixture", None, |e| {
        e.create_asset(ImageAsset {
            id: id(2),
            name: "shared".into(),
            content: Some(ImageContent {
                source: "assets/shared.png".into(),
                sha256: "0".repeat(64),
                width: 1,
                height: 1,
            }),
            unresolved_source: None,
        })?;
        e.create_parameter(Parameter {
            id: id(3),
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })?;
        e.create_part(Part {
            id: id(4),
            ..Default::default()
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
                .candidate_document()
                .get_mesh(&id(n))
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
        let mut b = e.candidate_document().get_mesh(&id(11)).unwrap().clone();
        b.masks = vec![id(12), id(10)];
        b.inverted_mask = true;
        e.replace_mesh(b)?;
        e.create_glue(Glue {
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
        })?;
        e.create_offscreen(Offscreen {
            id: id(40),
            runtime_id: "composite".into(),
            part_id: id(4),
            masks: vec![id(10), id(12)],
            ..Default::default()
        })?;
        e.set_model3_settings(Model3Settings {
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
    s.edit("lock", None, |e| {
        let mut flags = e.candidate_document().editor_state().clone();
        flags.objects.insert(
            id.into(),
            ObjectEditorState {
                locked: true,
                ..Default::default()
            },
        );
        e.replace_editor_state(flags)
    })
    .unwrap();
}

#[test]
fn cleans_mask_glue_hit_area_and_owned_identities_in_one_history_step() {
    let mut s = fixture();
    assert_eq!(
        s.evaluate(&PreviewValues::default())
            .unwrap()
            .drawables
            .len(),
        3
    );
    let before = s.document().clone();
    let mesh = s.handle(ObjectKind::Mesh, &id(10)).unwrap();
    let sample_id = s.document().get_binding(&id(20)).unwrap().axes[0].samples[0]
        .id
        .clone();
    let sample = s.handle(ObjectKind::BindingSample, &sample_id).unwrap();
    let request = s.deletion_request(&[mesh.clone()]).unwrap();
    let plan = s
        .plan_deletion(&request, &DeletionOptions::default())
        .unwrap();
    assert!(plan.report().native.requires_confirmation());
    assert!(s.document().same_content(&before));
    let receipt = s.execute_deletion_plan(&plan).unwrap();
    assert!(receipt.changed);
    assert!(receipt.changes.deleted.iter().any(|v| v.id == id(10)));
    assert!(receipt.changes.deleted.iter().any(|v| v.id == id(31)));
    assert!(receipt
        .changes
        .reference_selections
        .iter()
        .any(|v| v.entity.id == id(11)));
    assert!(s.document().get_mesh(&id(10)).is_none());
    assert!(s.document().get_glue(&id(30)).is_none());
    assert!(s.document().get_binding(&id(20)).is_none());
    assert_eq!(s.document().get_mesh(&id(11)).unwrap().masks, vec![id(12)]);
    assert!(s.document().get_mesh(&id(11)).unwrap().inverted_mask);
    assert_eq!(
        s.document().get_offscreen(&id(40)).unwrap().masks,
        vec![id(12)]
    );
    assert_eq!(
        s.document()
            .model3_settings()
            .hit_areas
            .as_ref()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(s.document().get_mesh(&id(12)), before.get_mesh(&id(12)));
    assert_eq!(
        s.document().get_binding(&id(22)),
        before.get_binding(&id(22))
    );
    assert_eq!(s.document().get_asset(&id(2)), before.get_asset(&id(2)));
    assert!(s.document().validate_structure().is_empty());
    assert_eq!(
        s.evaluate(&PreviewValues::default())
            .unwrap()
            .drawables
            .len(),
        2
    );
    s.resolve_handle(&mesh).unwrap_err();
    s.resolve_handle(&sample).unwrap_err();
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
    assert_eq!(
        s.evaluate(&PreviewValues::default())
            .unwrap()
            .drawables
            .len(),
        3
    );
    s.resolve_handle(&mesh).unwrap_err();
    s.resolve_handle(&sample).unwrap_err();
    assert!(s.execute_deletion_plan(&plan).is_err());
    s.redo().unwrap();
    assert!(s.document().get_mesh(&id(10)).is_none());
}

#[test]
fn multi_root_order_duplicates_cycles_and_mask_normalization() {
    let mut s = fixture();
    s.edit("cycle", None, |e| {
        let mut a = e.candidate_document().get_mesh(&id(10)).unwrap().clone();
        a.masks = vec![id(12)];
        e.replace_mesh(a)?;
        let mut c = e.candidate_document().get_mesh(&id(12)).unwrap().clone();
        c.masks = vec![id(10)];
        e.replace_mesh(c)
    })
    .unwrap();
    let a = s.analyze_deletion(&request(&[10, 12, 10])).unwrap();
    let b = s.analyze_deletion(&request(&[12, 10])).unwrap();
    assert_eq!(a.native, b.native);
    assert_eq!(
        a.native
            .removed
            .iter()
            .filter(|v| v.object.id == id(30) && v.object.kind == DeletionKind::Glue)
            .count(),
        1
    );
    // A removed Mask consumer gets no useless external unbind operation.
    assert!(!a
        .native
        .rewritten
        .iter()
        .any(|r| r.owner.id == id(10) || r.owner.id == id(12)));
    let plan = s
        .plan_deletion(&request(&[10, 12]), &DeletionOptions::default())
        .unwrap();
    s.execute_deletion_plan(&plan).unwrap();
    let b = s.document().get_mesh(&id(11)).unwrap();
    assert!(b.masks.is_empty());
    assert!(!b.inverted_mask);
    assert!(s
        .document()
        .get_offscreen(&id(40))
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
            s.document().get_binding(&id(20)).unwrap().axes[0].samples[0]
                .id
                .clone()
        } else if target == "glue_sample" {
            s.document()
                .get_glue(&id(30))
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
        let before = s.document().clone();
        let history = s.history_state();
        let analysis = s.analyze_deletion(&request(&[10])).unwrap();
        assert!(analysis
            .native
            .blockers
            .iter()
            .any(|b| b.source_id == target));
        assert_eq!(
            s.plan_deletion(&request(&[10]), &DeletionOptions::default())
                .unwrap_err()
                .code
                .as_ref(),
            "OBJECT_LOCKED"
        );
        assert!(s.document().same_content(&before));
        assert_eq!(s.history_state(), history);
    }
}

#[test]
fn locked_read_only_shared_resources_and_unrelated_binding_do_not_block() {
    let mut s = fixture();
    for target in [id(2), id(3), id(22)] {
        lock(&mut s, &target);
    }
    let plan = s
        .plan_deletion(&request(&[10]), &DeletionOptions::default())
        .unwrap();
    s.execute_deletion_plan(&plan).unwrap();
    assert!(s.document().get_mesh(&id(12)).is_some());
}

#[test]
fn glue_and_offscreen_roots_preserve_endpoint_meshes_and_part_binding() {
    let mut s = fixture();
    let before = s.document().clone();
    let request = DeletionRequest {
        roots: vec![
            DeletionTarget::Glue(id(30)),
            DeletionTarget::Offscreen(id(40)),
        ],
        ..Default::default()
    };
    let plan = s
        .plan_deletion(&request, &DeletionOptions::default())
        .unwrap();
    s.execute_deletion_plan(&plan).unwrap();
    for n in 10..=12 {
        assert_eq!(s.document().get_mesh(&id(n)), before.get_mesh(&id(n)));
    }
    assert_eq!(s.document().get_part(&id(4)), before.get_part(&id(4)));
    assert_eq!(
        s.document().get_scene_binding(&id(5)),
        before.get_scene_binding(&id(5))
    );
    assert!(s.document().get_glue(&id(30)).is_none());
    assert!(s.document().get_offscreen(&id(40)).is_none());
}

#[test]
fn plans_reject_changed_candidate_even_when_formal_version_has_not_changed() {
    let s = fixture();
    let mut e = s.begin_owned_edit("candidate", None).unwrap();
    let plan = e
        .plan_deletion(&request(&[10]), &DeletionOptions::default())
        .unwrap();
    let mut mesh = e.candidate_document().get_mesh(&id(11)).unwrap().clone();
    mesh.name = "changed".into();
    e.replace_mesh(mesh).unwrap();
    assert_eq!(
        e.apply_deletion_plan(&plan).unwrap_err().code.as_ref(),
        "STALE_PLAN"
    );
    assert!(e.is_aborted());
}

#[test]
fn candidate_plan_cannot_escape_to_another_edit_with_identical_source() {
    let mut s = fixture();
    let a = s.begin_owned_edit("first", None).unwrap();
    let plan = a
        .plan_deletion(&request(&[10]), &DeletionOptions::default())
        .unwrap();
    let mut b = s.begin_owned_edit("second", None).unwrap();
    assert_eq!(
        b.apply_deletion_plan(&plan).unwrap_err().code.as_ref(),
        "STALE_PLAN"
    );
    assert!(s.execute_deletion_plan(&plan).is_err());
}

#[test]
fn background_candidate_plan_retains_the_edit_identity() {
    let s = fixture();
    let a = s.begin_owned_edit("first", None).unwrap();
    let snapshot = s.candidate_snapshot(&a).unwrap();
    let plan = snapshot
        .plan_deletion_with_control(
            &request(&[10]),
            &DeletionOptions::default(),
            AuthoringLimits::default(),
            &ConversionControl::default(),
        )
        .unwrap();
    let mut b = s.begin_owned_edit("second", None).unwrap();
    assert_eq!(
        b.apply_deletion_plan(&plan).unwrap_err().code.as_ref(),
        "STALE_PLAN"
    );
}

#[test]
fn accepted_noop_advances_candidate_sequence_and_expires_deletion_plans() {
    let mut s = fixture();
    let mut a = s.begin_owned_edit("candidate", None).unwrap();
    let plan = a
        .plan_deletion(&request(&[10]), &DeletionOptions::default())
        .unwrap();
    let before = a.candidate_document().clone();
    a.rename_mesh(&id(11), "mesh").unwrap();
    assert!(a.candidate_document().is_same_snapshot(&before));
    assert_eq!(
        a.apply_deletion_plan(&plan).unwrap_err().code.as_ref(),
        "STALE_PLAN"
    );
    assert!(a.commit_to(&mut s).is_err());
}

#[test]
fn can_plan_after_an_earlier_edit_and_publish_both_atomically() {
    let mut s = fixture();
    s.edit("composed", None, |e| {
        let mut mesh = e.candidate_document().get_mesh(&id(11)).unwrap().clone();
        mesh.name = "kept".into();
        e.replace_mesh(mesh)?;
        let plan = e.plan_deletion(&request(&[10]), &DeletionOptions::default())?;
        e.apply_deletion_plan(&plan)
    })
    .unwrap();
    assert_eq!(s.document().get_mesh(&id(11)).unwrap().name, "kept");
    s.undo().unwrap();
    assert_eq!(s.document().get_mesh(&id(11)).unwrap().name, "mesh");
    assert!(s.document().get_mesh(&id(10)).is_some());
}

#[test]
fn budget_rejection_and_cancel_preserve_source_and_redo() {
    let mut s = fixture();
    s.edit("redo seed", None, |e| e.rename_mesh(&id(12), "redo"))
        .unwrap();
    s.undo().unwrap();
    let before = s.document().clone();
    let history = s.history_state();
    let snapshot = s.read_snapshot();
    let tiny = AuthoringLimits {
        max_conversion_bytes: 1,
        ..Default::default()
    };
    assert_eq!(
        snapshot
            .plan_deletion_with_control(
                &request(&[10]),
                &DeletionOptions::default(),
                tiny,
                &ConversionControl::default()
            )
            .unwrap_err()
            .code
            .as_ref(),
        "AUTHORING_MEMORY_LIMIT"
    );
    let control = ConversionControl::with_progress(|n| n < 5);
    assert_eq!(
        snapshot
            .plan_deletion_with_control(
                &request(&[10]),
                &DeletionOptions::default(),
                AuthoringLimits::default(),
                &control
            )
            .unwrap_err()
            .code
            .as_ref(),
        "AUTHORING_CANCELLED"
    );
    let control = ConversionControl::default();
    let plan = snapshot
        .plan_deletion_with_control(
            &request(&[10]),
            &DeletionOptions::default(),
            AuthoringLimits::default(),
            &control,
        )
        .unwrap();
    let mut edit = s.begin_owned_edit("cancel after install", None).unwrap();
    edit.apply_deletion_plan(&plan).unwrap();
    control.cancel();
    assert!(edit.commit_to(&mut s).is_err());
    assert!(s.document().same_content(&before));
    assert_eq!(s.history_state(), history);
}

#[test]
fn model3_namespace_regression_requires_explicit_acceptance_and_keeps_provenance() {
    let mut s = fixture();
    s.edit("opaque provenance", None, |e| {
        let mut settings = e.candidate_document().model3_settings().clone();
        settings
            .extensions
            .insert("Vendor".into(), serde_json::json!({"Mesh": "external"}));
        settings.source_runtime_ids = Some(zipzi_project::model3::runtime_namespace(
            e.candidate_document(),
        ));
        e.set_model3_settings(settings)
    })
    .unwrap();
    let before = s.document().clone();
    let analysis = s.analyze_deletion(&request(&[10])).unwrap();
    assert_eq!(
        analysis.new_export_failures,
        [DeletionExportFailure::Model3NamespaceChanged].into()
    );
    assert_eq!(
        s.plan_deletion(&request(&[10]), &DeletionOptions::default())
            .unwrap_err()
            .code
            .as_ref(),
        "DELETION_EXPORT_DECISION_REQUIRED"
    );
    let options = DeletionOptions {
        accepted_export_failures: analysis.new_export_failures,
    };
    let plan = s.plan_deletion(&request(&[10]), &options).unwrap();
    s.execute_deletion_plan(&plan).unwrap();
    assert_eq!(
        s.document().model3_settings().source_runtime_ids,
        before.model3_settings().source_runtime_ids
    );
    assert_eq!(
        s.document().model3_settings().extensions,
        before.model3_settings().extensions
    );
    assert_eq!(
        zipzi_project::model3::export_settings(s.document())
            .unwrap_err()
            .code,
        "MODEL3_NAMESPACE_CHANGED"
    );
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
    assert_eq!(
        zipzi_project::model3::export_settings(s.document())
            .unwrap_err()
            .code,
        "UNSUPPORTED_MODEL3_FIELD"
    );
}

#[test]
fn userdata_namespace_loss_is_reported_even_without_a_hit_area_and_existing_loss_is_not_new() {
    let mut s = fixture();
    s.edit("userdata", None, |e| {
        let mut settings = e.candidate_document().model3_settings().clone();
        settings.hit_areas = None;
        settings.user_data = Some(AttachmentRef::Unresolved {
            path: "model.userdata3.json".into(),
        });
        settings.source_runtime_ids = Some(zipzi_project::model3::runtime_namespace(
            e.candidate_document(),
        ));
        e.set_model3_settings(settings)
    })
    .unwrap();
    assert!(zipzi_project::model3::export_settings(s.document()).is_ok());
    assert_eq!(
        s.analyze_deletion(&request(&[10]))
            .unwrap()
            .new_export_failures,
        [DeletionExportFailure::Model3NamespaceChanged].into()
    );
    let plan = s
        .plan_deletion(
            &request(&[10]),
            &DeletionOptions {
                accepted_export_failures: [DeletionExportFailure::Model3NamespaceChanged].into(),
            },
        )
        .unwrap();
    s.execute_deletion_plan(&plan).unwrap();
    assert_eq!(
        zipzi_project::model3::export_settings(s.document())
            .unwrap_err()
            .code,
        "MODEL3_NAMESPACE_CHANGED"
    );
    // A second deletion does not ask to accept the already broken namespace again.
    assert!(s
        .analyze_deletion(&request(&[12]))
        .unwrap()
        .new_export_failures
        .is_empty());
    let second = s
        .plan_deletion(&request(&[12]), &DeletionOptions::default())
        .unwrap();
    s.execute_deletion_plan(&second).unwrap();
    assert_eq!(
        zipzi_project::model3::export_settings(s.document())
            .unwrap_err()
            .code,
        "MODEL3_NAMESPACE_CHANGED"
    );
}

#[test]
fn missing_and_wrong_kind_roots_are_rejected_and_strict_deletion_stays_strict() {
    let mut s = fixture();
    assert_eq!(
        s.plan_deletion(&request(&[999]), &DeletionOptions::default())
            .unwrap_err()
            .code
            .as_ref(),
        "MISSING_DELETION_TARGET"
    );
    let part = s.handle(ObjectKind::Parameter, &id(3)).unwrap();
    assert_eq!(
        s.deletion_request(&[part]).unwrap().roots,
        vec![DeletionTarget::Parameter(id(3))]
    );
    assert_eq!(
        s.edit("strict", None, |e| e.erase_object(&id(10)))
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_REFERENCED"
    );
    let wrong = DeletionRequest {
        roots: vec![DeletionTarget::Glue(id(10))],
        ..Default::default()
    };
    assert_eq!(
        s.plan_deletion(&wrong, &DeletionOptions::default())
            .unwrap_err()
            .code
            .as_ref(),
        "MISSING_DELETION_TARGET"
    );
}

#[test]
fn native_plan_rejects_a_different_branch_with_equal_revision() {
    let s = fixture();
    let mut a = s.document().fork_candidate();
    let mut b = s.document().fork_candidate();
    assert!(a.rename_mesh(&id(11), "left".into()).status.is_ok());
    assert!(b.rename_mesh(&id(11), "right".into()).status.is_ok());
    let plan = a
        .plan_deletion(
            &request(&[10]),
            AuthoringLimits::default(),
            &ConversionControl::default(),
        )
        .unwrap();
    assert_eq!(b.apply_deletion_plan(&plan).status.code, "STALE_PLAN");
    assert!(b.get_mesh(&id(10)).is_some());
    let applied = a.apply_deletion_plan(&plan);
    assert!(applied.status.is_ok());
    assert!(applied.changes.mesh_ids.contains(&id(10)));
    assert_eq!(a.apply_deletion_plan(&plan).status.code, "STALE_PLAN");
}

#[test]
fn removes_owned_blend_bindings_but_keeps_locked_shared_tables_and_constraints() {
    let mut s = fixture();
    s.edit("blend", None, |e| {
        e.create_parameter(Parameter {
            id: id(60),
            minimum: 0.,
            maximum: 1.,
            kind: ParameterKind::BlendShape,
            ..Default::default()
        })?;
        e.create_blend_key_table(BlendShapeKeyTable {
            id: id(61),
            parameter_id: id(60),
            keys: vec![0., 1.],
            base_key_idx: 0,
            samples: vec![],
            base_sample_id: String::new(),
        })?;
        e.create_blend_constraint(BlendShapeConstraint {
            id: id(62),
            parameter_id: id(3),
            keys: vec![0., 1.],
            weights: vec![1., 1.],
            samples: vec![],
        })?;
        for (binding, mesh) in [(63, 10), (64, 12)] {
            e.create_blend_binding(BlendShapeBinding {
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
            })?;
        }
        Ok(())
    })
    .unwrap();
    lock(&mut s, &id(61));
    lock(&mut s, &id(62));
    let before = s.document().clone();
    let plan = s
        .plan_deletion(&request(&[10]), &DeletionOptions::default())
        .unwrap();
    assert!(plan
        .report()
        .native
        .removed
        .iter()
        .any(|r| r.object.id == id(63) && r.reason == DeletionReason::Owned));
    s.execute_deletion_plan(&plan).unwrap();
    assert!(s.document().get_blend_binding(&id(63)).is_none());
    assert_eq!(
        s.document().get_blend_binding(&id(64)),
        before.get_blend_binding(&id(64))
    );
    assert_eq!(
        s.document().get_blend_key_table(&id(61)),
        before.get_blend_key_table(&id(61))
    );
    assert_eq!(
        s.document().get_blend_constraint(&id(62)),
        before.get_blend_constraint(&id(62))
    );
}

#[test]
fn inherited_offscreen_lock_blocks_mask_rewrite() {
    let mut s = fixture();
    lock(&mut s, &id(4));
    let analysis = s.analyze_deletion(&request(&[10])).unwrap();
    assert!(analysis
        .native
        .blockers
        .iter()
        .any(|b| b.object.id == id(40) && b.source_id == id(4)));
    assert_eq!(
        s.plan_deletion(&request(&[10]), &DeletionOptions::default())
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
}
