use super::*;
use zipzi_sdk::{DeletionIntent, DeletionReviewState, DeletionRow, DeletionSection};
fn prepare(s: &AuthoringSession) -> zipzi_sdk::DeletionReviewPlan {
    s.read_snapshot()
        .prepare_deletion_review(
            DeletionIntent {
                request: request(&[10]),
                ..Default::default()
            },
            s.authoring_limits(),
            &Default::default(),
        )
        .unwrap()
}
#[test]
fn report_pages_separate_owned_identities_and_keep_the_frozen_source() {
    let mut s = fixture();
    let before = s.version();
    let plan = prepare(&s);
    assert_eq!(plan.summary().state, DeletionReviewState::NeedsConfirmation);
    let first = plan.report_page(DeletionSection::Removed, 0, 1).unwrap();
    assert_eq!(first.rows.len(), 1);
    assert!(first.next_offset.is_some());
    let owned = plan.report_page(DeletionSection::Owned, 0, 256).unwrap();
    assert!(owned.rows.iter().all(
        |r| matches!(r, DeletionRow::Removal { removal } if removal.reason == DeletionReason::Owned)
    ));
    assert!(owned.rows.iter().any(|r| matches!(r, DeletionRow::Removal { removal } if removal.object.kind == DeletionKind::Sample)));
    assert_eq!(s.version(), before);
    assert!(plan.report_page(DeletionSection::Removed, 0, 257).is_err());
    assert!(plan
        .report_page(DeletionSection::Removed, first.total + 1, 1)
        .is_err());
    s.edit("rename", None, |e| e.rename_mesh(&id(11), "changed"))
        .unwrap();
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(plan.report_page(DeletionSection::Removed, 0, 1).unwrap()).unwrap()
    );
    assert!(s.edit("stale", None, |e| plan.apply(e)).is_err());
    assert!(s.document().get_mesh(&id(10)).is_some());
}
#[test]
fn blocked_review_retains_typed_lock_rows_and_cannot_execute() {
    let mut s = fixture();
    lock(&mut s, &id(11));
    let before = s.document().clone();
    let history = s.history_lengths();
    let plan = prepare(&s);
    assert_eq!(plan.summary().state, DeletionReviewState::Blocked);
    assert!(!plan.summary().export_analysis_complete);
    let locks = plan.report_page(DeletionSection::Locks, 0, 64).unwrap();
    assert!(locks.rows.iter().any(|r| matches!(r, DeletionRow::Lock { blocker } if blocker.object.id == id(11) && blocker.source_id == id(11))));
    assert_eq!(
        s.edit("blocked", None, |e| plan.apply(e))
            .unwrap_err()
            .code
            .as_ref(),
        "DELETION_NOT_READY"
    );
    assert!(s.document().same_content(&before));
    assert_eq!(s.history_lengths(), history);
}
#[test]
fn overlapping_part_roots_record_linear_scope_edges_without_losing_explicit_roots() {
    let mut s = fixture();
    s.edit("hierarchy", None, |e| {
        for n in 100..164 {
            e.create_part(Part {
                id: id(n),
                parent_id: if n == 100 { String::new() } else { id(n - 1) },
                ..Default::default()
            })?;
        }
        Ok(())
    })
    .unwrap();
    let roots: Vec<_> = (100..164).map(|n| DeletionTarget::Part(id(n))).collect();
    let first = s
        .document()
        .analyze_deletion(
            &DeletionRequest {
                roots: roots.clone(),
                ..Default::default()
            },
            &Default::default(),
        )
        .unwrap();
    assert_eq!(first.requested.len(), 64);
    assert_eq!(
        first
            .relations
            .iter()
            .filter(|r| r.field == DeletionField::RequestedScope)
            .count(),
        63
    );
    let second = s
        .document()
        .analyze_deletion(
            &DeletionRequest {
                roots: roots.into_iter().rev().collect(),
                ..Default::default()
            },
            &Default::default(),
        )
        .unwrap();
    assert_eq!(first, second);
}

