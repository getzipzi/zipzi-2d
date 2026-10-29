use super::*;
use zipzi_sdk::{
    DeletionChoice, DeletionDecisionReason, DeletionOffscreenPlacement, OpaqueExportProblem,
};

pub(super) fn tree() -> AuthoringSession {
    let mut s = fixture();
    s.edit("cross-part tree", None, |e| {
        e.create_part(Part {
            id: id(90),
            runtime_id: "Outside".into(),
            ..Default::default()
        })?;
        e.create_transform(Transform {
            id: id(91),
            runtime_id: "Parent".into(),
            part_id: Some(id(4).into()),
            data: TransformData::Rotation(RotationTransform {
                base_angle: 0.,
                pose: RotationPose {
                    angle: 30.,
                    ..Default::default()
                },
            }),
            ..Default::default()
        })?;
        e.set_mesh_part(&id(12), &id(90))?;
        e.set_deform_parent(&id(12), &id(91))
    })
    .unwrap();
    s
}
fn roots(roots: Vec<DeletionTarget>) -> DeletionRequest {
    DeletionRequest {
        roots,
        ..Default::default()
    }
}
fn positions(s: &AuthoringSession) -> Vec<Vec2> {
    s.evaluate(&PreviewValues::default())
        .unwrap()
        .drawables
        .into_iter()
        .find(|d| d.id == id(12))
        .unwrap()
        .positions
}
fn clip() -> MotionClip {
    MotionClip {
        id: id(100),
        name: "Motion".into(),
        duration: 1.,
        fps: 30.,
        looping: false,
        restricted_beziers: true,
        fade_in: None,
        fade_out: None,
        track_order: vec![id(101), id(103)],
        tracks: vec![
            MotionTrack {
                id: id(101),
                target: MotionTrackTarget::PartOpacity { part_id: id(4) },
                initial: MotionKey {
                    id: id(102),
                    time: 0.,
                    value: 1.,
                },
                segments: std::sync::Arc::new(vec![MotionSegment::Linear {
                    end: MotionKey {
                        id: id(106),
                        time: 1.,
                        value: 0.,
                    },
                }]),
                fade_in: None,
                fade_out: None,
                extensions: Default::default(),
            },
            MotionTrack {
                id: id(103),
                target: MotionTrackTarget::Parameter {
                    parameter_id: id(3),
                },
                initial: MotionKey {
                    id: id(104),
                    time: 0.,
                    value: 0.,
                },
                segments: Default::default(),
                fade_in: None,
                fade_out: None,
                extensions: Default::default(),
            },
        ]
        .into(),
        events: vec![MotionEvent {
            id: id(105),
            time: 0.5,
            value: "event".into(),
            extensions: Default::default(),
        }]
        .into(),
        extensions: Default::default(),
        meta_extensions: Default::default(),
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    }
}
pub(super) fn animation(s: &mut AuthoringSession) {
    s.edit("cross-domain references", None, |e| {
        e.create_motion(clip())?;
        let mut pose = PoseAsset::empty(id(110));
        pose.groups = vec![PoseGroup {
            id: id(111),
            name: "Group".into(),
            entries: vec![
                PoseEntry {
                    id: id(112),
                    part: PosePartRef::Resolved { part_id: id(4) },
                    links: Some(
                        vec![PoseLink {
                            id: id(113),
                            target: PosePartRef::Resolved { part_id: id(90) },
                        }]
                        .into(),
                    ),
                    extensions: Default::default(),
                },
                PoseEntry {
                    id: id(114),
                    part: PosePartRef::Resolved { part_id: id(90) },
                    links: Some(
                        vec![PoseLink {
                            id: id(115),
                            target: PosePartRef::Resolved { part_id: id(4) },
                        }]
                        .into(),
                    ),
                    extensions: Default::default(),
                },
            ]
            .into(),
        }]
        .into();
        e.set_pose(pose)?;
        let mut info = e.candidate_document().display_info().clone();
        info.parts = Some(vec![
            CdiPartEntry::Resolved {
                part_id: id(4),
                extensions: Default::default(),
            },
            CdiPartEntry::Unresolved {
                runtime_id: "Legacy".into(),
                name: "Keep".into(),
                extensions: Default::default(),
            },
        ]);
        e.replace_display_info(info)?;
        let mut expression = ExpressionAsset {
            id: id(120),
            name: "Expression".into(),
            file_type: None,
            fade_in: None,
            fade_out: None,
            entries: vec![],
            extensions: Default::default(),
            opaque_source_ids: None,
            opaque_source_content_hash: None,
        };
        let mut scene = clip();
        scene.id = expression.id.clone();
        scene.name = "Source".into();
        scene.events.clear();
        scene.track_order.clear();
        scene.tracks[0].id = id(121);
        scene.tracks[0].initial.id = id(122);
        scene.tracks[0].segments = Default::default();
        scene.tracks[1].id = id(123);
        scene.tracks[1].initial.id = id(124);
        expression.preserve_source_scene(&scene);
        e.create_expression(expression)
    })
    .unwrap();
}

