use serde_json::json;
use std::sync::Arc;
use zipzi_core::draw_order::DrawOrderGroup;
use zipzi_core::{document::*, state::*, *};

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
pub(super) fn owned() -> OwnedDocument {
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
    input.editor_state.cursor_preview_bindings = Some(
        [(
            id(4),
            CursorPreviewBinding {
                axis: CursorPreviewAxis::Y,
                influence_percent: 75.,
                reversed: true,
            },
        )]
        .into(),
    );
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
