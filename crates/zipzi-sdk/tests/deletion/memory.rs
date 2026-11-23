use super::*;
fn large_fixture() -> AuthoringSession {
    let mut s = fixture();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut mesh = e.read_document().unwrap().mesh(&id(12)).unwrap().clone();
                mesh.id = id(70);
                mesh.runtime_id = id(70);
                mesh.name = "x".repeat(12 * 1024 * 1024);
                // Rebuilding an unrelated vertex index must not consume the deletion
                // budget either. These unused vertices are valid authored geometry.
                for vertex in 100..120_100 {
                    mesh.vertex_ids.push(vertex);
                    mesh.base_positions.push(Vec2::default());
                    mesh.uvs.push(Vec2::default());
                }
                crate::support::insert_mesh(e, mesh)?;
                {
                    let binding = BlendShapeBinding {
                        id: id(60),
                        target_id: id(70),
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
                }
            })
            .and_then(|value| {
                let prepared = draft.prepare("large unrelated record")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    s
}

#[test]
fn small_deletion_shares_unrelated_records_through_preparation_publication_and_history() {
    let mut s = large_fixture();
    let source = s.snapshot().clone();
    let limits = ConversionLimits {
        max_workspace_bytes: 1024 * 1024,
        ..Default::default()
    };
    let (outcome, p) = crate::support::prepared(&s, |c| {
        c.edit_deletion(&request(&[10]), limits, &Default::default())
    })
    .unwrap();
    assert!(outcome.work.workspace_bytes < limits.max_workspace_bytes);
    for state in [p.snapshot().document(), source.document()] {
        assert!(std::ptr::eq(
            source.document().mesh(&id(70)).unwrap(),
            state.mesh(&id(70)).unwrap()
        ));
        assert!(std::ptr::eq(
            source.document().binding(&id(22)).unwrap(),
            state.binding(&id(22)).unwrap()
        ));
        assert!(std::ptr::eq(
            source.document().scene_binding(&id(5)).unwrap(),
            state.scene_binding(&id(5)).unwrap()
        ));
        assert!(std::ptr::eq(
            source.document().blend_binding(&id(60)).unwrap(),
            state.blend_binding(&id(60)).unwrap()
        ));
    }
    s.publish(p).unwrap();
    assert!(s.snapshot().document().mesh(&id(10)).is_none());
    assert!(std::ptr::eq(
        source.document().mesh(&id(70)).unwrap(),
        s.snapshot().document().mesh(&id(70)).unwrap()
    ));
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), source.content_id());
    s.redo(s.version()).unwrap();
    assert!(s.snapshot().document().mesh(&id(10)).is_none());
}

