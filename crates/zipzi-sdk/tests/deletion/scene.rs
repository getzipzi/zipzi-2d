use super::*;
use zipzi_project::OpaqueExportProblem;

pub(super) fn tree() -> AuthoringSession {
    let mut s = fixture();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_part(Part {
                    id: id(90),
                    runtime_id: "Outside".into(),
                    ..Default::default()
                })?;
                crate::support::insert_transform(
                    e,
                    Transform {
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
                    },
                )?;
                {
                    let mut mesh = e.read_document()?.mesh(&id(12)).unwrap().clone();
                    mesh.part_id = (&id(90)).to_owned();
                    e.replace_mesh(mesh)
                }?;
                e.edit_scene(
                    SceneEdit::Reparent {
                        edit: ReparentEdit::keep_local(
                            vec![(&id(12)).to_owned()],
                            (&id(91)).to_owned(),
                        ),
                    },
                    Default::default(),
                    &Default::default(),
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("cross-part tree")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
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
    crate::support::frame(s.snapshot(), &PreviewValues::default())
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
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
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
                e.replace_pose(Some(pose))?;
                let mut info = e.read_document().unwrap().display_info().clone();
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
            .and_then(|value| {
                let prepared = draft.prepare("cross-domain references")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}

#[test]
fn part_deletion_cleans_pose_motion_expression_and_cdi_while_preserving_external_mesh_pose() {
    let mut s = tree();
    animation(&mut s);
    let before = s.snapshot().document().clone();
    let pose_before = positions(&s);
    let plan = prepare_deletion(&s, &roots(vec![DeletionTarget::Part(id(4))])).unwrap();
    assert!(plan
        .0
        .report
        .migrations
        .iter()
        .any(|m| m.object.id == id(12) && m.guarantee.is_some()));
    s.publish(plan.1).unwrap();
    assert!(s.snapshot().document().part(&id(4)).is_none());
    assert!(s.snapshot().document().transform(&id(91)).is_none());
    assert!(s.snapshot().document().part(&id(90)).is_some());
    assert!(s.snapshot().document().mesh(&id(12)).is_some());
    assert!(s
        .snapshot()
        .document()
        .mesh(&id(12))
        .unwrap()
        .deformer_id
        .is_empty());
    for (a, b) in positions(&s).iter().zip(&pose_before) {
        assert!((a.x - b.x).abs() < 1e-4 && (a.y - b.y).abs() < 1e-4);
    }
    let motion = s.snapshot().document().motion(&id(100)).unwrap();
    assert_eq!(motion.tracks.len(), 1);
    assert_eq!(motion.tracks[0].id, id(103));
    assert_eq!(motion.track_order, vec![id(103)]);
    assert_eq!(motion.events, before.motion(&id(100)).unwrap().events);
    let group = &s.snapshot().document().pose().as_ref().unwrap().groups[0];
    assert_eq!(group.entries.len(), 1);
    assert_eq!(group.entries[0].id, id(114));
    assert!(group.entries[0].links.as_ref().unwrap().is_empty());
    assert_eq!(
        s.snapshot()
            .document()
            .expression(&id(120))
            .unwrap()
            .source_scene()
            .unwrap()
            .tracks
            .len(),
        1
    );
    assert!(
        matches!(&s.snapshot().document().display_info().parts.as_ref().unwrap()[0], CdiPartEntry::Unresolved { runtime_id, .. } if runtime_id == "Legacy")
    );
    assert!(s.snapshot().document().motion_track(&id(101)).is_none());
    assert!(s.snapshot().document().pose_entry(&id(112)).is_none());
    let reopened = crate::support::roundtrip(s.snapshot());
    let reopened = reopened.document();
    assert!(reopened.builder().freeze().is_ok());
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
    s.redo(s.version()).unwrap();
    assert!(s.snapshot().document().part(&id(4)).is_none());
}

#[test]
fn explicit_deformer_scope_is_unioned_before_parent_roots_are_deduplicated() {
    let s = tree();
    let a = analyze(
        &s,
        &roots(vec![
            DeletionTarget::Part(id(4)),
            DeletionTarget::Deformer(id(91)),
        ]),
    )
    .unwrap();
    let b = analyze(
        &s,
        &roots(vec![
            DeletionTarget::Deformer(id(91)),
            DeletionTarget::Part(id(4)),
        ]),
    )
    .unwrap();
    assert_eq!(a.0, b.0);
    assert!(a.0.removed.iter().any(|r| r.object.id == id(12)));
    assert!(!a.0.removed.iter().any(|r| r.object.id == id(90)));
    assert!(a.0.migrations.is_empty());
}

#[test]
fn nonlinear_migration_requires_an_explicit_choice_and_local_migration_does_not_write_bindings() {
    let mut s = tree();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut t = e
                    .read_document()
                    .unwrap()
                    .transform(&id(91))
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
                        Vec2::new(12., 9.),
                    ],
                });
                e.replace_transform(t)
            })
            .and_then(|value| {
                let prepared = draft.prepare("nonlinear")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let mut request = roots(vec![DeletionTarget::Part(id(4))]);
    let before = s.snapshot().document().clone();
    let report = analyze(&s, &request).unwrap();
    assert!(
        !report.0.blockers.is_empty()
            || !report.0.structural_blockers.is_empty()
            || !report.0.decisions.is_empty()
    );
    assert!(report
        .0
        .decisions
        .iter()
        .any(|d| d.object.id == id(12) && d.choices.contains(&DeletionChoice::KeepLocalData)));
    assert_eq!(
        prepare_deletion(&s, &request).unwrap_err().code.as_str(),
        "DELETION_DECISION_REQUIRED"
    );
    request
        .reparent
        .insert(id(12), ReparentStrategy::KeepLocalData);
    let sample = s.snapshot().document().binding(&id(22)).unwrap().axes[0].samples[0]
        .id
        .clone();
    lock(&mut s, &sample);
    let plan = prepare_deletion(&s, &request).unwrap();
    s.publish(plan.1).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .mesh(&id(12))
            .unwrap()
            .base_positions,
        before.mesh(&id(12)).unwrap().base_positions
    );
    assert_eq!(
        s.snapshot().document().binding(&id(22)),
        before.binding(&id(22))
    );
}

