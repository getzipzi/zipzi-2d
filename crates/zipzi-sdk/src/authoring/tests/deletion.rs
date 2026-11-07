use super::*;
use zipzi_core::document::{DeletionRequest, DeletionTarget, ObjectEditorState};
fn edit(s: &mut AuthoringSession, run: impl FnOnce(&mut Commands<'_>) -> Result<(), Status>) {
    let mut draft = s.begin_draft();
    draft.batch(draft.source(), run).unwrap();
    s.publish(draft.prepare("delete").unwrap()).unwrap();
}
fn request(root: DeletionTarget) -> DeletionRequest {
    DeletionRequest {
        roots: vec![root],
        ..Default::default()
    }
}
fn binding(c: &mut Commands<'_>) -> Result<(), Status> {
    c.create_mesh_binding_spec(MeshBindingCreateSpec {
        id: Some(id(20)),
        mesh_id: id(4),
        axes: vec![MeshBindingAxisSpec {
            parameter_id: id(2),
            keys: vec![0., 1.],
        }],
        keyforms: [0., 1.]
            .into_iter()
            .map(|v| MeshKeyformSpec {
                keys: vec![v],
                positions: mesh()
                    .base_positions
                    .iter()
                    .map(|p| Vec2::new(p.x + 10. * v, p.y))
                    .collect(),
                opacity: None,
                multiply: None,
                screen: None,
                draw_order: None,
            })
            .collect(),
    })?;
    Ok(())
}
#[test]
fn deleting_parameter_samples_native_grid_and_preserves_history() {
    let mut session = AuthoringSession::new(snapshot());
    edit(&mut session, binding);
    let before = session.snapshot().clone();
    let mut req = request(DeletionTarget::Parameter(id(2)));
    req.parameter_slices.insert(id(2), 0.25);
    let report = before
        .document()
        .analyze_deletion(&req, Default::default(), &Default::default())
        .unwrap();
    assert!(report.decisions.is_empty());
    edit(&mut session, |c| {
        let out = c.edit_deletion(&req, Default::default(), &Default::default())?;
        assert!(out.work.payload_bytes > 0);
        Ok(())
    });
    let state = session.snapshot().document();
    assert!(state.parameter(&id(2)).is_none());
    assert!(state.binding(&id(20)).unwrap().axes.is_empty());
    assert_eq!(
        state.mesh(&id(4)).unwrap().base_positions[0],
        Vec2::new(2.5, 0.)
    );
    assert!(before.document().binding(&id(20)).is_some());
    session.undo(session.version()).unwrap();
    assert_eq!(session.snapshot().content_id(), before.content_id());
    session.redo(session.version()).unwrap();
    assert!(session.snapshot().document().parameter(&id(2)).is_none());
}
#[test]
fn deleting_mesh_cleans_owned_records_and_flags_but_retains_resources() {
    let mut session = AuthoringSession::new(snapshot());
    edit(&mut session, binding);
    let binding = session.snapshot().document().binding(&id(20)).unwrap();
    let sample = binding.axes[0].samples[0].id.clone();
    let mut editor = session.snapshot().document().editor_state().clone();
    editor.objects.insert(
        sample.clone(),
        ObjectEditorState {
            hidden_in_editor: true,
            locked: false,
        },
    );
    edit(&mut session, |c| c.replace_editor_state(editor));
    let req = request(DeletionTarget::Mesh(id(4)));
    edit(&mut session, |c| {
        c.edit_deletion(&req, Default::default(), &Default::default())?;
        Ok(())
    });
    let state = session.snapshot().document();
    assert!(state.mesh(&id(4)).is_none());
    assert!(state.binding(&id(20)).is_none());
    assert!(!state.editor_state().objects.contains_key(&sample));
    assert!(state.asset(&id(5)).is_some());
    assert!(state.part(&id(3)).is_some());
}
#[test]
fn deletion_lock_and_budget_failures_do_not_publish_prior_batch_commands() {
    let mut session = AuthoringSession::new(snapshot());
    let mut editor = session.snapshot().document().editor_state().clone();
    editor.objects.insert(
        id(4),
        ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    edit(&mut session, |c| c.replace_editor_state(editor));
    let before = session.snapshot().clone();
    let req = request(DeletionTarget::Mesh(id(4)));
    let report = before
        .document()
        .analyze_deletion(&req, Default::default(), &Default::default())
        .unwrap();
    assert_eq!(report.blockers.len(), 1);
    let mut draft = session.begin_draft();
    assert_eq!(
        draft
            .batch(draft.source(), |c| {
                c.rename(NameTarget::Part(&id(3)), "pending".into())?;
                c.edit_deletion(&req, Default::default(), &Default::default())?;
                Ok(())
            })
            .unwrap_err()
            .code,
        "OBJECT_LOCKED"
    );
    assert!(draft.prepare("failed").is_err());
    assert_eq!(session.snapshot().content_id(), before.content_id());
    let mut draft = session.begin_draft();
    assert_eq!(
        draft
            .batch(draft.source(), |c| {
                c.edit_deletion(
                    &req,
                    ConversionLimits {
                        max_workspace_bytes: 1,
                        ..Default::default()
                    },
                    &Default::default(),
                )?;
                Ok(())
            })
            .unwrap_err()
            .code,
        "AUTHORING_MEMORY_LIMIT"
    );
}
#[test]
fn deleting_sample_keeps_other_stable_identities() {
    let mut session = AuthoringSession::new(snapshot());
    edit(&mut session, binding);
    let axis = session.snapshot().document().binding(&id(20)).unwrap().axes[0].clone();
    let req = request(DeletionTarget::Record(
        zipzi_core::document::DeletionObject {
            kind: zipzi_core::document::DeletionKind::Sample,
            id: axis.samples[1].id.clone(),
            owner_id: None,
        },
    ));
    edit(&mut session, |c| {
        c.edit_deletion(&req, Default::default(), &Default::default())?;
        Ok(())
    });
    let b = session.snapshot().document().binding(&id(20)).unwrap();
    assert_eq!(b.axes[0].id, axis.id);
    assert_eq!(b.axes[0].samples[0].id, axis.samples[0].id);
    assert_eq!(b.keyforms.len(), 1);
}

#[test]
fn blocked_deletion_review_retains_candidate_and_strict_command_aborts() {
    let mut session = AuthoringSession::new(snapshot());
    edit(&mut session, binding);
    let mut editor = session.snapshot().document().editor_state().clone();
    editor.objects.insert(
        id(2),
        ObjectEditorState {
            locked: true,
            hidden_in_editor: false,
        },
    );
    edit(&mut session, |c| c.replace_editor_state(editor));
    let req = request(DeletionTarget::Parameter(id(2)));
    let mut draft = session.begin_draft();
    let report = draft
        .batch(draft.source(), |c| {
            c.review_deletion(&req, Default::default(), &Default::default())
        })
        .unwrap();
    assert!(!report.report.blockers.is_empty());
    assert_eq!(
        draft.read_snapshot().unwrap().snapshot.content_id(),
        session.snapshot().content_id()
    );
    assert!(draft
        .batch(draft.source(), |c| c.edit_deletion(
            &req,
            Default::default(),
            &Default::default()
        ))
        .is_err());
    assert!(!draft.is_active());
    assert!(session.snapshot().document().parameter(&id(2)).is_some());
}
