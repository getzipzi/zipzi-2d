use super::*;
use zipzi_core::document::PhysicsAssetImport;
use zipzi_core::BindingAxisEditing;
fn roots(roots: Vec<DeletionTarget>) -> DeletionRequest {
    DeletionRequest {
        roots,
        ..Default::default()
    }
}
fn record(kind: DeletionKind, n: u32) -> DeletionTarget {
    DeletionTarget::Record(DeletionObject {
        kind,
        id: id(n),
        owner_id: None,
    })
}
fn apply(s: &mut AuthoringSession, request: &DeletionRequest) {
    let analysis = analyze(s, request).unwrap();
    assert!(analysis.0.decisions.is_empty(), "{:?}", analysis.0);
    assert!(
        analysis.0.structural_blockers.is_empty(),
        "{:?}",
        analysis.0
    );
    let (_, p) = prepare_deletion(s, request).unwrap();
    s.publish(p).unwrap();
}
#[test]
fn parameter_default_reduces_all_ordinary_grids_and_cleans_animation_in_one_undo() {
    let mut s = scene::tree();
    scene::animation(&mut s);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut p = e
                    .read_document()
                    .unwrap()
                    .parameter(&id(3))
                    .unwrap()
                    .clone();
                p.default_value = 0.4;
                e.replace_parameter(p)?;
                let mut b = e.read_document().unwrap().binding(&id(20)).unwrap().clone();
                for v in &mut b.keyforms[1].positions {
                    v.x += 10.;
                }
                e.replace_binding(b)
            })
            .and_then(|value| {
                let prepared = draft.prepare("default")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.snapshot().document().clone();
    let evaluated = crate::support::frame(s.snapshot(), &PreviewValues::default()).unwrap();
    apply(&mut s, &roots(vec![DeletionTarget::Parameter(id(3))]));
    assert!(s.snapshot().document().parameter(&id(3)).is_none());
    for n in 20..=22 {
        let b = s.snapshot().document().binding(&id(n)).unwrap();
        assert!(b.axes.is_empty());
        assert!(b.keyforms.is_empty());
    }
    assert!(s
        .snapshot()
        .document()
        .glue(&id(30))
        .unwrap()
        .binding
        .as_ref()
        .unwrap()
        .axes
        .is_empty());
    let clip = s.snapshot().document().motion(&id(100)).unwrap();
    assert_eq!(clip.tracks.len(), 1);
    assert_eq!(clip.tracks[0].id, id(101));
    assert_eq!(clip.events.len(), 1);
    let after = crate::support::frame(s.snapshot(), &PreviewValues::default()).unwrap();
    for drawable in &evaluated.drawables {
        let result = after
            .drawables
            .iter()
            .find(|v| v.id == drawable.id)
            .unwrap();
        assert_eq!(drawable.positions, result.positions);
    }
    assert!(s.snapshot().document().builder().freeze().is_ok());
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
    s.redo(s.version()).unwrap();
    assert!(s.snapshot().document().parameter(&id(3)).is_none());
}
#[test]
fn texture_requires_explicit_resolution_and_attachment_cleanup_keeps_users() {
    let mut s = fixture();
    let mut request = roots(vec![DeletionTarget::Texture(id(2))]);
    let review = analyze(&s, &request).unwrap();
    assert!(review
        .0
        .decisions
        .iter()
        .any(|d| d.reason == DeletionDecisionReason::TextureReplacementRequired));
    assert!(prepare_deletion(&s, &request).is_err());
    request.texture_replacements.insert(id(2), None);
    apply(&mut s, &request);
    for n in 10..=12 {
        assert_eq!(
            s.snapshot()
                .document()
                .mesh(&id(n))
                .unwrap()
                .texture_asset_id,
            SceneReference::Unspecified
        );
    }
    assert!(s.snapshot().document().asset(&id(2)).is_none());
    s.undo(s.version()).unwrap();
    assert!(s.snapshot().document().asset(&id(2)).is_some());
}
#[test]
fn deleting_motion_removes_registrations_but_keeps_other_business_assets() {
    let mut s = scene::tree();
    scene::animation(&mut s);
    let before = s.snapshot().document().clone();
    apply(&mut s, &roots(vec![record(DeletionKind::Motion, 100)]));
    assert!(s.snapshot().document().motion(&id(100)).is_none());
    assert!(s.snapshot().document().pose().is_some());
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
}
#[test]
fn drawing_and_glue_roles_do_not_delete_their_host() {
    let mut s = fixture();
    apply(&mut s, &roots(vec![record(DeletionKind::GlueBinding, 30)]));
    assert!(s
        .snapshot()
        .document()
        .glue(&id(30))
        .unwrap()
        .binding
        .is_none());
    assert!(s.snapshot().document().binding(&id(20)).is_some());
    s.undo(s.version()).unwrap();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::drawing_groups(
                    e,
                    vec![
                        zipzi_core::draw_order::DrawOrderGroup {
                            owner: String::new(),
                            items: vec![id(10), id(4), id(11)],
                            min_order: -10,
                            max_order: 10,
                        },
                        zipzi_core::draw_order::DrawOrderGroup {
                            owner: id(4),
                            items: vec![id(12)],
                            min_order: -20,
                            max_order: 20,
                        },
                    ],
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("drawing role")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let mut request = roots(vec![
        record(DeletionKind::DrawingGroup, 4),
        DeletionTarget::Offscreen(id(40)),
    ]);
    request
        .reparent
        .insert(id(4), ReparentStrategy::KeepLocalData);
    apply(&mut s, &request);
    assert!(s.snapshot().document().part(&id(4)).is_some());
    assert!(s.snapshot().document().scene_binding(&id(5)).is_some());
}

fn drawing_role_request(s: &mut AuthoringSession) -> DeletionRequest {
    apply(s, &roots(vec![DeletionTarget::Offscreen(id(40))]));
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::drawing_groups(
                    e,
                    vec![
                        zipzi_core::draw_order::DrawOrderGroup {
                            owner: String::new(),
                            items: vec![id(10), id(4), id(11)],
                            min_order: -10,
                            max_order: 10,
                        },
                        zipzi_core::draw_order::DrawOrderGroup {
                            owner: id(4),
                            items: vec![id(12)],
                            min_order: -20,
                            max_order: 20,
                        },
                    ],
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("drawing role without an Offscreen")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    roots(vec![record(DeletionKind::DrawingGroup, 4)])
}

#[test]
fn drawing_role_flattening_requires_review_of_the_retained_members() {
    let mut s = fixture();
    let request = drawing_role_request(&mut s);
    let (review, prepared) = crate::support::prepared(&s, |c| {
        c.review_deletion(&request, Default::default(), &Default::default())
    })
    .unwrap();
    assert!(review.report.requires_confirmation());
    assert!(review
        .report
        .rewritten
        .iter()
        .any(|r| r.owner.kind == DeletionKind::DrawingGroup && r.owner.id == id(1)));
    s.publish(prepared).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .draw_order_groups()
            .as_ref()
            .unwrap()[0]
            .items,
        vec![id(10), id(12), id(11)]
    );
    assert!(s.snapshot().document().part(&id(4)).is_some());
}

#[test]
fn locked_drawing_destination_is_reported_without_publishing() {
    let mut s = fixture();
    let request = drawing_role_request(&mut s);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_part({
                    let mut value = Part {
                        id: id(90),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                let mut groups = e
                    .read_document()
                    .unwrap()
                    .draw_order_groups()
                    .as_ref()
                    .unwrap()
                    .to_vec();
                groups[0].owner = id(90);
                groups.insert(
                    0,
                    zipzi_core::draw_order::DrawOrderGroup {
                        owner: String::new(),
                        items: vec![id(90)],
                        min_order: -100,
                        max_order: 100,
                    },
                );
                crate::support::drawing_groups(e, groups)
            })
            .and_then(|value| {
                let prepared = draft.prepare("external drawing destination")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    lock(&mut s, &id(90));
    let before = s.snapshot().document().clone();
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    let analysis = analyze(&s, &request).unwrap();
    assert!(
        analysis.0.blockers.iter().any(|b| b.source_id == id(90)),
        "{:?}",
        analysis.0
    );
    let (review, prepared) = crate::support::prepared(&s, |c| {
        c.review_deletion(&request, Default::default(), &Default::default())
    })
    .unwrap();
    assert!(review
        .report
        .blockers
        .iter()
        .any(|b| b.source_id == id(90) && b.object.kind == DeletionKind::DrawingGroup));
    assert!(prepared.writes().is_empty());
    assert!(prepare_deletion(&s, &request).is_err());
    assert_eq!(s.snapshot().content_id(), before.content_id());
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
}
#[test]
fn samples_and_keys_use_native_interpolation_and_empty_tracks_are_reported() {
    let mut s = scene::tree();
    scene::animation(&mut s);
    let sample = s.snapshot().document().binding(&id(20)).unwrap().axes[0].samples[1]
        .id
        .clone();
    let request = roots(vec![
        DeletionTarget::Record(DeletionObject {
            kind: DeletionKind::Sample,
            id: sample.clone(),
            owner_id: None,
        }),
        record(DeletionKind::MotionKey, 106),
    ]);
    apply(&mut s, &request);
    assert!(!s
        .snapshot()
        .document()
        .contains_object(ObjectKind::BindingSample, &sample));
    assert_eq!(
        s.snapshot().document().motion(&id(100)).unwrap().tracks[0]
            .keys()
            .count(),
        1
    );
    s.undo(s.version()).unwrap();
    let request = roots(vec![
        record(DeletionKind::MotionKey, 102),
        record(DeletionKind::MotionKey, 106),
    ]);
    let report = analyze(&s, &request).unwrap();
    assert!(report
        .0
        .removed
        .iter()
        .any(|r| r.object.kind == DeletionKind::MotionTrack && r.object.id == id(101)));
    apply(&mut s, &request);
    assert!(s
        .snapshot()
        .document()
        .motion(&id(100))
        .unwrap()
        .tracks
        .iter()
        .all(|t| t.id != id(101)));
}

fn physics(s: &mut AuthoringSession) {
    let wire = serde_json::json!({"Version":3,"Meta":{"PhysicsSettingCount":1,"TotalInputCount":1,"TotalOutputCount":1,"VertexCount":3,"Fps":30,"EffectiveForces":{"Gravity":{"X":0,"Y":-1},"Wind":{"X":0,"Y":0}},"PhysicsDictionary":[{"Id":"Rig","Name":"Rig"}]},"PhysicsSettings":[{"Id":"Rig","Input":[{"Source":{"Target":"Parameter","Id":"P"},"Weight":100,"Type":"Angle","Reflect":false}],"Output":[{"Destination":{"Target":"Parameter","Id":"P"},"VertexIndex":1,"Scale":1,"Weight":100,"Type":"Angle","Reflect":false}],"Vertices":[{"Position":{"X":0,"Y":0},"Radius":0,"Mobility":0.95,"Delay":0.8,"Acceleration":1},{"Position":{"X":0,"Y":1},"Radius":1,"Mobility":0.95,"Delay":0.8,"Acceleration":1},{"Position":{"X":0,"Y":2},"Radius":1,"Mobility":0.95,"Delay":0.8,"Acceleration":1}],"Normalization":{"Position":{"Minimum":-1,"Default":0,"Maximum":1},"Angle":{"Minimum":-1,"Default":0,"Maximum":1}}}]});
    let asset = PhysicsAsset::from_wire(id(200), serde_json::from_value(wire).unwrap(), |_| {
        Some(id(3))
    })
    .unwrap();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_physics(Some(asset)))
            .and_then(|value| {
                let prepared = draft.prepare("physics")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
}
#[test]
fn parameter_physics_cleanup_and_invalid_chain_choices_honor_channel_locks() {
    let mut s = fixture();
    physics(&mut s);
    let group = s
        .snapshot()
        .document()
        .physics()
        .as_ref()
        .unwrap()
        .data
        .settings[0]
        .id
        .clone();
    let input = s
        .snapshot()
        .document()
        .physics()
        .as_ref()
        .unwrap()
        .data
        .settings[0]
        .inputs[0]
        .id
        .clone();
    let mut request = roots(vec![DeletionTarget::Parameter(id(3))]);
    let before = s.snapshot().document().clone();
    let report = analyze(&s, &request).unwrap();
    assert!(report.0.decisions.is_empty());
    let mut invalid = request.clone();
    invalid.roots.extend(
        s.snapshot()
            .document()
            .physics()
            .as_ref()
            .unwrap()
            .data
            .settings[0]
            .vertices
            .iter()
            .map(|v| {
                DeletionTarget::Record(DeletionObject {
                    kind: DeletionKind::PhysicsNode,
                    id: v.id.clone(),
                    owner_id: None,
                })
            }),
    );
    assert!(analyze(&s, &invalid)
        .unwrap()
        .0
        .decisions
        .iter()
        .any(|d| d.object.id == group
            && matches!(d.reason, DeletionDecisionReason::InvalidContainer { .. })));
    assert_eq!(s.snapshot().content_id(), before.content_id());
    request.roots.push(DeletionTarget::Record(DeletionObject {
        kind: DeletionKind::PhysicsGroup,
        id: group,
        owner_id: None,
    }));
    lock(&mut s, &input);
    assert!(analyze(&s, &request)
        .unwrap()
        .0
        .blockers
        .iter()
        .any(|b| b.source_id == input));
    s.undo(s.version()).unwrap();
    apply(&mut s, &request);
    assert!(s
        .snapshot()
        .document()
        .physics()
        .as_ref()
        .unwrap()
        .data
        .settings
        .is_empty());
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
}
#[test]
fn physics_node_replacement_preserves_output_and_rejects_deleted_or_root_destinations() {
    let mut s = fixture();
    physics(&mut s);
    let group = &s
        .snapshot()
        .document()
        .physics()
        .as_ref()
        .unwrap()
        .data
        .settings[0];
    let removed = group.vertices[1].id.clone();
    let replacement = group.vertices[2].id.clone();
    let output = group.outputs[0].id.clone();
    let mut request = roots(vec![DeletionTarget::Record(DeletionObject {
        kind: DeletionKind::PhysicsNode,
        id: removed.clone(),
        owner_id: None,
    })]);
    request
        .physics_node_replacements
        .insert(removed.clone(), replacement.clone());
    apply(&mut s, &request);
    let group = &s
        .snapshot()
        .document()
        .physics()
        .as_ref()
        .unwrap()
        .data
        .settings[0];
    assert_eq!(group.vertices.len(), 2);
    assert_eq!(group.outputs[0].id, output);
    assert_eq!(group.outputs[0].node_id, replacement);
    s.undo(s.version()).unwrap();
    request
        .physics_node_replacements
        .insert(removed.clone(), removed);
    assert!(!analyze(&s, &request).unwrap().0.decisions.is_empty());
}
#[test]
fn parameter_folder_dissolves_by_default_and_explicit_contents_delete_uses_same_cleanup() {
    let mut s = fixture();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                for (n, parent) in [(210, None), (211, Some(id(210)))] {
                    e.edit_organization(OrganizationEdit::CreateGroup {
                        group: CdiParameterGroup {
                            id: id(n),
                            runtime_id: format!("G{n}"),
                            name: "folder".into(),
                            parent_id: parent,
                            extensions: Default::default(),
                        },
                        before: None,
                    })?;
                }
                e.edit_organization(OrganizationEdit::SetParameterGroup {
                    parameter_id: id(3),
                    group_id: Some(id(211)),
                })?;
                let mut info = e.read_document().unwrap().display_info().clone();
                info.parameters
                    .as_mut()
                    .unwrap()
                    .push(CdiParameterEntry::Unresolved {
                        runtime_id: "legacy".into(),
                        name: "keep".into(),
                        group_id: Some(id(210)),
                        extensions: Default::default(),
                    });
                e.replace_display_info(info)
            })
            .and_then(|value| {
                let prepared = draft.prepare("folders")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.snapshot().document().clone();
    apply(
        &mut s,
        &roots(vec![DeletionTarget::ParameterFolder(id(210))]),
    );
    assert!(s.snapshot().document().parameter(&id(3)).is_some());
    assert_eq!(
        s.snapshot()
            .document()
            .parameter_group(&id(211))
            .unwrap()
            .parent_id,
        None
    );
    assert!(s.snapshot().document().display_info().parameters.as_ref().unwrap().iter().any(|e|matches!(e,CdiParameterEntry::Unresolved {runtime_id,group_id:None,..} if runtime_id=="legacy")));
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
    apply(
        &mut s,
        &roots(vec![DeletionTarget::ParameterFolderContents(id(210))]),
    );
    assert!(s.snapshot().document().parameter(&id(3)).is_none());
    assert!(s.snapshot().document().parameter_group(&id(211)).is_none());
    assert!(s
        .snapshot()
        .document()
        .display_info()
        .parameters
        .as_ref()
        .unwrap()
        .iter()
        .any(
            |e| matches!(e,CdiParameterEntry::Unresolved {runtime_id,..} if runtime_id=="legacy")
        ));
}
#[test]
fn attachment_removal_clears_only_resolved_optional_references_and_retains_bytes_for_undo() {
    let mut s = scene::tree();
    scene::animation(&mut s);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                {
                    for attachment in vec![PackageAttachment {
                        id: id(220),
                        path: "audio.wav".into(),
                        bytes: Some(std::sync::Arc::new(vec![1, 2, 3])),
                    }] {
                        e.edit_attachment(AttachmentEdit::Create {
                            id: attachment.id.clone(),
                            path: attachment.path.clone(),
                        })?;
                        let expected = e
                            .read_document()?
                            .package_attachments()
                            .iter()
                            .find(|a| a.id == attachment.id)
                            .unwrap()
                            .clone();
                        e.edit_attachment(AttachmentEdit::Replace {
                            expected,
                            attachment,
                        })?;
                    }
                    Ok::<_, Status>(())
                }?;
                let mut settings = e.read_document().unwrap().model3_settings().clone();
                settings.user_data = Some(AttachmentRef::Resolved {
                    attachment_id: id(220),
                });
                e.replace_model3_settings(settings)
            })
            .and_then(|value| {
                let prepared = draft.prepare("attachment")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.snapshot().document().clone();
    apply(&mut s, &roots(vec![DeletionTarget::Attachment(id(220))]));
    assert!(s.snapshot().document().attachment(&id(220)).is_none());
    assert!(s
        .snapshot()
        .document()
        .model3_settings()
        .user_data
        .is_none());
    assert!(s.snapshot().document().motion(&id(100)).is_some());
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
}
#[test]
fn default_outside_grid_retains_native_disabled_behavior_and_explicit_coordinate_replans() {
    let mut s = fixture();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut p = e
                    .read_document()
                    .unwrap()
                    .parameter(&id(3))
                    .unwrap()
                    .clone();
                p.maximum = 2.;
                p.default_value = 1.5;
                e.replace_parameter(p)
            })
            .and_then(|value| {
                let prepared = draft.prepare("outside")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.snapshot().document().clone();
    let mut request = roots(vec![DeletionTarget::Parameter(id(3))]);
    let report = analyze(&s, &request).unwrap();
    assert!(report.0.effects.iter().any(|e|matches!(e,DeletionEffect::ParameterDimensionRemoved {disabled:true,value,..} if *value==1.5)));
    apply(&mut s, &request);
    for n in 10..=12 {
        assert!(!s.snapshot().document().mesh(&id(n)).unwrap().enabled);
    }
    assert!(!s.snapshot().document().part(&id(4)).unwrap().enabled);
    assert!(s
        .snapshot()
        .document()
        .glue(&id(30))
        .unwrap()
        .binding
        .as_ref()
        .unwrap()
        .axes
        .is_empty());
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
    request.parameter_slices.insert(id(3), 0.5);
    apply(&mut s, &request);
    assert!(s.snapshot().document().mesh(&id(10)).unwrap().enabled);
}
#[test]
fn parameter_cleanup_removes_expression_entries_metadata_and_shared_constraints() {
    let mut s = scene::tree();
    scene::animation(&mut s);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(230),
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
                            id: id(231),
                            parameter_id: id(230),
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
                            id: id(232),
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
                {
                    let binding = BlendShapeBinding {
                        id: id(233),
                        target_id: id(10),
                        target_kind: BlendShapeTargetKind::Mesh,
                        key_table_id: id(231),
                        constraint_ids: vec![id(232)],
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
                let mut expression = e
                    .read_document()
                    .unwrap()
                    .expression(&id(120))
                    .unwrap()
                    .clone();
                expression.entries.push(ExpressionEntry {
                    id: id(234),
                    target: ExpressionTarget::Resolved {
                        parameter_id: id(3),
                    },
                    value: 0.7,
                    blend: None,
                    extensions: Default::default(),
                });
                expression.entries.push(ExpressionEntry {
                    id: id(235),
                    target: ExpressionTarget::Unresolved {
                        runtime_id: "legacy".into(),
                    },
                    value: 0.3,
                    blend: None,
                    extensions: Default::default(),
                });
                e.replace_expression(expression)?;
                let mut settings = e.read_document().unwrap().model3_settings().clone();
                settings.groups = Some(vec![ModelParameterGroup {
                    id: id(236),
                    name: "params".into(),
                    parameters: vec![ReferenceEntry {
                        id: id(237),
                        target: ModelTargetRef::Resolved { object_id: id(3) },
                    }],
                    extensions: Default::default(),
                }]);
                e.replace_model3_settings(settings)
            })
            .and_then(|value| {
                let prepared = draft.prepare("parameter consumers")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.snapshot().document().clone();
    apply(&mut s, &roots(vec![DeletionTarget::Parameter(id(3))]));
    assert!(s.snapshot().document().blend_constraint(&id(232)).is_none());
    assert!(s
        .snapshot()
        .document()
        .blend_binding(&id(233))
        .unwrap()
        .constraint_ids
        .is_empty());
    let exp = s.snapshot().document().expression(&id(120)).unwrap();
    assert_eq!(exp.entries.len(), 1);
    assert_eq!(exp.entries[0].id, id(235));
    assert!(exp
        .source_scene()
        .unwrap()
        .tracks
        .iter()
        .all(|t| !matches!(t.target, MotionTrackTarget::Parameter { .. })));
    assert!(s
        .snapshot()
        .document()
        .model3_settings()
        .groups
        .as_ref()
        .unwrap()[0]
        .parameters
        .is_empty());
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
    apply(&mut s, &roots(vec![DeletionTarget::Parameter(id(230))]));
    assert!(s.snapshot().document().blend_key_table(&id(231)).is_none());
    assert!(s.snapshot().document().blend_binding(&id(233)).is_none());
    assert!(s.snapshot().document().mesh(&id(10)).is_some());
}

#[test]
fn multiple_axes_reduce_at_frozen_defaults_and_keep_the_other_parameter_domain() {
    let mut s = fixture();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(240),
                        minimum: 0.,
                        maximum: 1.,
                        default_value: 0.25,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                let mut p = e
                    .read_document()
                    .unwrap()
                    .parameter(&id(3))
                    .unwrap()
                    .clone();
                p.default_value = 0.4;
                e.replace_parameter(p)?;
                let mut b = e.read_document().unwrap().binding(&id(20)).unwrap().clone();
                b.axes.push(BindingAxis::new(id(240), vec![0., 1.]));
                let base = e
                    .read_document()
                    .unwrap()
                    .mesh(&id(10))
                    .unwrap()
                    .base_positions
                    .clone();
                b.keyforms = [(0., 0.), (1., 0.), (0., 1.), (1., 1.)]
                    .into_iter()
                    .map(|(x, y)| MeshKeyform {
                        keys: vec![x, y],
                        samples: vec![],
                        positions: base
                            .iter()
                            .map(|p| Vec2::new(p.x + x * 10. + y * 5., p.y))
                            .collect(),
                        appearance: Appearance::default(),
                        draw_order: None,
                    })
                    .collect();
                e.replace_binding(b)
            })
            .and_then(|value| {
                let prepared = draft.prepare("two dimensions")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.snapshot().document().clone();
    let eval = |s: &AuthoringSession, v: f32| {
        let mut values = PreviewValues::default();
        if s.snapshot().document().parameter(&id(240)).is_some() {
            values.insert(id(240), v);
        }
        crate::support::frame(s.snapshot(), &values)
            .unwrap()
            .drawables
            .into_iter()
            .find(|v| v.id == id(10))
            .unwrap()
            .positions
    };
    let expected: Vec<_> = [0., 0.3, 0.8, 1.]
        .into_iter()
        .map(|v| eval(&s, v))
        .collect();
    apply(&mut s, &roots(vec![DeletionTarget::Parameter(id(3))]));
    assert_eq!(
        s.snapshot().document().binding(&id(20)).unwrap().axes.len(),
        1
    );
    for (i, v) in [0., 0.3, 0.8, 1.].into_iter().enumerate() {
        for (a, b) in expected[i].iter().zip(eval(&s, v)) {
            assert!((a.x - b.x).abs() < 1e-4);
        }
    }
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
    let expected = eval(&s, 0.25);
    apply(
        &mut s,
        &roots(vec![
            DeletionTarget::Parameter(id(240)),
            DeletionTarget::Parameter(id(3)),
        ]),
    );
    let actual = eval(&s, 0.25);
    for (a, b) in expected.iter().zip(actual) {
        assert!((a.x - b.x).abs() < 1e-4);
    }
}
#[test]
fn blend_base_sample_requires_an_explicit_retained_base_and_last_sample_has_a_choice() {
    let mut s = fixture();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(250),
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
                            id: id(251),
                            parameter_id: id(250),
                            keys: vec![0., 1.],
                            base_key_idx: 0,
                            samples: vec![],
                            base_sample_id: String::new(),
                        },
                    },
                    Default::default(),
                    &Default::default(),
                )
                .map(|_| ())
            })
            .and_then(|value| {
                let prepared = draft.prepare("blend table")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let table = s.snapshot().document().blend_key_table(&id(251)).unwrap();
    let base = table.base_sample_id.clone();
    let replacement = table.samples[1].id.clone();
    let mut request = roots(vec![DeletionTarget::Record(DeletionObject {
        kind: DeletionKind::Sample,
        id: base,
        owner_id: None,
    })]);
    assert!(analyze(&s, &request)
        .unwrap()
        .0
        .decisions
        .iter()
        .any(|d| d.reason == DeletionDecisionReason::BlendBaseReplacementRequired));
    request
        .blend_base_replacements
        .insert(id(251), replacement.clone());
    apply(&mut s, &request);
    assert_eq!(
        s.snapshot()
            .document()
            .blend_key_table(&id(251))
            .unwrap()
            .base_sample_id,
        replacement
    );
    let request = roots(vec![DeletionTarget::Record(DeletionObject {
        kind: DeletionKind::Sample,
        id: replacement,
        owner_id: None,
    })]);
    let report = analyze(&s, &request).unwrap();
    assert!(report
        .0
        .decisions
        .iter()
        .any(|d| d.object.id == id(251) && d.choices.contains(&DeletionChoice::DeleteSubtree)));
}

#[test]
fn unconfigured_blend_table_is_not_a_deletion_target() {
    let mut s = fixture();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let binding = BlendShapeBinding {
                    id: id(260),
                    target_id: id(10),
                    target_kind: BlendShapeTargetKind::Mesh,
                    key_table_id: String::new(),
                    constraint_ids: vec![],
                    keyforms: DeltaKeyforms::Mesh(vec![]),
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
            })
            .and_then(|value| {
                let prepared = draft.prepare("pending Blend binding")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.snapshot().document().clone();
    let request = roots(vec![DeletionTarget::Record(DeletionObject {
        kind: DeletionKind::BlendKeyTable,
        id: String::new(),
        owner_id: None,
    })]);
    let error = prepare_deletion(&s, &request).unwrap_err();
    assert_eq!(error.code.as_str(), "MISSING_DELETION_TARGET");
    assert_eq!(s.snapshot().content_id(), before.content_id());
    assert!(s.snapshot().document().blend_binding(&id(260)).is_some());
}

#[test]
fn deleting_motion_group_migrates_the_last_registration_and_explicit_last_delete_needs_choice() {
    let mut s = scene::tree();
    scene::animation(&mut s);
    let entry = s
        .snapshot()
        .document()
        .motion_roots()
        .iter()
        .find(|e| e.clip_id == id(100))
        .unwrap()
        .clone();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.edit_motion(MotionEdit::CreateGroup {
                    id: id(260),
                    name: "Group".into(),
                })?;
                e.edit_motion(MotionEdit::MoveRegistrations {
                    ids: vec![entry.id.clone()],
                    destination: MotionContainer::Group(id(260)),
                    before: None,
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("motion group")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before = s.snapshot().document().clone();
    let request = roots(vec![record(DeletionKind::MotionGroup, 260)]);
    let report = analyze(&s, &request).unwrap();
    assert!(report
        .0
        .rewritten
        .iter()
        .any(|r| r.field == DeletionField::MotionRegistrationContainer && r.owner.id == entry.id));
    assert!(!report.0.removed.iter().any(|r| r.object.id == entry.id));
    apply(&mut s, &request);
    assert!(s.snapshot().document().motion_groups().is_empty());
    assert_eq!(s.snapshot().document().motion_roots(), &[entry.clone()]);
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
    let request = roots(vec![DeletionTarget::Record(DeletionObject {
        kind: DeletionKind::MotionRegistration,
        id: entry.id,
        owner_id: None,
    })]);
    assert!(analyze(&s,&request).unwrap().0.decisions.iter().any(|d|matches!(&d.reason,DeletionDecisionReason::InvalidContainer {code} if code=="LAST_MOTION_REGISTRATION")));
}
#[test]
fn scene_binding_removal_requires_and_applies_an_explicit_offscreen_mapping() {
    let mut s = fixture();
    let mut request = roots(vec![record(DeletionKind::SceneBinding, 5)]);
    assert!(analyze(&s, &request)
        .unwrap()
        .0
        .decisions
        .iter()
        .any(|d| d.reason == DeletionDecisionReason::OffscreenMappingRequired));
    request.offscreen_placements.insert(
        id(40),
        DeletionOffscreenPlacement {
            part_id: id(4),
            mapping: Some(vec![-1]),
        },
    );
    apply(&mut s, &request);
    assert!(s.snapshot().document().scene_binding(&id(5)).is_none());
    assert_eq!(
        s.snapshot()
            .document()
            .offscreen(&id(40))
            .unwrap()
            .part_keyform_indices,
        vec![-1]
    );
    assert!(s.snapshot().document().part(&id(4)).is_some());
}
#[test]
fn native_repeat_and_decimal_snapping_are_frozen_instead_of_raw_interpolation() {
    for (repeat, precision, value) in [(true, 6, 1.), (false, 0, 0.4)] {
        let mut s = fixture();
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| {
                    let mut p = e
                        .read_document()
                        .unwrap()
                        .parameter(&id(3))
                        .unwrap()
                        .clone();
                    p.repeat = repeat;
                    p.decimal_places = precision;
                    p.default_value = value;
                    e.replace_parameter(p)?;
                    let mut b = e.read_document().unwrap().binding(&id(20)).unwrap().clone();
                    for v in &mut b.keyforms[1].positions {
                        v.x += 10.;
                    }
                    e.replace_binding(b)
                })
                .and_then(|value| {
                    let prepared = draft.prepare("native parameter selection")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        let positions = |s: &AuthoringSession| {
            crate::support::frame(s.snapshot(), &PreviewValues::default())
                .unwrap()
                .drawables
                .into_iter()
                .find(|d| d.id == id(10))
                .unwrap()
                .positions
        };
        let before = positions(&s);
        apply(&mut s, &roots(vec![DeletionTarget::Parameter(id(3))]));
        assert_eq!(positions(&s), before);
    }
}

#[test]
fn opaque_parameter_cleanup_reports_all_native_export_guards_without_rewriting_provenance() {
    use zipzi_project::OpaqueExportProblem as P;
    let mut s = fixture();
    physics(&mut s);
    let runtime = s
        .snapshot()
        .document()
        .parameter(&id(3))
        .unwrap()
        .runtime_id
        .clone();
    let mut wire: serde_json::Value = serde_json::from_str(
        &zipzi_project::export_physics3(s.snapshot().document())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    wire["Vendor"] = serde_json::json!({"reference":runtime});
    {
        let mut draft = s.begin_draft();
        draft.batch(draft.source(), |e| {
        {let imported=zipzi_project::import_physics3(&e.read_document()?, &id(200),&wire.to_string()).map_err(|error|Status::error(error.code.clone(),format!("{:?}",error)))?;e.replace_physics(Some(imported.candidate.physics().as_ref().unwrap().as_ref().clone()))}?;
        {let imported=zipzi_project::import_expression3(&e.read_document()?, &id(270),"Expression",&serde_json::json!({"Type":"Live2D Expression","Parameters":[{"Id":runtime,"Value":0.4,"Blend":"Add"}],"Vendor":true}).to_string()).map_err(|error|Status::error(error.code.clone(),format!("{:?}",error)))?;e.create_expression(imported.candidate.expression(&id(270)).unwrap().clone())}?;
        {let imported=zipzi_project::import_motion3(&e.read_document()?, &id(271),"Motion",&serde_json::json!({"Version":3,"Meta":{"Duration":1.,"Fps":30.,"Loop":false,"AreBeziersRestricted":true,"CurveCount":1,"TotalSegmentCount":0,"TotalPointCount":1,"UserDataCount":0,"TotalUserDataSize":0},"Curves":[{"Target":"Parameter","Id":runtime,"Segments":[0.,0.]}],"Vendor":true}).to_string()).map_err(|error|Status::error(error.code.clone(),format!("{:?}",error)))?;e.create_motion(imported.candidate.motion(&id(271)).unwrap().clone())}?;
        {let imported=zipzi_project::import_cdi3(&e.read_document()?, &serde_json::json!({"Version":3,"Parameters":[{"Id":runtime,"Name":"Control","GroupId":""}],"Vendor":true}).to_string()).map_err(|error|Status::error(error.code.clone(),format!("{:?}",error)))?;e.replace_display_info(imported.candidate.display_info().clone())}?;
        Ok(())
    }).and_then(|value| {
            let prepared = draft.prepare("opaque parameter consumers")?;
            s.publish(prepared).map(|receipt| (value, receipt))
        })
    }.unwrap();
    let before = s.snapshot().document().clone();
    assert!(zipzi_project::preflight_opaque_exports(
        s.snapshot().document(),
        &ConversionControl::default()
    )
    .unwrap()
    .is_empty());
    let request = roots(vec![DeletionTarget::Parameter(id(3))]);
    let report = analyze(&s, &request).unwrap();
    for problem in [P::NamespaceChanged, P::ContentChanged] {
        assert!(
            report
                .1
                .contains(&zipzi_project::OpaqueExportIssue::Motion {
                    id: id(271),
                    problem
                }),
            "{:?}",
            report.1
        );
        assert!(report
            .1
            .contains(&zipzi_project::OpaqueExportIssue::Expression {
                id: id(270),
                problem
            }));
        assert!(report
            .1
            .contains(&zipzi_project::OpaqueExportIssue::Physics {
                id: id(200),
                problem
            }));
    }
    assert!(report.1.contains(&zipzi_project::OpaqueExportIssue::Cdi {
        problem: P::NamespaceChanged
    }));
    assert!(!report.1.is_empty());
    apply(&mut s, &request);
    assert_eq!(
        s.snapshot()
            .document()
            .physics()
            .as_ref()
            .unwrap()
            .opaque_source_content_hash,
        before
            .physics()
            .as_ref()
            .unwrap()
            .opaque_source_content_hash
    );
    assert_eq!(
        s.snapshot()
            .document()
            .physics()
            .as_ref()
            .unwrap()
            .opaque_source_ids,
        before.physics().as_ref().unwrap().opaque_source_ids
    );
    assert_eq!(
        s.snapshot()
            .document()
            .expression(&id(270))
            .unwrap()
            .opaque_source_content_hash,
        before
            .expression(&id(270))
            .unwrap()
            .opaque_source_content_hash
    );
    assert_eq!(
        s.snapshot()
            .document()
            .motion(&id(271))
            .unwrap()
            .opaque_source_ids,
        before.motion(&id(271)).unwrap().opaque_source_ids
    );
    assert_eq!(
        s.snapshot().document().display_info().opaque_source_ids,
        before.display_info().opaque_source_ids
    );
    assert!(zipzi_project::export_motion3(s.snapshot().document(), &id(271)).is_err());
    assert!(zipzi_project::export_expression3(s.snapshot().document(), &id(270)).is_err());
    assert!(zipzi_project::export_physics3(s.snapshot().document()).is_err());
    assert!(zipzi_project::export_cdi3(s.snapshot().document()).is_err());
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
    assert!(zipzi_project::preflight_opaque_exports(
        s.snapshot().document(),
        &ConversionControl::default()
    )
    .unwrap()
    .is_empty());
}

#[test]
fn retained_motion_keys_and_physics_node_order_keep_native_lock_protection() {
    let mut s = scene::tree();
    scene::animation(&mut s);
    lock(&mut s, &id(102));
    let request = roots(vec![record(DeletionKind::MotionKey, 106)]);
    let before = s.snapshot().document().clone();
    let history = {
        let history = s.history_state();
        (history.undo_steps, history.redo_steps)
    };
    let report = analyze(&s, &request).unwrap();
    assert!(report.0.blockers.iter().any(|b| b.source_id == id(102)));
    assert!(prepare_deletion(&s, &request).is_err());
    assert_eq!(s.snapshot().content_id(), before.content_id());
    assert_eq!(
        {
            let history = s.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    let mut s = fixture();
    physics(&mut s);
    let nodes = s
        .snapshot()
        .document()
        .physics()
        .as_ref()
        .unwrap()
        .data
        .settings[0]
        .vertices
        .iter()
        .map(|v| v.id.clone())
        .collect::<Vec<_>>();
    lock(&mut s, &nodes[2]);
    let request = roots(vec![DeletionTarget::Record(DeletionObject {
        kind: DeletionKind::PhysicsNode,
        id: nodes[1].clone(),
        owner_id: None,
    })]);
    let before = s.snapshot().document().clone();
    assert!(analyze(&s, &request)
        .unwrap()
        .0
        .blockers
        .iter()
        .any(|b| b.source_id == nodes[2]));
    assert_eq!(s.snapshot().content_id(), before.content_id());
}
