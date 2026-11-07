use super::*;
#[test]
fn deletion_report_keeps_owned_identities_and_its_frozen_source() {
    let mut s = fixture();
    let before = s.version();
    let (outcome, prepared) = crate::support::prepared(&s, |c| {
        c.review_deletion(&request(&[10]), Default::default(), &Default::default())
    })
    .unwrap();
    assert!(outcome.report.requires_confirmation());
    assert!(outcome
        .report
        .removed
        .iter()
        .any(|r| r.reason == DeletionReason::Owned && r.object.kind == DeletionKind::Sample));
    let frozen = serde_json::to_value(&outcome.report).unwrap();
    assert_eq!(s.version(), before);
    let (_, rename) = crate::support::prepared(&s, |c| {
        c.rename(zipzi_sdk::NameTarget::Mesh(&id(11)), "changed".into())
    })
    .unwrap();
    s.publish(rename).unwrap();
    assert_eq!(serde_json::to_value(&outcome.report).unwrap(), frozen);
    assert_eq!(
        s.publish(prepared).unwrap_err().code.as_str(),
        "STALE_SOURCE"
    );
    assert!(s.snapshot().document().mesh(&id(10)).is_some());
}
#[test]
fn blocked_review_preserves_the_candidate_and_retains_typed_lock_rows() {
    let mut s = fixture();
    lock(&mut s, &id(11));
    let source = s.read_snapshot();
    let history = s.history_state();
    let (outcome, prepared) = crate::support::prepared(&s, |c| {
        c.review_deletion(&request(&[10]), Default::default(), &Default::default())
    })
    .unwrap();
    assert!(outcome
        .report
        .blockers
        .iter()
        .any(|b| b.object.id == id(11) && b.source_id == id(11)));
    assert!(prepared.writes().is_empty());
    assert_eq!(
        prepared.snapshot().content_id(),
        source.snapshot.content_id()
    );
    assert_eq!(
        prepare_deletion(&s, &request(&[10]))
            .unwrap_err()
            .code
            .as_str(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.read_snapshot().source, source.source);
    assert_eq!(s.history_state(), history);
}
#[test]
fn overlapping_part_roots_record_linear_scope_edges_without_losing_explicit_roots() {
    let mut s = fixture();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                for n in 100..164 {
                    e.create_part({
                        let mut value = Part {
                            id: id(n),
                            parent_id: if n == 100 { String::new() } else { id(n - 1) },
                            ..Default::default()
                        };
                        value.runtime_id = value.id.clone();
                        value
                    })?;
                }
                Ok(())
            })
            .and_then(|value| {
                let prepared = draft.prepare("hierarchy")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let roots: Vec<_> = (100..164).map(|n| DeletionTarget::Part(id(n))).collect();
    let first = s
        .snapshot()
        .document()
        .analyze_deletion(
            &DeletionRequest {
                roots: roots.clone(),
                ..Default::default()
            },
            Default::default(),
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
        .snapshot()
        .document()
        .analyze_deletion(
            &DeletionRequest {
                roots: roots.into_iter().rev().collect(),
                ..Default::default()
            },
            Default::default(),
            &Default::default(),
        )
        .unwrap();
    assert_eq!(first, second);
}

#[test]
fn export_review_compares_candidate_and_source_without_rewriting_provenance() {
    let mut s = fixture();
    let (_, p) = crate::support::prepared(&s, |c| {
        let d = c.read_document()?;
        let mut settings = d.model3_settings().clone();
        settings.user_data = Some(AttachmentRef::Unresolved {
            path: "userdata.json".into(),
        });
        settings.source_runtime_ids = Some(zipzi_project::model3::runtime_namespace(&d));
        c.replace_model3_settings(settings)
    })
    .unwrap();
    s.publish(p).unwrap();
    let before = s.read_snapshot();
    let analysis = analyze(&s, &request(&[10])).unwrap();
    assert!(analysis.0.requires_confirmation());
    assert!(analysis
        .1
        .contains(&zipzi_project::OpaqueExportIssue::Model3NamespaceChanged));
    let (_, p) = prepare_deletion(&s, &request(&[10])).unwrap();
    assert_eq!(
        p.snapshot().document().model3_settings().source_runtime_ids,
        before
            .snapshot
            .document()
            .model3_settings()
            .source_runtime_ids
    );
    assert_eq!(s.read_snapshot().source, before.source);
    s.publish(p).unwrap();
    assert_eq!(
        zipzi_project::model3::export_settings(s.snapshot().document())
            .unwrap_err()
            .code,
        "MODEL3_NAMESPACE_CHANGED"
    );
}
#[test]
fn parameter_review_retains_business_and_dependent_axis_removals() {
    let s = fixture();
    let (outcome, _) = crate::support::prepared(&s, |c| {
        c.review_deletion(
            &DeletionRequest {
                roots: vec![DeletionTarget::Parameter(id(3))],
                ..Default::default()
            },
            Default::default(),
            &Default::default(),
        )
    })
    .unwrap();
    assert!(outcome
        .report
        .removed
        .iter()
        .any(|r| r.object.kind == DeletionKind::Parameter));
    assert!(outcome
        .report
        .removed
        .iter()
        .any(|r| r.object.kind == DeletionKind::Axis && r.reason == DeletionReason::Dependency));
}
