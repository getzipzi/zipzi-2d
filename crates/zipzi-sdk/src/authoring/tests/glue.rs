use super::*;

fn glue_weight_session(weights: &[[f32; 2]]) -> (AuthoringSession, String) {
    let initial = snapshot();
    let mut builder = initial.document().builder();
    let glue = id(10);
    let mut peer = mesh();
    peer.id = id(6);
    peer.runtime_id = "peer".into();
    builder.create_mesh(peer).unwrap();
    builder
        .create_glue(zipzi_core::Glue {
            id: glue.clone(),
            runtime_id: "roundoff-glue".into(),
            mesh_a_id: id(4).into(),
            mesh_b_id: id(6).into(),
            intensity: 0.75,
            pairs: weights
                .iter()
                .enumerate()
                .map(|(i, &[a, b])| zipzi_core::GlueVertexPair {
                    id: id(20 + i as u32),
                    vertex_a: Some(i as u32 + 1),
                    vertex_b: Some(i as u32 + 1),
                    weight_a: a,
                    weight_b: b,
                })
                .collect(),
            ..Default::default()
        })
        .unwrap();
    let session = AuthoringSession::new(
        Snapshot::new(builder.freeze().unwrap().state, ResourceView::default()).unwrap(),
    );
    (session, glue)
}

#[test]
fn glue_imported_weight_roundoff_is_editable_and_only_requested_writes_are_normalized() {
    // The three overflowing pairs from Mao's Glue__ArtMesh180__ArtMesh179.
    let weights = [
        [0.5723187, 0.42768192],
        [0.8380362, 0.16196397],
        [0.6087419, 0.39125824],
    ];
    assert!(weights.iter().all(|[a, b]| a + b > 1.));
    let (mut session, glue_id) = glue_weight_session(&weights);
    let original = session
        .snapshot()
        .document()
        .glue(&glue_id)
        .unwrap()
        .clone();
    let content = session.snapshot().content_id();
    validate_glue_weight_edit(session.snapshot().document(), &original).unwrap();
    assert_eq!(session.snapshot().content_id(), content);
    assert_eq!(
        session.snapshot().document().glue(&glue_id),
        Some(&original)
    );
    let mut draft = session.begin_draft();
    draft
        .batch(draft.source(), |edit| {
            edit.set_glue_weights(&glue_id, &[(original.pairs[0].id.as_str(), weights[0])])
        })
        .unwrap();
    session
        .publish(draft.prepare("Paint one pair").unwrap())
        .unwrap();
    let written = session
        .snapshot()
        .document()
        .glue(&glue_id)
        .unwrap()
        .clone();
    let p = &written.pairs[0];
    assert!(p.weight_a >= 0. && p.weight_b >= 0. && p.weight_a + p.weight_b <= 1.);
    assert!((p.weight_a - weights[0][0]).abs() < 1e-6 && (p.weight_b - weights[0][1]).abs() < 1e-6);
    let before_ratio = f64::from(weights[0][0]) / f64::from(weights[0][1]);
    let after_ratio = f64::from(p.weight_a) / f64::from(p.weight_b);
    assert!((before_ratio - after_ratio).abs() < 1e-6);
    assert_eq!(&written.pairs[1..], &original.pairs[1..]);
    assert_eq!(written.intensity, original.intensity);
    assert_eq!(written.pairs[0].id, original.pairs[0].id);
    session.undo(session.version()).unwrap();
    assert_eq!(
        session.snapshot().document().glue(&glue_id),
        Some(&original)
    );
    session.redo(session.version()).unwrap();
    assert_eq!(session.snapshot().document().glue(&glue_id), Some(&written));
}