#[test]
fn review_pages_require_exact_export_acceptance_without_changing_provenance() {
    let mut s = fixture();
    s.edit("opaque source", None, |e| {
        let mut settings = e.candidate_document().model3_settings().clone();
        settings
            .extensions
            .insert("Vendor".into(), serde_json::json!({"Mesh":"external"}));
        settings.source_runtime_ids = Some(zipzi_project::model3::runtime_namespace(
            e.candidate_document(),
        ));
        e.set_model3_settings(settings)
    })
    .unwrap();
    let source = s.document().model3_settings().clone();
    let plan = prepare(&s);
    assert_eq!(plan.summary().state, DeletionReviewState::NeedsChoice);
    let exports = plan.report_page(DeletionSection::Exports, 0, 64).unwrap();
    assert!(matches!(
        exports.rows.as_slice(),
        [DeletionRow::Export {
            failure: DeletionExportFailure::Model3NamespaceChanged,
            existing: false
        }]
    ));
    let accepted = s
        .read_snapshot()
        .prepare_deletion_review(
            DeletionIntent {
                request: request(&[10]),
                accepted_export_failures: vec![DeletionExportFailure::Model3NamespaceChanged],
            },
            s.authoring_limits(),
            &Default::default(),
        )
        .unwrap();
    assert_eq!(
        accepted.summary().state,
        DeletionReviewState::NeedsConfirmation
    );
    s.edit("accepted", None, |e| accepted.apply(e)).unwrap();
    assert_eq!(
        s.document().model3_settings().source_runtime_ids,
        source.source_runtime_ids
    );
    assert_eq!(s.document().model3_settings().extensions, source.extensions);
    s.undo().unwrap();
    assert_eq!(s.document().model3_settings(), &source);
}

#[test]
fn accepting_export_loss_still_requires_deletion_confirmation() {
    let mut s = AuthoringSession::new(&id(1), Canvas::default()).unwrap();
    s.edit("unused parameter and opaque settings", None, |e| {
        e.create_parameter(Parameter {
            id: id(270),
            ..Default::default()
        })?;
        let mut settings = e.candidate_document().model3_settings().clone();
        settings
            .extensions
            .insert("Vendor".into(), serde_json::json!(true));
        settings.source_runtime_ids = Some(zipzi_project::model3::runtime_namespace(
            e.candidate_document(),
        ));
        e.set_model3_settings(settings)
    })
    .unwrap();
    let before = s.document().clone();
    let request = DeletionRequest {
        roots: vec![DeletionTarget::Parameter(id(270))],
        ..Default::default()
    };
    let prepare = |accepted_export_failures| {
        s.read_snapshot()
            .prepare_deletion_review(
                DeletionIntent {
                    request: request.clone(),
                    accepted_export_failures,
                },
                s.authoring_limits(),
                &Default::default(),
            )
            .unwrap()
    };
    let initial = prepare(vec![]);
    assert!(!initial.analysis().native.requires_confirmation());
    assert_eq!(initial.summary().state, DeletionReviewState::NeedsChoice);
    let accepted = prepare(vec![DeletionExportFailure::Model3NamespaceChanged]);
    assert_eq!(
        accepted.summary().state,
        DeletionReviewState::NeedsConfirmation
    );
    assert!(s.document().same_content(&before));
    s.edit("confirm deletion", None, |e| accepted.apply(e))
        .unwrap();
    assert!(s.document().get_parameter(&id(270)).is_none());
    s.undo().unwrap();
    assert!(s.document().same_content(&before));
}

#[test]
fn parameter_review_folds_dependent_axes_but_keeps_business_removals_visible() {
    let s = fixture();
    let plan = s
        .read_snapshot()
        .prepare_deletion_review(
            DeletionIntent {
                request: DeletionRequest {
                    roots: vec![DeletionTarget::Parameter(id(3))],
                    ..Default::default()
                },
                ..Default::default()
            },
            s.authoring_limits(),
            &Default::default(),
        )
        .unwrap();
    let removed = plan.report_page(DeletionSection::Removed, 0, 256).unwrap();
    assert_eq!(removed.total, 1);
    assert!(
        matches!(&removed.rows[0],DeletionRow::Removal {removal} if removal.object.kind==DeletionKind::Parameter)
    );
    let internal = plan.report_page(DeletionSection::Owned, 0, 256).unwrap();
    assert!(internal.rows.iter().any(|row|matches!(row,DeletionRow::Removal {removal} if removal.object.kind==DeletionKind::Axis && removal.reason==DeletionReason::Dependency)));
}