#[test]
fn part_deletion_cleans_pose_motion_expression_and_cdi_while_preserving_external_mesh_pose() {
    let mut s = tree();
    animation(&mut s);
    let before = s.document().clone();
    let pose_before = positions(&s);
    let plan = s
        .plan_deletion(
            &roots(vec![DeletionTarget::Part(id(4))]),
            &DeletionOptions::default(),
        )
        .unwrap();
    assert!(plan
        .report()
        .native
        .migrations
        .iter()
        .any(|m| m.object.id == id(12) && m.guarantee.is_some()));
    let receipt = s.execute_deletion_plan(&plan).unwrap();
    assert!(s.document().get_part(&id(4)).is_none());
    assert!(s.document().get_transform(&id(91)).is_none());
    assert!(s.document().get_part(&id(90)).is_some());
    assert!(s.document().get_mesh(&id(12)).is_some());
    assert!(s
        .document()
        .get_mesh(&id(12))
        .unwrap()
        .deformer_id
        .is_empty());
    for (a, b) in positions(&s).iter().zip(&pose_before) {
        assert!((a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4);
    }
    let motion = s.document().get_motion(&id(100)).unwrap();
    assert_eq!(motion.tracks.len(), 1);
    assert_eq!(motion.tracks[0].id, id(103));
    assert_eq!(motion.track_order, vec![id(103)]);
    assert_eq!(motion.events, before.get_motion(&id(100)).unwrap().events);
    let group = &s.document().pose().unwrap().groups[0];
    assert_eq!(group.entries.len(), 1);
    assert_eq!(group.entries[0].id, id(114));
    assert!(group.entries[0].links.as_ref().unwrap().is_empty());
    assert_eq!(
        s.document()
            .get_expression(&id(120))
            .unwrap()
            .source_scene()
            .unwrap()
            .tracks
            .len(),
        1
    );
    assert!(
        matches!(&s.document().display_info().parts.as_ref().unwrap()[0], CdiPartEntry::Unresolved { runtime_id, .. } if runtime_id == "Legacy")
    );
    assert!(receipt.changes.deleted.iter().any(|o| o.id == id(101)));
    assert!(receipt.changes.deleted.iter().any(|o| o.id == id(112)));
    let encoded = zipzi_project::encode_project(s.document()).unwrap();
    let reopened = zipzi_project::decode_project(&encoded).unwrap();
    assert!(reopened.validate_structure().is_empty());
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
    s.redo().unwrap();
    assert!(s.document().get_part(&id(4)).is_none());
}

#[test]
fn explicit_deformer_scope_is_unioned_before_parent_roots_are_deduplicated() {
    let s = tree();
    let a = s
        .analyze_deletion(&roots(vec![
            DeletionTarget::Part(id(4)),
            DeletionTarget::Deformer(id(91)),
        ]))
        .unwrap();
    let b = s
        .analyze_deletion(&roots(vec![
            DeletionTarget::Deformer(id(91)),
            DeletionTarget::Part(id(4)),
        ]))
        .unwrap();
    assert_eq!(a.native, b.native);
    assert!(a.native.removed.iter().any(|r| r.object.id == id(12)));
    assert!(!a.native.removed.iter().any(|r| r.object.id == id(90)));
    assert!(a.native.migrations.is_empty());
    let handle = s.handle(ObjectKind::Transform, &id(91)).unwrap();
    assert_eq!(
        s.deletion_request(&[handle]).unwrap().roots,
        vec![DeletionTarget::Deformer(id(91))]
    );
}

#[test]
fn nonlinear_migration_requires_an_explicit_choice_and_local_migration_does_not_write_bindings() {
    let mut s = tree();
    s.edit("nonlinear", None, |e| {
        let mut t = e
            .candidate_document()
            .get_transform(&id(91))
            .unwrap()
            .clone();
        t.data = TransformData::Warp(WarpTransform {
            rows: 1,
            columns: 1,
            quad: false,
            bezier: None,
            points: vec![
                Vec2::new(0., 0.),
                Vec2::new(10., 0.),
                Vec2::new(0., 10.),
                Vec2::new(10., 10.),
            ],
        });
        e.replace_transform(t)
    })
    .unwrap();
    let mut request = roots(vec![DeletionTarget::Part(id(4))]);
    let before = s.document().clone();
    let report = s.analyze_deletion(&request).unwrap();
    assert!(!report.export_analysis_complete);
    assert!(report
        .native
        .decisions
        .iter()
        .any(|d| d.object.id == id(12) && d.choices.contains(&DeletionChoice::KeepLocalData)));
    assert_eq!(
        s.plan_deletion(&request, &DeletionOptions::default())
            .unwrap_err()
            .code
            .as_ref(),
        "DELETION_DECISION_REQUIRED"
    );
    request
        .reparent
        .insert(id(12), ReparentStrategy::KeepLocalData);
    let sample = s.document().get_binding(&id(22)).unwrap().axes[0].samples[0]
        .id
        .clone();
    lock(&mut s, &sample);
    let plan = s
        .plan_deletion(&request, &DeletionOptions::default())
        .unwrap();
    s.execute_deletion_plan(&plan).unwrap();
    assert_eq!(
        s.document().get_mesh(&id(12)).unwrap().base_positions,
        before.get_mesh(&id(12)).unwrap().base_positions
    );
    assert_eq!(
        s.document().get_binding(&id(22)),
        before.get_binding(&id(22))
    );
}

#[test]
fn readonly_locked_deformation_destination_does_not_block_preservation() {
    let mut s = tree();
    s.edit("ancestor", None, |e| {
        e.create_transform(Transform {
            id: id(92),
            runtime_id: "Kept".into(),
            data: TransformData::Rotation(RotationTransform::default()),
            ..Default::default()
        })?;
        let mut t = e
            .candidate_document()
            .get_transform(&id(91))
            .unwrap()
            .clone();
        t.parent_id = Some(id(92).into());
        e.replace_transform(t)
    })
    .unwrap();
    lock(&mut s, &id(92));
    let report = s
        .analyze_deletion(&roots(vec![DeletionTarget::Part(id(4))]))
        .unwrap();
    assert!(
        report.native.decisions.is_empty(),
        "{:?}",
        report.native.decisions
    );
    let plan = s
        .plan_deletion(
            &roots(vec![DeletionTarget::Part(id(4))]),
            &DeletionOptions::default(),
        )
        .unwrap();
    s.execute_deletion_plan(&plan).unwrap();
    assert_eq!(s.document().get_mesh(&id(12)).unwrap().deformer_id, id(92));
    assert!(s.document().get_transform(&id(92)).is_some());
}

#[test]
fn pose_track_and_key_inherited_locks_block_the_entire_deletion() {
    for target in [112, 111, 115, 101, 102, 100] {
        let mut s = tree();
        animation(&mut s);
        lock(&mut s, &id(target));
        let before = s.document().clone();
        let report = s
            .analyze_deletion(&roots(vec![DeletionTarget::Part(id(4))]))
            .unwrap();
        assert!(report
            .native
            .blockers
            .iter()
            .any(|b| b.source_id == id(target)));
        assert!(s
            .plan_deletion(
                &roots(vec![DeletionTarget::Part(id(4))]),
                &DeletionOptions::default()
            )
            .is_err());
        assert!(s.document().same_content(&before));
    }
}

#[test]
fn dissolve_part_requires_a_legal_offscreen_owner_and_explicit_state_loss_choice() {
    let mut s = tree();
    let mut request = roots(vec![DeletionTarget::DissolvePart(id(4))]);
    let report = s.analyze_deletion(&request).unwrap();
    assert!(report
        .native
        .decisions
        .iter()
        .any(|d| d.reason == DeletionDecisionReason::OffscreenOwnerUnavailable));
    request
        .reparent
        .insert(id(4), ReparentStrategy::KeepLocalData);
    request.offscreen_placements.insert(
        id(40),
        DeletionOffscreenPlacement {
            part_id: id(90),
            mapping: None,
        },
    );
    let plan = s
        .plan_deletion(&request, &DeletionOptions::default())
        .unwrap();
    s.execute_deletion_plan(&plan).unwrap();
    assert_eq!(s.document().get_offscreen(&id(40)).unwrap().part_id, id(90));
    assert!(s
        .document()
        .get_transform(&id(91))
        .unwrap()
        .part_id
        .is_none());
    assert_eq!(s.document().get_mesh(&id(12)).unwrap().deformer_id, id(91));
    assert!(s.document().get_part(&id(4)).is_none());
}

#[test]
fn opaque_pose_and_unrelated_motion_namespace_regressions_are_reported_without_rebasing() {
    let mut s = tree();
    let part_runtime = s.document().get_part(&id(4)).unwrap().runtime_id.clone();
    let param_runtime = s
        .document()
        .get_parameter(&id(3))
        .unwrap()
        .runtime_id
        .clone();
    s.edit("opaque imports", None, |e| {
        e.import_pose3(&id(130), &serde_json::json!({"Type":"Live2D Pose","Groups":[[{"Id":part_runtime}]],"Vendor":true}).to_string())?;
        e.import_motion3(&id(131), "Unrelated", &serde_json::json!({"Version":3,"Meta":{"Duration":1.,"Fps":30.,"Loop":false,"AreBeziersRestricted":true,"CurveCount":1,"TotalSegmentCount":0,"TotalPointCount":1,"UserDataCount":0,"TotalUserDataSize":0},"Curves":[{"Target":"Parameter","Id":param_runtime,"Segments":[0.,0.]}],"Vendor":true}).to_string())?; Ok(())
    }).unwrap();
    let request = roots(vec![DeletionTarget::Part(id(4))]);
    let report = s.analyze_deletion(&request).unwrap();
    assert!(report
        .new_export_failures
        .contains(&DeletionExportFailure::Pose {
            id: id(130),
            problem: OpaqueExportProblem::NamespaceChanged
        }));
    assert!(report
        .new_export_failures
        .contains(&DeletionExportFailure::Pose {
            id: id(130),
            problem: OpaqueExportProblem::ContentChanged
        }));
    assert!(report
        .new_export_failures
        .contains(&DeletionExportFailure::Motion {
            id: id(131),
            problem: OpaqueExportProblem::NamespaceChanged
        }));
    let provenance = s.document().pose().unwrap().opaque_source_ids.clone();
    assert_eq!(
        s.plan_deletion(&request, &DeletionOptions::default())
            .unwrap_err()
            .code
            .as_ref(),
        "DELETION_EXPORT_DECISION_REQUIRED"
    );
    let plan = s
        .plan_deletion(
            &request,
            &DeletionOptions {
                accepted_export_failures: report.new_export_failures,
            },
        )
        .unwrap();
    s.execute_deletion_plan(&plan).unwrap();
    assert_eq!(s.document().pose().unwrap().opaque_source_ids, provenance);
    assert_eq!(
        zipzi_project::export_pose3(s.document()).unwrap_err().code,
        "OPAQUE_NAMESPACE_CHANGED"
    );
}

#[test]
fn deleted_drawing_group_flattens_external_members_in_source_order() {
    use zipzi_core::draw_order::DrawOrderGroup;
    let mut s = tree();
    s.edit("independent drawing", None, |e| {
        e.replace_draw_order_groups(vec![
            DrawOrderGroup {
                owner: String::new(),
                items: vec![id(10), id(4), id(11)],
                min_order: -10,
                max_order: 10,
            },
            DrawOrderGroup {
                owner: id(4),
                items: vec![id(12)],
                min_order: -20,
                max_order: 20,
            },
        ])
    })
    .unwrap();
    let plan = s
        .plan_deletion(
            &roots(vec![DeletionTarget::Part(id(4))]),
            &DeletionOptions::default(),
        )
        .unwrap();
    assert!(plan.report().native.effects.iter().any(|e| matches!(e, DeletionEffect::DrawingGroupFlattened { group, dynamic_order: true, .. } if group.id == id(4))));
    assert!(plan
        .report()
        .native
        .removed
        .iter()
        .any(|r| r.object.id == id(4) && r.object.kind == DeletionKind::DrawingGroup));
    s.execute_deletion_plan(&plan).unwrap();
    let groups = s.document().draw_order_groups().unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].items, vec![id(10), id(12), id(11)]);
    assert!(s.document().get_mesh(&id(12)).is_some());
}