#[test]
fn glue_weight_tolerance_keeps_exact_values_and_rejects_real_overflow_or_nonfinite_values() {
    let (mut session, glue_id) = glue_weight_session(&[[0.5, 0.5]]);
    let pair = session.snapshot().document().glue(&glue_id).unwrap().pairs[0]
        .id
        .clone();
    for weights in [
        [0.8, 0.2],
        [0., 0.],
        [1. + 4. * f32::EPSILON, 0.],
        [0., 1. + 4. * f32::EPSILON],
    ] {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.set_glue_weights(&glue_id, &[(pair.as_str(), weights)])
            })
            .unwrap();
        session
            .publish(draft.prepare("Write weights").unwrap())
            .unwrap();
        let stored = &session.snapshot().document().glue(&glue_id).unwrap().pairs[0];
        if weights[0] + weights[1] <= 1. {
            assert_eq!([stored.weight_a, stored.weight_b], weights);
        } else {
            assert!(
                stored.weight_a >= 0.
                    && stored.weight_b >= 0.
                    && stored.weight_a + stored.weight_b <= 1.
            );
        }
    }
    let before = session
        .snapshot()
        .document()
        .glue(&glue_id)
        .unwrap()
        .clone();
    let history = session.history_state().undo_steps;
    for weights in [
        [0.5, 0.500002],
        [0.75, 0.75],
        [-f32::EPSILON, 1.],
        [f32::NAN, 0.],
        [0., f32::INFINITY],
    ] {
        let mut draft = session.begin_draft();
        let error = draft
            .batch(draft.source(), |edit| {
                edit.set_glue_weights(&glue_id, &[(pair.as_str(), weights)])
            })
            .unwrap_err();
        assert_eq!(error.code, "INVALID_GLUE_WEIGHTS");
        assert_eq!(session.snapshot().document().glue(&glue_id), Some(&before));
        assert_eq!(session.history_state().undo_steps, history);
    }
    let (session, glue_id) = glue_weight_session(&[[0.5, 0.500002]]);
    let glue = session.snapshot().document().glue(&glue_id).unwrap();
    assert_eq!(
        validate_glue_weight_edit(session.snapshot().document(), glue)
            .unwrap_err()
            .code,
        "INVALID_GLUE_WEIGHTS"
    );
}

#[test]
fn glue_binding_batches_reject_occupied_endpoints_and_keep_independent_seams() {
    let mut session = AuthoringSession::new(snapshot());
    let mut draft = session.begin_draft();
    let a = id(4);
    let b = id(6);
    let first = draft
        .batch(draft.source(), |edit| {
            let mut peer = mesh();
            peer.id = b.clone();
            peer.runtime_id = "peer".into();
            edit.create_mesh(peer)?;
            edit.bind_glue_vertices(None, &a, &b, &[(1, 1)])
        })
        .unwrap();
    session
        .publish(draft.prepare("First seam").unwrap())
        .unwrap();

    let mut draft = session.begin_draft();
    let error = draft
        .batch(draft.source(), |edit| {
            edit.bind_glue_vertices(None, &a, &b, &[(2, 2), (1, 3)])
        })
        .unwrap_err();
    assert_eq!(error.code, "GLUE_OCCUPIED");
    assert_eq!(
        session.snapshot().document().glue_order(),
        std::slice::from_ref(&first)
    );
    assert_eq!(session.history_state().undo_steps, 1);

    let mut draft = session.begin_draft();
    let second = draft
        .batch(draft.source(), |edit| {
            edit.bind_glue_vertices(None, &a, &b, &[(2, 2)])
        })
        .unwrap();
    session
        .publish(draft.prepare("Second seam").unwrap())
        .unwrap();
    let model = session.snapshot().document();
    let first_pair = model.glue(&first).unwrap().pairs[0].id.clone();
    let second_pair = model.glue(&second).unwrap().pairs[0].id.clone();

    let mut draft = session.begin_draft();
    let error = draft
        .batch(draft.source(), |edit| {
            edit.unbind_glue_vertices(&first, &[second_pair])
        })
        .unwrap_err();
    assert_eq!(error.code, "MISSING_GLUE_PAIR");
    assert_eq!(session.history_state().undo_steps, 2);

    let mut draft = session.begin_draft();
    draft
        .batch(draft.source(), |edit| {
            edit.unbind_glue_vertices(&first, &[first_pair])
        })
        .unwrap();
    session
        .publish(draft.prepare("Unbind first seam").unwrap())
        .unwrap();
    assert!(
        session
            .snapshot()
            .document()
            .glue(&first)
            .unwrap()
            .pairs
            .is_empty()
    );
    assert_eq!(
        session
            .snapshot()
            .document()
            .glue(&second)
            .unwrap()
            .pairs
            .len(),
        1
    );
}
