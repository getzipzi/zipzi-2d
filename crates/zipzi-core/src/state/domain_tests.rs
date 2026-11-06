use super::*;
use serde_json::json;

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn parameter(n: u32, kind: ParameterKind) -> Parameter {
    Parameter {
        id: id(n),
        runtime_id: format!("param{n}"),
        kind,
        minimum: 0.,
        maximum: 1.,
        default_value: 0.,
        ..Default::default()
    }
}
fn mesh(n: u32) -> Mesh {
    Mesh {
        id: id(n),
        runtime_id: format!("mesh{n}"),
        part_id: id(2),
        deformer_id: id(3),
        vertex_ids: vec![1, 2, 3],
        base_positions: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
        uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
        triangles: vec![[1, 2, 3]],
        ..Default::default()
    }
}
fn clip() -> MotionClip {
    MotionClip {
        id: id(30),
        name: "motion".into(),
        duration: 1.,
        fps: 30.,
        looping: false,
        restricted_beziers: false,
        fade_in: None,
        fade_out: None,
        track_order: vec![],
        tracks: vec![MotionTrack {
            id: id(31),
            target: MotionTrackTarget::Parameter {
                parameter_id: id(4),
            },
            initial: MotionKey {
                id: id(32),
                time: 0.,
                value: 0.,
            },
            segments: Arc::new(vec![MotionSegment::Linear {
                end: MotionKey {
                    id: id(33),
                    time: 1.,
                    value: 1.,
                },
            }]),
            fade_in: None,
            fade_out: None,
            extensions: Default::default(),
        }]
        .into(),
        events: vec![MotionEvent {
            id: id(34),
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
fn owned() -> OwnedDocument {
    let mut input = OwnedDocument {
        id: id(1),
        ..Default::default()
    };
    input.assets.push(AssetRecord {
        id: id(50),
        name: "missing".into(),
        content: AssetContent::Unresolved {
            source: "art.png".into(),
        },
    });
    input.parts.push(Part {
        id: id(2),
        runtime_id: "part".into(),
        ..Default::default()
    });
    input.transforms.push(Transform {
        id: id(3),
        runtime_id: "rotation".into(),
        part_id: Some(id(2).into()),
        ..Default::default()
    });
    input.parameters = vec![
        parameter(4, ParameterKind::Normal),
        parameter(5, ParameterKind::BlendShape),
    ];
    input.meshes = vec![mesh(6), mesh(7)];
    input.bindings.push(MeshBinding {
        id: id(8),
        mesh_id: id(6),
        axes: vec![BindingAxis::new(id(4), vec![0., 1.])],
        keyforms: vec![
            MeshKeyform {
                keys: vec![0.],
                positions: mesh(6).base_positions,
                ..Default::default()
            },
            MeshKeyform {
                keys: vec![1.],
                positions: mesh(6).base_positions,
                ..Default::default()
            },
        ],
    });
    input.scene_bindings.push(SceneBinding {
        id: id(9),
        axes: vec![],
        track: SceneTrack::Rotation {
            target_id: id(3).into(),
            keyforms: vec![],
        },
    });
    input.blend_key_tables.push(BlendShapeKeyTable {
        id: id(10),
        parameter_id: id(5),
        keys: vec![0., 1.],
        base_key_idx: 0,
        samples: vec![],
        base_sample_id: String::new(),
    });
    input.blend_constraints.push(BlendShapeConstraint {
        id: id(11),
        parameter_id: id(4),
        keys: vec![0., 1.],
        weights: vec![0., 1.],
        samples: vec![],
    });
    input.blend_bindings.push(BlendShapeBinding {
        id: id(12),
        target_kind: BlendShapeTargetKind::Mesh,
        target_id: id(6),
        key_table_id: id(10),
        constraint_ids: vec![id(11)],
        keyforms: DeltaKeyforms::Mesh(vec![
            DeltaMeshKeyform {
                positions: vec![Vec2::default(); 3],
                ..Default::default()
            },
            DeltaMeshKeyform {
                positions: vec![Vec2::new(1., 0.); 3],
                ..Default::default()
            },
        ]),
    });
    input.glues.push(Glue {
        id: id(13),
        runtime_id: "glue".into(),
        mesh_a_id: id(6).into(),
        mesh_b_id: id(7).into(),
        ..Default::default()
    });
    input.offscreens.push(Offscreen {
        id: id(14),
        runtime_id: "offscreen".into(),
        part_id: id(2),
        ..Default::default()
    });
    input.expressions.push(ExpressionAsset {
        id: id(20),
        name: "expression".into(),
        file_type: None,
        fade_in: Some(0.1),
        fade_out: None,
        entries: vec![ExpressionEntry {
            id: id(21),
            target: ExpressionTarget::Resolved {
                parameter_id: id(4),
            },
            value: 0.5,
            blend: None,
            extensions: Default::default(),
        }],
        extensions: Default::default(),
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    });
    input.motions.push(clip());
    input.motion_groups.push(MotionGroup {
        id: id(35),
        name: "idle".into(),
        entries: vec![MotionRegistration {
            id: id(36),
            clip_id: id(30),
            fade_in: None,
            fade_out: None,
            sound: Some(AttachmentRef::Resolved {
                attachment_id: id(40),
            }),
            extensions: Default::default(),
        }],
    });
    input.pose = Some(Arc::new(PoseAsset {
        id: id(22),
        file_type: None,
        fade_in: Some(0.3),
        groups: vec![PoseGroup {
            id: id(23),
            name: "pose".into(),
            entries: vec![PoseEntry {
                id: id(24),
                part: PosePartRef::Resolved { part_id: id(2) },
                links: None,
                extensions: Default::default(),
            }]
            .into(),
        }]
        .into(),
        extensions: Default::default(),
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    }));
    input.physics = Some(Arc::new(PhysicsAsset::empty(id(25))));
    input.package_attachments.push(PackageAttachment {
        id: id(40),
        path: "sound/a.wav".into(),
        bytes: Some(Arc::new(vec![1, 2, 3])),
    });
    input.missing_attachments.push("unavailable.bin".into());
    input.editor_state.objects.insert(
        id(31),
        ObjectEditorState {
            hidden_in_editor: true,
            locked: false,
        },
    );
    input.editor_state.rotation_bone_lengths.insert(id(3), 128.);
    input.hierarchy_order.organization = vec![id(2), id(6), id(7)];
    input.hierarchy_order.deformation = vec![id(3), id(6)];
    input.hierarchy_order.glues = vec![id(13)];
    input.draw_order_groups = Some(vec![DrawOrderGroup {
        owner: String::new(),
        items: vec![id(6), id(7)],
        min_order: -10,
        max_order: 10,
    }]);
    input.model3_settings.hit_areas = Some(vec![ModelHitArea {
        id: id(41),
        name: "hit".into(),
        mesh: ModelTargetRef::Resolved { object_id: id(6) },
        extensions: Default::default(),
    }]);
    input.model3_settings.user_data = Some(AttachmentRef::Resolved {
        attachment_id: id(40),
    });
    input
        .model3_settings
        .extensions
        .insert("vendor".into(), json!({"opaque":[1,"keep"]}));
    input.model3_settings.source_content = Some("opaque original".into());
    input
        .display_info
        .extensions
        .insert("vendor".into(), json!({"display":true}));
    input
}
fn complete() -> DocumentState {
    DocumentState::from_owned(owned()).unwrap()
}

#[test]
fn owned_construction_covers_every_persistent_domain_and_moves_geometry() {
    let input = owned();
    let positions = input.meshes[0].base_positions.as_ptr();
    let opaque = input.model3_settings.clone();
    let state = DocumentState::from_owned(input).unwrap();
    assert_eq!(
        state.mesh(&id(6)).unwrap().base_positions.as_ptr(),
        positions
    );
    assert_eq!(state.model3_settings(), &opaque);
    assert_eq!(
        state.missing_attachments(),
        &vec!["unavailable.bin".to_owned()]
    );
    assert_eq!(
        state.pose().as_ref().unwrap().groups[0].entries[0].id,
        id(24)
    );
    assert_eq!(state.physics().as_ref().unwrap().id, id(25));
    assert_eq!(
        state.package_attachments()[0]
            .bytes
            .as_ref()
            .unwrap()
            .as_slice(),
        &[1, 2, 3]
    );
    assert_eq!(state.binding(&id(8)).unwrap().keyforms[1].samples.len(), 1);
    assert_eq!(state.blend_key_table(&id(10)).unwrap().samples.len(), 2);
    assert_eq!(state.blend_constraint(&id(11)).unwrap().samples.len(), 2);
    assert!(state.builder().freeze().unwrap().writes.is_empty());
}

#[test]
fn unrelated_edit_shares_every_domain_root_and_record() {
    let base = complete();
    let mut builder = base.builder();
    let mut parameter = base.parameter(&id(4)).unwrap().clone();
    parameter.name = "renamed".into();
    builder.replace_parameter(parameter).unwrap();
    let result = builder.freeze().unwrap();
    assert_eq!(result.writes.records().len(), 1);
    assert!(result.writes.estimated_bytes() > 0);
    assert!(std::ptr::eq(
        base.mesh(&id(6)).unwrap(),
        result.state.mesh(&id(6)).unwrap()
    ));
    assert!(std::ptr::eq(
        base.binding(&id(8)).unwrap(),
        result.state.binding(&id(8)).unwrap()
    ));
    assert!(std::ptr::eq(
        base.motion(&id(30)).unwrap(),
        result.state.motion(&id(30)).unwrap()
    ));
    assert!(std::ptr::eq(base.pose(), result.state.pose()));
    assert!(std::ptr::eq(base.physics(), result.state.physics()));
    assert!(std::ptr::eq(
        base.model3_settings(),
        result.state.model3_settings()
    ));
    assert!(std::ptr::eq(
        base.package_attachments(),
        result.state.package_attachments()
    ));
    assert!(std::ptr::eq(
        base.editor_state(),
        result.state.editor_state()
    ));
}

#[test]
fn full_graph_rejects_dangling_targets_and_cross_domain_identity_collisions() {
    let mut bad = owned();
    bad.meshes[0].part_id = id(999);
    assert_eq!(
        DocumentState::from_owned(bad).unwrap_err().code,
        "MISSING_PART"
    );
    let mut bad = owned();
    bad.meshes[0].triangles[0][0] = 99;
    assert_eq!(
        DocumentState::from_owned(bad).unwrap_err().code,
        "MISSING_VERTEX"
    );
    let mut bad = owned();
    bad.parts[0].parent_id = id(2);
    assert_eq!(
        DocumentState::from_owned(bad).unwrap_err().code,
        "RELATION_CYCLE"
    );
    let mut bad = owned();
    bad.expressions[0].entries[0].id = id(33);
    assert_eq!(
        DocumentState::from_owned(bad).unwrap_err().code,
        "DUPLICATE_ID"
    );
    let mut bad = owned();
    bad.editor_state.objects.insert(id(999), Default::default());
    assert_eq!(
        DocumentState::from_owned(bad).unwrap_err().code,
        "INVALID_EDITOR_STATE"
    );
    let mut bad = owned();
    bad.model3_settings.hit_areas.as_mut().unwrap()[0].mesh =
        ModelTargetRef::Resolved { object_id: id(999) };
    assert_eq!(
        DocumentState::from_owned(bad).unwrap_err().code,
        "INVALID_MODEL3_HIT_AREAS"
    );
    let mut bad = owned();
    bad.package_attachments[0].path = "../escape".into();
    assert_eq!(
        DocumentState::from_owned(bad).unwrap_err().code,
        "INVALID_ATTACHMENT_PATH"
    );
}

#[test]
fn freezing_rechecks_dependencies_changed_outside_their_record() {
    let base = complete();
    let mut builder = base.builder();
    builder.remove_parameter(&id(4)).unwrap();
    assert!(builder.freeze().is_err());
    assert!(base.parameter(&id(4)).is_some());
    let mut builder = base.builder();
    let mut parameter = base.parameter(&id(4)).unwrap().clone();
    parameter.maximum = 0.5;
    builder.replace_parameter(parameter).unwrap();
    assert_eq!(builder.freeze().unwrap_err().code, "INVALID_KEYS");
    let mut builder = base.builder();
    builder.remove_asset(&id(50)).unwrap();
    let result = builder.freeze().unwrap();
    assert!(result.state.asset(&id(50)).is_none());
    assert_eq!(result.writes.records()[0].change, RecordChange::Deleted);
}

#[test]
fn metadata_only_order_changes_and_net_zero_roots_are_tracked() {
    let base = complete();
    let mut builder = base.builder();
    builder
        .reorder(RecordDomain::Mesh, vec![id(7), id(6)])
        .unwrap();
    let mut editor = base.editor_state().clone();
    editor.objects.clear();
    builder.set_editor_state(editor).unwrap();
    builder
        .set_editor_state(base.editor_state().clone())
        .unwrap();
    let mut parameter = base.parameter(&id(4)).unwrap().clone();
    parameter.runtime_id = "temporary".into();
    builder.replace_parameter(parameter).unwrap();
    builder
        .replace_parameter(base.parameter(&id(4)).unwrap().clone())
        .unwrap();
    let result = builder.freeze().unwrap();
    assert!(result.writes.records().is_empty());
    assert!(result.writes.order_changed(RecordDomain::Mesh));
    assert!(!result.writes.root_changed(RootDomain::EditorState));
    assert!(Arc::ptr_eq(
        &base.content.parameter_runtime_ids,
        &result.state.content.parameter_runtime_ids,
    ));
    assert!(std::ptr::eq(
        base.editor_state(),
        result.state.editor_state()
    ));
    let mut builder = base.builder();
    assert!(builder
        .reorder(RecordDomain::Mesh, vec![id(6), id(6)])
        .is_err());
    assert!(builder.freeze().is_err());
}

#[test]
fn motion_rename_estimate_excludes_shared_curve_payloads() {
    fn cost(segments: usize) -> usize {
        let mut input = owned();
        let clip = &mut input.motions[0];
        let track = &mut clip.tracks[0];
        track.segments = Arc::new(
            (0..segments)
                .map(|n| MotionSegment::Linear {
                    end: MotionKey {
                        id: id(1000 + n as u32),
                        time: (n + 1) as f32 / segments as f32,
                        value: 0.,
                    },
                })
                .collect(),
        );
        let state = DocumentState::from_owned(input).unwrap();
        let before = state.motion(&id(30)).unwrap();
        let mut renamed = before.clone();
        renamed.name = "renamed".into();
        let mut builder = state.builder();
        builder.replace_motion(renamed).unwrap();
        let result = builder.freeze().unwrap();
        assert!(Arc::ptr_eq(
            &before.tracks[0].segments,
            &result.state.motion(&id(30)).unwrap().tracks[0].segments
        ));
        result.writes.estimated_bytes()
    }
    assert_eq!(cost(1), cost(10000));
}

#[test]
fn changed_coarse_blocks_charge_both_endpoints_but_noop_has_zero_cost() {
    let base = complete();
    assert_eq!(base.builder().freeze().unwrap().writes.estimated_bytes(), 0);
    let mut builder = base.builder();
    let mut changed = base.mesh(&id(6)).unwrap().clone();
    changed.base_positions[0].x = 0.1;
    builder.replace_mesh(changed).unwrap();
    let result = builder.freeze().unwrap();
    assert_eq!(result.writes.records()[0].domain, RecordDomain::Mesh);
    assert!(result.writes.estimated_bytes() >= 2 * std::mem::size_of::<Mesh>());
    assert_eq!(base.mesh(&id(6)).unwrap().base_positions[0].x, 0.);
}

#[test]
fn attachment_step_estimate_deduplicates_shared_bytes_and_retains_old_versions() {
    fn cost(length: usize) -> usize {
        let base = DocumentState::new(id(1), Default::default()).unwrap();
        let bytes = Arc::new(vec![0u8; length]);
        let mut builder = base.builder();
        builder
            .set_package_attachments(vec![
                PackageAttachment {
                    id: id(100),
                    path: "a.bin".into(),
                    bytes: Some(bytes.clone()),
                },
                PackageAttachment {
                    id: id(101),
                    path: "b.bin".into(),
                    bytes: Some(bytes),
                },
            ])
            .unwrap();
        builder.freeze().unwrap().writes.estimated_bytes()
    }
    assert_eq!(cost(10000) - cost(1000), 9000);
    let base = complete();
    let old = base.package_attachments()[0]
        .bytes
        .as_ref()
        .unwrap()
        .clone();
    let mut attachments = base.package_attachments().clone();
    attachments[0].bytes = Some(Arc::new(vec![9; 10000]));
    let mut builder = base.builder();
    builder.set_package_attachments(attachments).unwrap();
    let result = builder.freeze().unwrap();
    assert!(result.writes.root_changed(RootDomain::PackageAttachments));
    assert!(result.writes.estimated_bytes() >= 10003);
    assert_eq!(&**old, &[1, 2, 3]);
    assert_eq!(
        result.state.package_attachments()[0]
            .bytes
            .as_ref()
            .unwrap()[0],
        9
    );
}

#[test]
fn invalid_business_records_and_metadata_fail_freeze_without_publishing() {
    let base = complete();
    let mut builder = base.builder();
    let mut clip = base.motion(&id(30)).unwrap().clone();
    clip.tracks[0].initial.value = f32::NAN;
    builder.replace_motion(clip).unwrap();
    assert_eq!(builder.freeze().unwrap_err().code, "INVALID_MOTION_POINT");
    let mut builder = base.builder();
    let mut expression = base.expression(&id(20)).unwrap().clone();
    expression.entries[0].target = ExpressionTarget::Resolved {
        parameter_id: id(999),
    };
    builder.replace_expression(expression).unwrap();
    assert_eq!(builder.freeze().unwrap_err().code, "MISSING_PARAMETER");
    let mut builder = base.builder();
    let mut pose = base.pose().as_ref().unwrap().as_ref().clone();
    pose.fade_in = Some(-1.);
    builder.set_pose(Some(Arc::new(pose))).unwrap();
    assert_eq!(builder.freeze().unwrap_err().code, "INVALID_POSE_FADE");
    let mut builder = base.builder();
    let mut physics = base.physics().as_ref().unwrap().as_ref().clone();
    physics.data.meta.fps = Some(-1.);
    builder.set_physics(Some(Arc::new(physics))).unwrap();
    assert!(builder.freeze().is_err());
    let mut builder = base.builder();
    let mut info = base.display_info().clone();
    info.parameters = Some(vec![CdiParameterEntry::Resolved {
        parameter_id: id(999),
        group_id: None,
        extensions: Default::default(),
    }]);
    builder.set_display_info(info).unwrap();
    assert!(builder.freeze().is_err());
    assert!(base.builder().freeze().is_ok());
}

#[test]
fn captured_capabilities_follow_owned_locks_without_session_handles() {
    use zipzi_contract::authoring::CapabilityState as C;
    let before = complete();
    assert!(matches!(
        before.expression_capabilities(&id(20)).unwrap().delete,
        C::Available
    ));
    assert!(matches!(
        before.pose_capabilities(&id(22)).unwrap().delete,
        C::Available
    ));
    assert!(matches!(
        before.motion_capabilities(&id(30)).unwrap().delete,
        C::Available
    ));
    assert!(matches!(
        before.physics_capabilities(&id(25)).unwrap().patch,
        C::Available
    ));
    let mut builder = before.builder();
    let mut flags = before.editor_state().clone();
    for child in [21, 24, 32, 36, 25] {
        flags.objects.entry(id(child)).or_default().locked = true;
    }
    builder.set_editor_state(flags).unwrap();
    let after = builder.freeze().unwrap().state;
    let locked = |state| {
        matches!(
            state,
            C::Unavailable {
                code: "OBJECT_LOCKED"
            }
        )
    };
    assert!(locked(
        after.expression_capabilities(&id(20)).unwrap().delete
    ));
    assert!(locked(after.pose_capabilities(&id(22)).unwrap().delete));
    assert!(locked(after.motion_capabilities(&id(30)).unwrap().delete));
    assert!(locked(after.motion_capabilities(&id(31)).unwrap().delete));
    assert!(locked(after.motion_capabilities(&id(35)).unwrap().delete));
    assert!(locked(after.physics_capabilities(&id(25)).unwrap().patch));
    // Parent patch availability is independent of deleting a locked descendant.
    assert!(matches!(
        after.expression_capabilities(&id(20)).unwrap().patch,
        C::Available
    ));
    // Earlier snapshots retain their own capability answers.
    assert!(matches!(
        before.motion_capabilities(&id(30)).unwrap().delete,
        C::Available
    ));
    assert!(matches!(
        before.pose_capabilities(&id(22)).unwrap().delete,
        C::Available
    ));
    assert_eq!(
        after.pose_capabilities(&id(30)).unwrap_err().code,
        "MISSING_OBJECT"
    );
    assert_eq!(
        after.expression_capabilities(&id(99)).unwrap_err().code,
        "MISSING_OBJECT"
    );
    assert_eq!(
        after
            .builder()
            .edit_expression(ExpressionEdit::Delete { id: id(20) })
            .unwrap_err()
            .code,
        "OBJECT_LOCKED"
    );
    assert_eq!(
        after
            .builder()
            .edit_pose(PoseEdit::Delete { ids: vec![id(22)] })
            .unwrap_err()
            .code,
        "OBJECT_LOCKED"
    );
}

#[test]
fn typed_collection_queries_validate_roles_and_inherit_locks() {
    use zipzi_contract::{
        authoring::{CapabilityState as C, ObjectKind as K},
        kernel_wire::CollectionTarget as T,
    };
    let before = complete();
    assert!(before.contains_object(K::Mesh, &id(6)));
    assert!(!before.contains_object(K::Parameter, &id(6)));
    assert!(before.contains_object(K::DrawOrderGroup, before.id()));
    assert!(!before.contains_object(K::DrawOrderGroup, &id(6)));
    let entries = T::ExpressionEntries {
        expression_id: id(20),
    };
    assert!(matches!(
        before.collection_capabilities(&entries).unwrap().create,
        C::Available
    ));
    assert_eq!(
        before
            .collection_capabilities(&T::ExpressionEntries {
                expression_id: id(21)
            })
            .unwrap_err()
            .code,
        "MISSING_OBJECT"
    );
    assert_eq!(
        before
            .collection_capabilities(&T::Pose(PoseCollection::Entries { group_id: id(24) }))
            .unwrap_err()
            .code,
        "MISSING_OBJECT"
    );
    assert!(matches!(
        before
            .binding_capabilities(K::MeshBinding, &id(8))
            .unwrap()
            .patch,
        C::Available
    ));
    assert!(matches!(
        before.scene_capabilities(K::Mesh, &id(6)).unwrap().patch,
        C::Available
    ));
    assert!(matches!(
        before
            .resource_capabilities(K::Asset, &id(50))
            .unwrap()
            .load_content,
        C::Available
    ));
    assert!(matches!(
        before
            .reference_capabilities(K::ModelHitArea, &id(41))
            .unwrap()
            .patch,
        C::Available
    ));
    let mut builder = before.builder();
    let mut flags = before.editor_state().clone();
    for owner in [2, 20, 23, 35, 40, 50] {
        flags.objects.entry(id(owner)).or_default().locked = true;
    }
    builder.set_editor_state(flags).unwrap();
    let after = builder.freeze().unwrap().state;
    let locked = |state| {
        matches!(
            state,
            C::Unavailable {
                code: "OBJECT_LOCKED"
            }
        )
    };
    assert!(locked(
        after.collection_capabilities(&entries).unwrap().create
    ));
    assert!(locked(
        after
            .collection_capabilities(&T::Pose(PoseCollection::Entries { group_id: id(23) }))
            .unwrap()
            .create
    ));
    assert!(locked(
        after
            .collection_capabilities(&T::Motion(MotionContainer::Group(id(35))))
            .unwrap()
            .create
    ));
    assert!(locked(
        after
            .collection_capabilities(&T::Organization(OrganizationCollection::PartMembers {
                part_id: Some(id(2))
            }))
            .unwrap()
            .create
    ));
    assert!(locked(
        after
            .binding_capabilities(K::MeshBinding, &id(8))
            .unwrap()
            .patch
    ));
    assert!(locked(
        after.scene_capabilities(K::Mesh, &id(6)).unwrap().patch
    ));
    assert!(locked(
        after
            .scene_content_capabilities(K::Mesh, &id(6))
            .unwrap()
            .edit
    ));
    assert!(locked(
        after
            .resource_capabilities(K::Asset, &id(50))
            .unwrap()
            .patch
    ));
    assert!(locked(
        after
            .attachment_capabilities(K::Attachment, &id(40))
            .unwrap()
            .patch
    ));
    assert!(matches!(
        before.collection_capabilities(&entries).unwrap().create,
        C::Available
    ));
}
