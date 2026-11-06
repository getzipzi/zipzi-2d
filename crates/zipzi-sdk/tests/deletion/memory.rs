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
