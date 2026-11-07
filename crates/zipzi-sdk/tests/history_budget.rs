use zipzi_core::{state::DocumentState, ModelSetup};
use zipzi_sdk::{AuthoringSession, HistoryLimits, ResourceView, Snapshot};
fn session(max_steps: usize, target_bytes: usize) -> AuthoringSession {
    let mut s = AuthoringSession::new(
        Snapshot::new(
            DocumentState::new(
                "00000000-0000-4000-8000-000000000201",
                ModelSetup::default(),
            )
            .unwrap(),
            ResourceView::default(),
        )
        .unwrap(),
    );
    s.set_history_limits(HistoryLimits {
        max_steps,
        target_bytes,
    });
    s
}
fn add(s: &mut AuthoringSession, n: u32) {
    let mut d = s.begin_draft();
    d.batch(d.source(), |c| {
        c.create_empty_asset(
            format!("00000000-0000-4000-8000-{n:012x}"),
            format!("image-{n}"),
        )
    })
    .unwrap();
    s.publish(d.prepare("image").unwrap()).unwrap();
}
#[test]
fn oversized_step_is_editable_and_keeps_one_undo_step() {
    let mut s = session(50, 0);
    add(&mut s, 202);
    assert!(s.history_state().over_target);
    assert_eq!(s.history_state().undo_steps, 1);
    s.undo(s.version()).unwrap();
    assert!(s.snapshot().document().asset_order().is_empty());
    s.redo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().asset_order().len(), 1);
}
#[test]
fn successful_publish_evicts_oldest_step_and_keeps_contiguous_history() {
    let mut s = session(2, usize::MAX);
    add(&mut s, 202);
    add(&mut s, 203);
    add(&mut s, 204);
    assert_eq!(s.history_state().undo_steps, 2);
    s.undo(s.version()).unwrap();
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.snapshot().document().asset_order(),
        &["00000000-0000-4000-8000-0000000000ca"]
    );
    assert_eq!(s.undo(s.version()).unwrap_err().code, "NO_UNDO");
}
#[test]
fn failed_candidate_does_not_trim_history_or_remove_redo() {
    let mut s = session(2, 0);
    add(&mut s, 202);
    s.undo(s.version()).unwrap();
    let source = s.version();
    let history = s.history_state();
    s.take_events();
    let mut d = s.begin_draft();
    assert!(d
        .batch(d.source(), |c| {
            c.create_empty_asset(
                "00000000-0000-4000-8000-000000000203".into(),
                "candidate".into(),
            )?;
            c.create_empty_asset("invalid".into(), "invalid".into())
        })
        .is_err());
    assert!(!d.is_active());
    assert_eq!(s.version(), source);
    assert_eq!(
        (
            s.history_state().undo_steps,
            s.history_state().redo_steps,
            s.history_state().estimated_bytes
        ),
        (
            history.undo_steps,
            history.redo_steps,
            history.estimated_bytes
        )
    );
    assert!(s.take_events().is_empty());
    s.redo(s.version()).unwrap();
    assert_eq!(s.snapshot().document().asset_order().len(), 1);
}