#[test]
fn conversion_honors_locked_samples_that_its_final_write_set_would_change() {
    let mut s = tree();
    let sample = s.document().get_binding(&id(22)).unwrap().axes[0].samples[0]
        .id
        .clone();
    lock(&mut s, &sample);
    let before = s.document().clone();
    let analysis = s
        .analyze_deletion(&roots(vec![DeletionTarget::Part(id(4))]))
        .unwrap();
    assert!(analysis
        .native
        .blockers
        .iter()
        .any(|b| b.source_id == sample));
    assert!(!analysis.export_analysis_complete);
    assert!(s.document().same_content(&before));
}

#[test]
fn numeric_conversion_failure_is_a_typed_choice_and_never_a_silent_local_fallback() {
    let s = tree();
    let before = s.document().clone();
    let mut request = roots(vec![DeletionTarget::Part(id(4))]);
    request.tolerance.absolute = 1e-12;
    request.tolerance.relative = 0.;
    let analysis = s.analyze_deletion(&request).unwrap();
    assert!(analysis.native.decisions.iter().any(|d| matches!(&d.reason, DeletionDecisionReason::ConversionUnproven { code } if code == "REPARENT_TOLERANCE_EXCEEDED")));
    assert!(!analysis.export_analysis_complete);
    assert!(s.document().same_content(&before));
}

#[test]
fn removing_only_deformer_preserves_its_children_and_part_scope() {
    let mut s = tree();
    let plan = s
        .plan_deletion(
            &roots(vec![DeletionTarget::RemoveDeformer(id(91))]),
            &DeletionOptions::default(),
        )
        .unwrap();
    s.execute_deletion_plan(&plan).unwrap();
    assert!(s.document().get_transform(&id(91)).is_none());
    assert!(s.document().get_mesh(&id(12)).is_some());
    assert!(s.document().get_part(&id(4)).is_some());
    assert!(s.document().get_offscreen(&id(40)).is_some());
}
