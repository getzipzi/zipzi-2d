use super::*;

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
    assert!(session
        .snapshot()
        .document()
        .glue(&first)
        .unwrap()
        .pairs
        .is_empty());
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
