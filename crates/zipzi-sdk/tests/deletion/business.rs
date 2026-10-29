use super::*;
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
    let analysis = s.analyze_deletion(request).unwrap();
    assert!(
        analysis.native.decisions.is_empty(),
        "{:?}",
        analysis.native
    );
    assert!(
        analysis.native.structural_blockers.is_empty(),
        "{:?}",
        analysis.native
    );
    let plan = s
        .plan_deletion(
            request,
            &DeletionOptions {
                accepted_export_failures: analysis.new_export_failures,
            },
        )
        .unwrap();
    s.execute_deletion_plan(&plan).unwrap();
}
#[test]
fn parameter_default_reduces_all_ordinary_grids_and_cleans_animation_in_one_undo() {
    let mut s = scene::tree();
    scene::animation(&mut s);
    s.edit("default", None, |e| {
        let mut p = e
            .candidate_document()
            .get_parameter(&id(3))
            .unwrap()
            .clone();
        p.default_value = 0.4;
        e.replace_parameter(p)?;
        let mut b = e.candidate_document().get_binding(&id(20)).unwrap().clone();
        for v in &mut b.keyforms[1].positions {
            v.x += 10.;
        }
        e.replace_binding(b)
    })
    .unwrap();
    let before = s.document().clone();
    let evaluated = s.evaluate(&PreviewValues::default()).unwrap();
    apply(&mut s, &roots(vec![DeletionTarget::Parameter(id(3))]));
    assert!(s.document().get_parameter(&id(3)).is_none());
    for n in 20..=22 {
        let b = s.document().get_binding(&id(n)).unwrap();
        assert!(b.axes.is_empty());
        assert!(b.keyforms.is_empty());
    }
    assert!(s
        .document()
        .get_glue(&id(30))
        .unwrap()
        .binding
        .as_ref()
        .unwrap()
        .axes
        .is_empty());
    let clip = s.document().get_motion(&id(100)).unwrap();
    assert_eq!(clip.tracks.len(), 1);
    assert_eq!(clip.tracks[0].id, id(101));
    assert_eq!(clip.events.len(), 1);
    let after = s.evaluate(&PreviewValues::default()).unwrap();
    for drawable in &evaluated.drawables {
        let result = after
            .drawables
            .iter()
            .find(|v| v.id == drawable.id)
            .unwrap();
        assert_eq!(drawable.positions, result.positions);
    }
    assert!(s.document().validate_structure().is_empty());
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
    s.redo().unwrap();
    assert!(s.document().get_parameter(&id(3)).is_none());
}
#[test]
fn texture_requires_explicit_resolution_and_attachment_cleanup_keeps_users() {
    let mut s = fixture();
    let mut request = roots(vec![DeletionTarget::Texture(id(2))]);
    let review = s.analyze_deletion(&request).unwrap();
    assert!(review
        .native
        .decisions
        .iter()
        .any(|d| d.reason == DeletionDecisionReason::TextureReplacementRequired));
    assert!(s
        .plan_deletion(&request, &DeletionOptions::default())
        .is_err());
    request.texture_replacements.insert(id(2), None);
    apply(&mut s, &request);
    for n in 10..=12 {
        assert_eq!(
            s.document().get_mesh(&id(n)).unwrap().texture_asset_id,
            SceneReference::Unspecified
        );
    }
    assert!(s.document().get_asset(&id(2)).is_none());
    s.undo().unwrap();
    assert!(s.document().get_asset(&id(2)).is_some());
}
#[test]
fn deleting_motion_removes_registrations_but_keeps_other_business_assets() {
    let mut s = scene::tree();
    scene::animation(&mut s);
    let before = s.document().clone();
    apply(&mut s, &roots(vec![record(DeletionKind::Motion, 100)]));
    assert!(s.document().get_motion(&id(100)).is_none());
    assert!(s.document().pose().is_some());
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
}
#[test]
fn drawing_and_glue_roles_do_not_delete_their_host() {
    let mut s = fixture();
    apply(&mut s, &roots(vec![record(DeletionKind::GlueBinding, 30)]));
    assert!(s.document().get_glue(&id(30)).unwrap().binding.is_none());
    assert!(s.document().get_binding(&id(20)).is_some());
    s.undo().unwrap();
    s.edit("drawing role", None, |e| {
        e.replace_draw_order_groups(vec![
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
        ])
    })
    .unwrap();
    let mut request = roots(vec![
        record(DeletionKind::DrawingGroup, 4),
        DeletionTarget::Offscreen(id(40)),
    ]);
    request
        .reparent
        .insert(id(4), ReparentStrategy::KeepLocalData);
    apply(&mut s, &request);
    assert!(s.document().get_part(&id(4)).is_some());
    assert!(s.document().get_scene_binding(&id(5)).is_some());
}
#[test]
fn samples_and_keys_use_native_interpolation_and_empty_tracks_are_reported() {
    let mut s = scene::tree();
    scene::animation(&mut s);
    let sample = s.document().get_binding(&id(20)).unwrap().axes[0].samples[1]
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
    assert!(s.document().binding_identity(&sample).is_none());
    assert_eq!(
        s.document().get_motion(&id(100)).unwrap().tracks[0]
            .keys()
            .count(),
        1
    );
    s.undo().unwrap();
    let request = roots(vec![
        record(DeletionKind::MotionKey, 102),
        record(DeletionKind::MotionKey, 106),
    ]);
    let report = s.analyze_deletion(&request).unwrap();
    assert!(report
        .native
        .removed
        .iter()
        .any(|r| r.object.kind == DeletionKind::MotionTrack && r.object.id == id(101)));
    apply(&mut s, &request);
    assert!(s
        .document()
        .get_motion(&id(100))
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
    s.edit("physics", None, |e| e.set_physics(asset)).unwrap();
}
#[test]
fn parameter_physics_cleanup_and_invalid_chain_choices_honor_channel_locks() {
    let mut s = fixture();
    physics(&mut s);
    let group = s.document().physics().unwrap().data.settings[0].id.clone();
    let input = s.document().physics().unwrap().data.settings[0].inputs[0]
        .id
        .clone();
    let mut request = roots(vec![DeletionTarget::Parameter(id(3))]);
    let before = s.document().clone();
    let report = s.analyze_deletion(&request).unwrap();
    assert!(report.native.decisions.is_empty());
    let mut invalid = request.clone();
    invalid.roots.extend(
        s.document().physics().unwrap().data.settings[0]
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
    assert!(s
        .analyze_deletion(&invalid)
        .unwrap()
        .native
        .decisions
        .iter()
        .any(|d| d.object.id == group
            && matches!(d.reason, DeletionDecisionReason::InvalidContainer { .. })));
    assert!(s.document().same_content(&before));
    request.roots.push(DeletionTarget::Record(DeletionObject {
        kind: DeletionKind::PhysicsGroup,
        id: group,
        owner_id: None,
    }));
    lock(&mut s, &input);
    assert!(s
        .analyze_deletion(&request)
        .unwrap()
        .native
        .blockers
        .iter()
        .any(|b| b.source_id == input));
    s.undo().unwrap();
    apply(&mut s, &request);
    assert!(s.document().physics().unwrap().data.settings.is_empty());
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
}
#[test]
fn physics_node_replacement_preserves_output_and_rejects_deleted_or_root_destinations() {
    let mut s = fixture();
    physics(&mut s);
    let group = &s.document().physics().unwrap().data.settings[0];
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
    let group = &s.document().physics().unwrap().data.settings[0];
    assert_eq!(group.vertices.len(), 2);
    assert_eq!(group.outputs[0].id, output);
    assert_eq!(group.outputs[0].node_id, replacement);
    s.undo().unwrap();
    request
        .physics_node_replacements
        .insert(removed.clone(), removed);
    assert!(!s
        .analyze_deletion(&request)
        .unwrap()
        .native
        .decisions
        .is_empty());
}
#[test]
fn parameter_folder_dissolves_by_default_and_explicit_contents_delete_uses_same_cleanup() {
    let mut s = fixture();
    s.edit("folders", None, |e| {
        for (n, parent) in [(210, None), (211, Some(id(210)))] {
            e.create_parameter_group(CdiParameterGroup {
                id: id(n),
                runtime_id: format!("G{n}"),
                name: "folder".into(),
                parent_id: parent,
                extensions: Default::default(),
            })?;
        }
        e.set_parameter_group(&id(3), Some(&id(211)))?;
        let mut info = e.candidate_document().display_info().clone();
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
    .unwrap();
    let before = s.document().clone();
    apply(
        &mut s,
        &roots(vec![DeletionTarget::ParameterFolder(id(210))]),
    );
    assert!(s.document().get_parameter(&id(3)).is_some());
    assert_eq!(
        s.document()
            .get_parameter_group(&id(211))
            .unwrap()
            .parent_id,
        None
    );
    assert!(s.document().display_info().parameters.as_ref().unwrap().iter().any(|e|matches!(e,CdiParameterEntry::Unresolved {runtime_id,group_id:None,..} if runtime_id=="legacy")));
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
    apply(
        &mut s,
        &roots(vec![DeletionTarget::ParameterFolderContents(id(210))]),
    );
    assert!(s.document().get_parameter(&id(3)).is_none());
    assert!(s.document().get_parameter_group(&id(211)).is_none());
    assert!(s
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
    s.edit("attachment", None, |e| {
        e.set_package_attachments(vec![PackageAttachment {
            id: id(220),
            path: "audio.wav".into(),
            bytes: Some(std::sync::Arc::new(vec![1, 2, 3])),
        }])?;
        let mut settings = e.candidate_document().model3_settings().clone();
        settings.user_data = Some(AttachmentRef::Resolved {
            attachment_id: id(220),
        });
        e.set_model3_settings(settings)
    })
    .unwrap();
    let before = s.document().clone();
    apply(&mut s, &roots(vec![DeletionTarget::Attachment(id(220))]));
    assert!(s.document().get_attachment(&id(220)).is_none());
    assert!(s.document().model3_settings().user_data.is_none());
    assert!(s.document().get_motion(&id(100)).is_some());
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
}
#[test]
fn default_outside_grid_retains_native_disabled_behavior_and_explicit_coordinate_replans() {
    let mut s = fixture();
    s.edit("outside", None, |e| {
        let mut p = e
            .candidate_document()
            .get_parameter(&id(3))
            .unwrap()
            .clone();
        p.maximum = 2.;
        p.default_value = 1.5;
        e.replace_parameter(p)
    })
    .unwrap();
    let before = s.document().clone();
    let mut request = roots(vec![DeletionTarget::Parameter(id(3))]);
    let report = s.analyze_deletion(&request).unwrap();
    assert!(report.native.effects.iter().any(|e|matches!(e,DeletionEffect::ParameterDimensionRemoved {disabled:true,value,..} if *value==1.5)));
    apply(&mut s, &request);
    for n in 10..=12 {
        assert!(!s.document().get_mesh(&id(n)).unwrap().enabled);
    }
    assert!(!s.document().get_part(&id(4)).unwrap().enabled);
    assert!(s
        .document()
        .get_glue(&id(30))
        .unwrap()
        .binding
        .as_ref()
        .unwrap()
        .axes
        .is_empty());
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
    request.parameter_slices.insert(id(3), 0.5);
    apply(&mut s, &request);
    assert!(s.document().get_mesh(&id(10)).unwrap().enabled);
}
#[test]
fn parameter_cleanup_removes_expression_entries_metadata_and_shared_constraints() {
    let mut s = scene::tree();
    scene::animation(&mut s);
    s.edit("parameter consumers", None, |e| {
        e.create_parameter(Parameter {
            id: id(230),
            minimum: 0.,
            maximum: 1.,
            kind: ParameterKind::BlendShape,
            ..Default::default()
        })?;
        e.create_blend_key_table(BlendShapeKeyTable {
            id: id(231),
            parameter_id: id(230),
            keys: vec![0., 1.],
            base_key_idx: 0,
            samples: vec![],
            base_sample_id: String::new(),
        })?;
        e.create_blend_constraint(BlendShapeConstraint {
            id: id(232),
            parameter_id: id(3),
            keys: vec![0., 1.],
            weights: vec![1., 1.],
            samples: vec![],
        })?;
        e.create_blend_binding(BlendShapeBinding {
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
        })?;
        let mut expression = e
            .candidate_document()
            .get_expression(&id(120))
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
        let mut settings = e.candidate_document().model3_settings().clone();
        settings.groups = Some(vec![ModelParameterGroup {
            id: id(236),
            name: "params".into(),
            parameters: vec![ReferenceEntry {
                id: id(237),
                target: ModelTargetRef::Resolved { object_id: id(3) },
            }],
            extensions: Default::default(),
        }]);
        e.set_model3_settings(settings)
    })
    .unwrap();
    let before = s.document().clone();
    apply(&mut s, &roots(vec![DeletionTarget::Parameter(id(3))]));
    assert!(s.document().get_blend_constraint(&id(232)).is_none());
    assert!(s
        .document()
        .get_blend_binding(&id(233))
        .unwrap()
        .constraint_ids
        .is_empty());
    let exp = s.document().get_expression(&id(120)).unwrap();
    assert_eq!(exp.entries.len(), 1);
    assert_eq!(exp.entries[0].id, id(235));
    assert!(exp
        .source_scene()
        .unwrap()
        .tracks
        .iter()
        .all(|t| !matches!(t.target, MotionTrackTarget::Parameter { .. })));
    assert!(s.document().model3_settings().groups.as_ref().unwrap()[0]
        .parameters
        .is_empty());
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
    apply(&mut s, &roots(vec![DeletionTarget::Parameter(id(230))]));
    assert!(s.document().get_blend_key_table(&id(231)).is_none());
    assert!(s.document().get_blend_binding(&id(233)).is_none());
    assert!(s.document().get_mesh(&id(10)).is_some());
}

#[test]
fn multiple_axes_reduce_at_frozen_defaults_and_keep_the_other_parameter_domain() {
    let mut s = fixture();
    s.edit("two dimensions", None, |e| {
        e.create_parameter(Parameter {
            id: id(240),
            minimum: 0.,
            maximum: 1.,
            default_value: 0.25,
            ..Default::default()
        })?;
        let mut p = e
            .candidate_document()
            .get_parameter(&id(3))
            .unwrap()
            .clone();
        p.default_value = 0.4;
        e.replace_parameter(p)?;
        let mut b = e.candidate_document().get_binding(&id(20)).unwrap().clone();
        b.axes.push(BindingAxis::new(id(240), vec![0., 1.]));
        let base = e
            .candidate_document()
            .get_mesh(&id(10))
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
    .unwrap();
    let before = s.document().clone();
    let eval = |s: &AuthoringSession, v: f32| {
        let mut values = PreviewValues::default();
        if s.document().get_parameter(&id(240)).is_some() {
            values.insert(id(240), v);
        }
        s.evaluate(&values)
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
    assert_eq!(s.document().get_binding(&id(20)).unwrap().axes.len(), 1);
    for (i, v) in [0., 0.3, 0.8, 1.].into_iter().enumerate() {
        for (a, b) in expected[i].iter().zip(eval(&s, v)) {
            assert!((a.x - b.x).abs() < 1e-4);
        }
    }
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
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
    s.edit("blend table", None, |e| {
        e.create_parameter(Parameter {
            id: id(250),
            minimum: 0.,
            maximum: 1.,
            kind: ParameterKind::BlendShape,
            ..Default::default()
        })?;
        e.create_blend_key_table(BlendShapeKeyTable {
            id: id(251),
            parameter_id: id(250),
            keys: vec![0., 1.],
            base_key_idx: 0,
            samples: vec![],
            base_sample_id: String::new(),
        })
    })
    .unwrap();
    let table = s.document().get_blend_key_table(&id(251)).unwrap();
    let base = table.base_sample_id.clone();
    let replacement = table.samples[1].id.clone();
    let mut request = roots(vec![DeletionTarget::Record(DeletionObject {
        kind: DeletionKind::Sample,
        id: base,
        owner_id: None,
    })]);
    assert!(s
        .analyze_deletion(&request)
        .unwrap()
        .native
        .decisions
        .iter()
        .any(|d| d.reason == DeletionDecisionReason::BlendBaseReplacementRequired));
    request
        .blend_base_replacements
        .insert(id(251), replacement.clone());
    apply(&mut s, &request);
    assert_eq!(
        s.document()
            .get_blend_key_table(&id(251))
            .unwrap()
            .base_sample_id,
        replacement
    );
    let request = roots(vec![DeletionTarget::Record(DeletionObject {
        kind: DeletionKind::Sample,
        id: replacement,
        owner_id: None,
    })]);
    let report = s.analyze_deletion(&request).unwrap();
    assert!(report
        .native
        .decisions
        .iter()
        .any(|d| d.object.id == id(251) && d.choices.contains(&DeletionChoice::DeleteSubtree)));
}

#[test]
fn deleting_motion_group_migrates_the_last_registration_and_explicit_last_delete_needs_choice() {
    let mut s = scene::tree();
    scene::animation(&mut s);
    let entry = s
        .document()
        .motion_roots()
        .iter()
        .find(|e| e.clip_id == id(100))
        .unwrap()
        .clone();
    s.edit("motion group", None, |e| {
        e.set_motion_library(
            vec![MotionGroup {
                id: id(260),
                name: "Group".into(),
                entries: vec![entry.clone()],
            }],
            vec![],
        )
    })
    .unwrap();
    let before = s.document().clone();
    let request = roots(vec![record(DeletionKind::MotionGroup, 260)]);
    let report = s.analyze_deletion(&request).unwrap();
    assert!(report
        .native
        .rewritten
        .iter()
        .any(|r| r.field == DeletionField::MotionRegistrationContainer && r.owner.id == entry.id));
    assert!(!report
        .native
        .removed
        .iter()
        .any(|r| r.object.id == entry.id));
    apply(&mut s, &request);
    assert!(s.document().motion_groups().is_empty());
    assert_eq!(s.document().motion_roots(), &[entry.clone()]);
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
    let request = roots(vec![DeletionTarget::Record(DeletionObject {
        kind: DeletionKind::MotionRegistration,
        id: entry.id,
        owner_id: None,
    })]);
    assert!(s.analyze_deletion(&request).unwrap().native.decisions.iter().any(|d|matches!(&d.reason,DeletionDecisionReason::InvalidContainer {code} if code=="LAST_MOTION_REGISTRATION")));
}
#[test]
fn scene_binding_removal_requires_and_applies_an_explicit_offscreen_mapping() {
    let mut s = fixture();
    let mut request = roots(vec![record(DeletionKind::SceneBinding, 5)]);
    assert!(s
        .analyze_deletion(&request)
        .unwrap()
        .native
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
    assert!(s.document().get_scene_binding(&id(5)).is_none());
    assert_eq!(
        s.document()
            .get_offscreen(&id(40))
            .unwrap()
            .part_keyform_indices,
        vec![-1]
    );
    assert!(s.document().get_part(&id(4)).is_some());
}
#[test]
fn native_repeat_and_decimal_snapping_are_frozen_instead_of_raw_interpolation() {
    for (repeat, precision, value) in [(true, 6, 1.), (false, 0, 0.4)] {
        let mut s = fixture();
        s.edit("native parameter selection", None, |e| {
            let mut p = e
                .candidate_document()
                .get_parameter(&id(3))
                .unwrap()
                .clone();
            p.repeat = repeat;
            p.decimal_places = precision;
            p.default_value = value;
            e.replace_parameter(p)?;
            let mut b = e.candidate_document().get_binding(&id(20)).unwrap().clone();
            for v in &mut b.keyforms[1].positions {
                v.x += 10.;
            }
            e.replace_binding(b)
        })
        .unwrap();
        let positions = |s: &AuthoringSession| {
            s.evaluate(&PreviewValues::default())
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
    use zipzi_sdk::OpaqueExportProblem as P;
    let mut s = fixture();
    physics(&mut s);
    let runtime = s
        .document()
        .get_parameter(&id(3))
        .unwrap()
        .runtime_id
        .clone();
    let mut wire: serde_json::Value = serde_json::from_str(
        &zipzi_project::export_physics3(s.document())
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    wire["Vendor"] = serde_json::json!({"reference":runtime});
    s.edit("opaque parameter consumers",None,|e| {
        e.import_physics3(&id(200),&wire.to_string())?;
        e.import_expression3(&id(270),"Expression",&serde_json::json!({"Type":"Live2D Expression","Parameters":[{"Id":runtime,"Value":0.4,"Blend":"Add"}],"Vendor":true}).to_string())?;
        e.import_motion3(&id(271),"Motion",&serde_json::json!({"Version":3,"Meta":{"Duration":1.,"Fps":30.,"Loop":false,"AreBeziersRestricted":true,"CurveCount":1,"TotalSegmentCount":0,"TotalPointCount":1,"UserDataCount":0,"TotalUserDataSize":0},"Curves":[{"Target":"Parameter","Id":runtime,"Segments":[0.,0.]}],"Vendor":true}).to_string())?;
        e.import_cdi3(&serde_json::json!({"Version":3,"Parameters":[{"Id":runtime,"Name":"Control","GroupId":""}],"Vendor":true}).to_string())?;
        Ok(())
    }).unwrap();
    let before = s.document().clone();
    assert!(
        zipzi_project::preflight_opaque_exports(s.document(), &ConversionControl::default())
            .unwrap()
            .is_empty()
    );
    let request = roots(vec![DeletionTarget::Parameter(id(3))]);
    let report = s.analyze_deletion(&request).unwrap();
    for problem in [P::NamespaceChanged, P::ContentChanged] {
        assert!(
            report
                .new_export_failures
                .contains(&DeletionExportFailure::Motion {
                    id: id(271),
                    problem
                }),
            "{:?}",
            report.new_export_failures
        );
        assert!(report
            .new_export_failures
            .contains(&DeletionExportFailure::Expression {
                id: id(270),
                problem
            }));
        assert!(report
            .new_export_failures
            .contains(&DeletionExportFailure::Physics {
                id: id(200),
                problem
            }));
    }
    assert!(report
        .new_export_failures
        .contains(&DeletionExportFailure::Cdi {
            problem: P::NamespaceChanged
        }));
    assert!(s
        .plan_deletion(&request, &DeletionOptions::default())
        .is_err());
    apply(&mut s, &request);
    assert_eq!(
        s.document().physics().unwrap().opaque_source_content_hash,
        before.physics().unwrap().opaque_source_content_hash
    );
    assert_eq!(
        s.document().physics().unwrap().opaque_source_ids,
        before.physics().unwrap().opaque_source_ids
    );
    assert_eq!(
        s.document()
            .get_expression(&id(270))
            .unwrap()
            .opaque_source_content_hash,
        before
            .get_expression(&id(270))
            .unwrap()
            .opaque_source_content_hash
    );
    assert_eq!(
        s.document().get_motion(&id(271)).unwrap().opaque_source_ids,
        before.get_motion(&id(271)).unwrap().opaque_source_ids
    );
    assert_eq!(
        s.document().display_info().opaque_source_ids,
        before.display_info().opaque_source_ids
    );
    assert!(zipzi_project::export_motion3(s.document(), &id(271)).is_err());
    assert!(zipzi_project::export_expression3(s.document(), &id(270)).is_err());
    assert!(zipzi_project::export_physics3(s.document()).is_err());
    assert!(zipzi_project::export_cdi3(s.document()).is_err());
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
    assert!(
        zipzi_project::preflight_opaque_exports(s.document(), &ConversionControl::default())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn retained_motion_keys_and_physics_node_order_keep_native_lock_protection() {
    let mut s = scene::tree();
    scene::animation(&mut s);
    lock(&mut s, &id(102));
    let request = roots(vec![record(DeletionKind::MotionKey, 106)]);
    let before = s.document().clone();
    let history = s.history_lengths();
    let report = s.analyze_deletion(&request).unwrap();
    assert!(report
        .native
        .blockers
        .iter()
        .any(|b| b.source_id == id(102)));
    assert!(s
        .plan_deletion(&request, &DeletionOptions::default())
        .is_err());
    assert!(s.document().same_content(&before));
    assert_eq!(s.history_lengths(), history);
    let mut s = fixture();
    physics(&mut s);
    let nodes = s.document().physics().unwrap().data.settings[0]
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
    let before = s.document().clone();
    assert!(s
        .analyze_deletion(&request)
        .unwrap()
        .native
        .blockers
        .iter()
        .any(|b| b.source_id == nodes[2]));
    assert!(s.document().same_content(&before));
}