#[test]
fn readonly_locked_deformation_destination_does_not_block_preservation() {
    let mut s = tree();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(
                    e,
                    Transform {
                        id: id(92),
                        runtime_id: "Kept".into(),
                        data: TransformData::Rotation(RotationTransform::default()),
                        ..Default::default()
                    },
                )?;
                let mut t = e
                    .read_document()
                    .unwrap()
                    .transform(&id(91))
                    .unwrap()
                    .clone();
                t.parent_id = Some(id(92).into());
                e.replace_transform(t)
            })
            .and_then(|value| {
                let prepared = draft.prepare("ancestor")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    lock(&mut s, &id(92));
    let report = analyze(&s, &roots(vec![DeletionTarget::Part(id(4))])).unwrap();
    assert!(report.0.decisions.is_empty(), "{:?}", report.0.decisions);
    let plan = prepare_deletion(&s, &roots(vec![DeletionTarget::Part(id(4))])).unwrap();
    s.publish(plan.1).unwrap();
    assert_eq!(
        s.snapshot().document().mesh(&id(12)).unwrap().deformer_id,
        id(92)
    );
    assert!(s.snapshot().document().transform(&id(92)).is_some());
}

#[test]
fn pose_track_and_key_inherited_locks_block_the_entire_deletion() {
    for target in [112, 111, 115, 101, 102, 100] {
        let mut s = tree();
        animation(&mut s);
        lock(&mut s, &id(target));
        let before = s.snapshot().document().clone();
        let report = analyze(&s, &roots(vec![DeletionTarget::Part(id(4))])).unwrap();
        assert!(report.0.blockers.iter().any(|b| b.source_id == id(target)));
        assert!(prepare_deletion(&s, &roots(vec![DeletionTarget::Part(id(4))])).is_err());
        assert_eq!(s.snapshot().content_id(), before.content_id());
    }
}

#[test]
fn dissolve_part_requires_a_legal_offscreen_owner_and_explicit_state_loss_choice() {
    let mut s = tree();
    let mut request = roots(vec![DeletionTarget::DissolvePart(id(4))]);
    let report = analyze(&s, &request).unwrap();
    assert!(report
        .0
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
    let plan = prepare_deletion(&s, &request).unwrap();
    s.publish(plan.1).unwrap();
    assert_eq!(
        s.snapshot().document().offscreen(&id(40)).unwrap().part_id,
        id(90)
    );
    assert!(s
        .snapshot()
        .document()
        .transform(&id(91))
        .unwrap()
        .part_id
        .is_none());
    assert_eq!(
        s.snapshot().document().mesh(&id(12)).unwrap().deformer_id,
        id(91)
    );
    assert!(s.snapshot().document().part(&id(4)).is_none());
}

#[test]
fn offscreen_can_migrate_into_an_owner_freed_by_the_same_deletion() {
    let mut s = tree();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_offscreen(
                    e,
                    Offscreen {
                        id: id(92),
                        part_id: id(90),
                        ..Default::default()
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("occupied destination")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.snapshot().document().clone();
    let mut request = roots(vec![
        DeletionTarget::DissolvePart(id(4)),
        DeletionTarget::Offscreen(id(92)),
    ]);
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
    let analysis = analyze(&s, &request).unwrap();
    assert!(analysis.0.decisions.is_empty(), "{:?}", analysis.0);
    let plan = prepare_deletion(&s, &request).unwrap();
    s.publish(plan.1).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .offscreen_for_part(&id(90))
            .unwrap()
            .id,
        id(40)
    );
    assert!(s.snapshot().document().offscreen(&id(92)).is_none());
    assert!(s.snapshot().document().builder().freeze().is_ok());
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
}

#[test]
fn opaque_pose_and_unrelated_motion_namespace_regressions_are_reported_without_rebasing() {
    let mut s = tree();
    let part_runtime = s
        .snapshot()
        .document()
        .part(&id(4))
        .unwrap()
        .runtime_id
        .clone();
    let param_runtime = s
        .snapshot()
        .document()
        .parameter(&id(3))
        .unwrap()
        .runtime_id
        .clone();
    {
        let mut draft = s.begin_draft();
        draft.batch(draft.source(), |e| {
        {let imported=zipzi_project::import_pose3(&e.read_document()?, &id(130), &serde_json::json!({"Type":"Live2D Pose","Groups":[[{"Id":part_runtime}]],"Vendor":true}).to_string()).map_err(|error|Status::error(error.code.clone(),format!("{:?}",error)))?;e.replace_pose(Some(imported.candidate.pose().as_ref().unwrap().as_ref().clone()))}?;
        {let imported=zipzi_project::import_motion3(&e.read_document()?, &id(131), "Unrelated", &serde_json::json!({"Version":3,"Meta":{"Duration":1.,"Fps":30.,"Loop":false,"AreBeziersRestricted":true,"CurveCount":1,"TotalSegmentCount":0,"TotalPointCount":1,"UserDataCount":0,"TotalUserDataSize":0},"Curves":[{"Target":"Parameter","Id":param_runtime,"Segments":[0.,0.]}],"Vendor":true}).to_string()).map_err(|error|Status::error(error.code.clone(),format!("{:?}",error)))?;e.create_motion(imported.candidate.motion(&id(131)).unwrap().clone())}?; Ok(())
    }).and_then(|value| {
            let prepared = draft.prepare("opaque imports")?;
            s.publish(prepared).map(|receipt| (value, receipt))
        })
    }.unwrap();
    let request = roots(vec![DeletionTarget::Part(id(4))]);
    let report = analyze(&s, &request).unwrap();
    assert!(report.1.contains(&zipzi_project::OpaqueExportIssue::Pose {
        id: id(130),
        problem: OpaqueExportProblem::NamespaceChanged
    }));
    assert!(report.1.contains(&zipzi_project::OpaqueExportIssue::Pose {
        id: id(130),
        problem: OpaqueExportProblem::ContentChanged
    }));
    assert!(report
        .1
        .contains(&zipzi_project::OpaqueExportIssue::Motion {
            id: id(131),
            problem: OpaqueExportProblem::NamespaceChanged
        }));
    let provenance = s
        .snapshot()
        .document()
        .pose()
        .as_ref()
        .unwrap()
        .opaque_source_ids
        .clone();
    assert!(!report.1.is_empty());
    let plan = prepare_deletion(&s, &request).unwrap();
    s.publish(plan.1).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .pose()
            .as_ref()
            .unwrap()
            .opaque_source_ids,
        provenance
    );
    assert_eq!(
        zipzi_project::export_pose3(s.snapshot().document())
            .unwrap_err()
            .code,
        "OPAQUE_NAMESPACE_CHANGED"
    );
}

#[test]
fn deleted_drawing_group_flattens_external_members_in_source_order() {
    use zipzi_core::draw_order::DrawOrderGroup;
    let mut s = tree();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::drawing_groups(
                    e,
                    vec![
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
                    ],
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("independent drawing")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let plan = prepare_deletion(&s, &roots(vec![DeletionTarget::Part(id(4))])).unwrap();
    assert!(plan.0.report.effects.iter().any(|e| matches!(e, DeletionEffect::DrawingGroupFlattened { group, dynamic_order: true, .. } if group.id == id(4))));
    assert!(plan
        .0
        .report
        .removed
        .iter()
        .any(|r| r.object.id == id(4) && r.object.kind == DeletionKind::DrawingGroup));
    s.publish(plan.1).unwrap();
    let groups = s
        .snapshot()
        .document()
        .draw_order_groups()
        .as_ref()
        .unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].items, vec![id(10), id(12), id(11)]);
    assert!(s.snapshot().document().mesh(&id(12)).is_some());
}

#[test]
fn conversion_honors_locked_samples_that_its_final_write_set_would_change() {
    let mut s = tree();
    let sample = s.snapshot().document().binding(&id(22)).unwrap().axes[0].samples[0]
        .id
        .clone();
    lock(&mut s, &sample);
    let before = s.snapshot().document().clone();
    let analysis = analyze(&s, &roots(vec![DeletionTarget::Part(id(4))])).unwrap();
    assert!(analysis.0.blockers.iter().any(|b| b.source_id == sample));
    assert!(
        !analysis.0.blockers.is_empty()
            || !analysis.0.structural_blockers.is_empty()
            || !analysis.0.decisions.is_empty()
    );
    assert_eq!(s.snapshot().content_id(), before.content_id());
}

#[test]
fn numeric_conversion_failure_is_a_typed_choice_and_never_a_silent_local_fallback() {
    let s = tree();
    let before = s.snapshot().document().clone();
    let mut request = roots(vec![DeletionTarget::Part(id(4))]);
    request.tolerance.absolute = 1e-12;
    request.tolerance.relative = 0.;
    let analysis = analyze(&s, &request).unwrap();
    assert!(analysis.0.decisions.iter().any(|d| matches!(&d.reason, DeletionDecisionReason::ConversionUnproven { code } if code == "REPARENT_TOLERANCE_EXCEEDED")));
    assert!(
        !analysis.0.blockers.is_empty()
            || !analysis.0.structural_blockers.is_empty()
            || !analysis.0.decisions.is_empty()
    );
    assert_eq!(s.snapshot().content_id(), before.content_id());
}

#[test]
fn removing_only_deformer_preserves_its_children_and_part_scope() {
    let mut s = tree();
    let plan = prepare_deletion(&s, &roots(vec![DeletionTarget::RemoveDeformer(id(91))])).unwrap();
    s.publish(plan.1).unwrap();
    assert!(s.snapshot().document().transform(&id(91)).is_none());
    assert!(s.snapshot().document().mesh(&id(12)).is_some());
    assert!(s.snapshot().document().part(&id(4)).is_some());
    assert!(s.snapshot().document().offscreen(&id(40)).is_some());
}