#[test]
fn deletion_budget_only_includes_curves_reached_by_the_request() {
    let mut s = fixture();
    let (_, prepared) = crate::support::prepared(&s, |c| {
        c.create_motion(MotionClip {
            id: id(80),
            name: "Dense".into(),
            duration: 6000.,
            fps: 30.,
            looping: false,
            restricted_beziers: false,
            fade_in: None,
            fade_out: None,
            track_order: vec![],
            tracks: vec![
                MotionTrack {
                    id: id(81),
                    target: MotionTrackTarget::Parameter {
                        parameter_id: id(3),
                    },
                    initial: MotionKey {
                        id: id(82),
                        time: 0.,
                        value: 0.,
                    },
                    segments: std::sync::Arc::new(
                        (1..=6000)
                            .map(|n| {
                                MotionSegment::new(
                                    MotionKey {
                                        id: id(10_000 + n),
                                        time: n as f32,
                                        value: 0.,
                                    },
                                    MotionCurve::Linear {},
                                )
                            })
                            .collect(),
                    ),
                    fade_in: None,
                    fade_out: None,
                    extensions: Default::default(),
                },
                MotionTrack {
                    id: id(83),
                    target: MotionTrackTarget::PartOpacity { part_id: id(4) },
                    initial: MotionKey {
                        id: id(84),
                        time: 0.,
                        value: 1.,
                    },
                    segments: Default::default(),
                    fade_in: None,
                    fade_out: None,
                    extensions: Default::default(),
                },
            ]
            .into(),
            events: Default::default(),
            extensions: Default::default(),
            meta_extensions: Default::default(),
            opaque_source_ids: None,
            opaque_source_content_hash: None,
        })
    })
    .unwrap();
    s.publish(prepared).unwrap();
    let before = s.snapshot().clone();
    let limits = ConversionLimits {
        max_workspace_bytes: 1024 * 1024,
        ..Default::default()
    };
    let control = ConversionControl::default();

    let (outcome, prepared) =
        crate::support::prepared(&s, |c| c.edit_deletion(&request(&[10]), limits, &control))
            .unwrap();
    assert!(outcome.work.workspace_bytes < limits.max_workspace_bytes);
    s.publish(prepared).unwrap();
    assert!(s.snapshot().document().mesh(&id(10)).is_none());
    assert!(std::ptr::eq(
        before.document().motion(&id(80)).unwrap(),
        s.snapshot().document().motion(&id(80)).unwrap(),
    ));

    let sparse_track = DeletionRequest {
        roots: vec![DeletionTarget::Record(DeletionObject {
            kind: DeletionKind::MotionTrack,
            id: id(83),
            owner_id: None,
        })],
        ..Default::default()
    };
    let (_, prepared) =
        crate::support::prepared(&s, |c| c.edit_deletion(&sparse_track, limits, &control)).unwrap();
    s.publish(prepared).unwrap();
    let retained = s.snapshot().document().motion(&id(80)).unwrap();
    assert_eq!(retained.tracks.len(), 1);
    assert!(std::sync::Arc::ptr_eq(
        &before.document().motion(&id(80)).unwrap().tracks[0].segments,
        &retained.tracks[0].segments,
    ));

    let affected = DeletionRequest {
        roots: vec![DeletionTarget::Parameter(id(3))],
        ..Default::default()
    };
    assert_eq!(
        s.snapshot()
            .document()
            .analyze_deletion(&affected, limits, &control)
            .unwrap_err()
            .code,
        "AUTHORING_MEMORY_LIMIT",
    );
}

#[test]
fn deletion_workspace_budget_charges_rewrites_and_reports_without_changing_redo() {
    let mut s = large_fixture();
    let (_, p) = crate::support::prepared(&s, |c| {
        c.rename(zipzi_sdk::NameTarget::Mesh(&id(11)), "redo".into())
    })
    .unwrap();
    s.publish(p).unwrap();
    s.undo(s.version()).unwrap();
    let source = s.read_snapshot();
    let h = s.history_state();
    let (outcome, _) = prepare_deletion(&s, &request(&[10])).unwrap();
    let limits = ConversionLimits {
        max_workspace_bytes: outcome.work.workspace_bytes - 1,
        ..Default::default()
    };
    assert_eq!(
        crate::support::prepared(&s, |c| c.review_deletion(
            &request(&[10]),
            limits,
            &Default::default()
        ))
        .unwrap_err()
        .code
        .as_str(),
        "AUTHORING_MEMORY_LIMIT"
    );
    let mut texture = DeletionRequest {
        roots: vec![DeletionTarget::Texture(id(2))],
        ..Default::default()
    };
    texture.texture_replacements.insert(id(2), None);
    assert_eq!(
        crate::support::prepared(&s, |c| c.edit_deletion(
            &texture,
            ConversionLimits {
                max_workspace_bytes: 1024 * 1024,
                ..Default::default()
            },
            &Default::default()
        ))
        .unwrap_err()
        .code
        .as_str(),
        "AUTHORING_MEMORY_LIMIT"
    );
    assert_eq!(s.read_snapshot().source, source.source);
    assert_eq!(s.history_state(), h);
    lock(&mut s, &id(11));
    let (blocked, p) = crate::support::prepared(&s, |c| {
        c.review_deletion(&request(&[10]), Default::default(), &Default::default())
    })
    .unwrap();
    assert!(!blocked.report.blockers.is_empty());
    assert!(p.writes().is_empty());
}
