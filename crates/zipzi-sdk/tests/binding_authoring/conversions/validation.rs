use super::*;

#[test]
fn limits_stale_plans_cancel_and_noop_leave_history_unchanged() {
    let mut s = bound();
    let (_, p) = crate::support::prepared(&s, |c| {
        c.create_parameter(Parameter {
            id: Q.into(),
            runtime_id: Q.into(),
            ..Default::default()
        })
    })
    .unwrap();
    s.publish(p).unwrap();
    s.undo(s.version()).unwrap();
    let history = s.history_state();
    let version = s.version();
    let noop = E::Grid(zipzi_sdk::BindingGridEdit::Edit {
        owner: BINDING_ID.into(),
        operation: zipzi_sdk::GridOperation::ReorderAxes {
            axis_ids: vec![crate::support::axis(&s, BINDING_ID, PARAMETER_ID)],
        },
    });
    assert!(apply(&mut s, noop.clone()).writes.is_empty());
    assert_eq!(s.history_state(), history);
    let (_, old) = crate::support::binding(&s, noop).unwrap();
    s.redo(s.version()).unwrap();
    assert_eq!(s.publish(old).unwrap_err().code, "STALE_SOURCE");
    assert_ne!(s.version(), version);
    let mut draft = s.begin_draft();
    let source = draft.source();
    draft
        .batch(source, |c| {
            c.rename(zipzi_sdk::NameTarget::Parameter(Q), "rename".into())
        })
        .unwrap();
    assert_eq!(
        draft
            .batch(source, |c| c
                .rename(zipzi_sdk::NameTarget::Parameter(Q), "stale".into()))
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    assert!(draft.prepare("stale sequence").is_err());
}

#[test]
fn locked_target_blocks_grid_range_and_direct_replacement() {
    let mut s = bound();
    let mut flags = s.snapshot().document().editor_state().clone();
    flags.objects.insert(
        MESH_ID.into(),
        zipzi_core::document::ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_editor_state(flags))
            .and_then(|value| {
                let prepared = draft.prepare("lock")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let version = s.version();
    assert_eq!(
        crate::support::binding(
            &s,
            E::Grid(zipzi_sdk::BindingGridEdit::Edit {
                owner: BINDING_ID.into(),
                operation: zipzi_sdk::GridOperation::InsertSample {
                    axis_id: crate::support::axis(&s, BINDING_ID, PARAMETER_ID),
                    id: uuid::Uuid::new_v4().to_string(),
                    value: 0.5
                }
            })
        )
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    assert_eq!(
        crate::support::binding(
            &s,
            E::ParameterRange(ParameterRangeEdit {
                parameter_id: PARAMETER_ID.into(),
                minimum: 10.0,
                maximum: 20.0,
                default_value: 15.0,
                policy: ParameterRangePolicy::MapKeyValues
            })
        )
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    let b = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap();
    assert!({
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_binding(b))
            .and_then(|value| {
                let prepared = draft.prepare("bypass")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .is_err());
    assert_eq!(s.version(), version);
}

#[test]
fn each_budget_boundary_is_checked_without_allocating_a_grid() {
    let cost = ConversionLimits::default()
        .check_grid(&[5, 20], 64)
        .unwrap();
    assert_eq!(cost.sample_combinations, 100);
    assert_eq!(cost.payload_bytes, 6400);
    let exact = ConversionLimits {
        max_sample_combinations: 100,
        max_workspace_bytes: cost.workspace_bytes,
    };
    assert_eq!(exact.check_grid(&[5, 20], 64).unwrap(), cost);
    assert_eq!(
        ConversionLimits {
            max_sample_combinations: 99,
            ..exact
        }
        .check_grid(&[5, 20], 64)
        .unwrap_err()
        .code,
        "AUTHORING_GRID_LIMIT"
    );
    assert_eq!(
        ConversionLimits {
            max_workspace_bytes: cost.workspace_bytes - 1,
            ..exact
        }
        .check_grid(&[5, 20], 64)
        .unwrap_err()
        .code,
        "AUTHORING_MEMORY_LIMIT"
    );
    assert_eq!(
        exact.check_grid(&[usize::MAX, 2], 1).unwrap_err().code,
        "AUTHORING_CAPACITY_OVERFLOW"
    );
    assert_eq!(
        exact.check_grid(&[1], usize::MAX).unwrap_err().code,
        "AUTHORING_CAPACITY_OVERFLOW"
    );
}

#[test]
fn cooperative_cancel_during_grid_construction_and_before_commit_preserves_history() {
    let mut s = bound();
    let (_, p) = crate::support::prepared(&s, |c| {
        c.create_parameter(Parameter {
            id: Q.into(),
            runtime_id: Q.into(),
            ..Default::default()
        })
    })
    .unwrap();
    s.publish(p).unwrap();
    s.undo(s.version()).unwrap();
    s.take_events();
    let version = s.version();
    let history = s.history_state();
    let intent = E::Grid(zipzi_sdk::BindingGridEdit::Edit {
        owner: BINDING_ID.into(),
        operation: zipzi_sdk::GridOperation::InsertSample {
            axis_id: crate::support::axis(&s, BINDING_ID, PARAMETER_ID),
            id: uuid::Uuid::new_v4().to_string(),
            value: 0.5,
        },
    });
    let control = zipzi_sdk::ConversionControl::with_progress(|n| n < 4);
    assert_eq!(
        crate::support::binding_with_control(&s, intent.clone(), Default::default(), &control)
            .unwrap_err()
            .code,
        "AUTHORING_CANCELLED"
    );
    assert_eq!(control.checkpoints(), 4);
    let (_, p) = crate::support::binding(&s, intent).unwrap();
    let control = zipzi_sdk::ConversionControl::default();
    control.cancel();
    assert!(s
        .publish_with_control(p, || control.check().is_ok())
        .is_err());
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    assert!(s.take_events().is_empty());
    s.redo(s.version()).unwrap();
    assert!(s.snapshot().document().parameter(Q).is_some());
}

#[test]
fn grid_report_names_stable_addresses_and_reconstruction_weights() {
    use zipzi_core::document::ConversionElement as A;
    let mut s = bound();
    let old = s.snapshot().document().binding(BINDING_ID).unwrap().clone();
    let p = crate::support::binding(
        &s,
        E::Grid(zipzi_sdk::BindingGridEdit::Edit {
            owner: BINDING_ID.into(),
            operation: zipzi_sdk::GridOperation::InsertSample {
                axis_id: crate::support::axis(&s, BINDING_ID, PARAMETER_ID),
                id: uuid::Uuid::new_v4().to_string(),
                value: 0.5,
            },
        }),
    )
    .unwrap();
    let report = p.0.report;
    assert_eq!(report.domain[0].interval.minimum, 0.);
    assert_eq!(report.domain[0].interval.maximum, 1.);
    assert!(report.deleted.is_empty());
    assert_eq!(
        report
            .generated
            .iter()
            .filter(|a| matches!(a.element, A::Sample(_)))
            .count(),
        1
    );
    let reconstructed = report
        .reconstructed
        .iter()
        .find(|r| r.sources.len() == 2)
        .unwrap();
    assert_eq!(
        reconstructed
            .sources
            .iter()
            .map(|(_, w)| w)
            .copied()
            .sum::<f32>(),
        1.
    );
    assert!(reconstructed.sources.iter().all(|(a,_)|matches!(&a.element,A::Keyform(samples) if old.keyforms.iter().any(|f|f.samples==*samples))));
    assert!(report.grid_guarantee.is_some());
    s.publish(p.1).unwrap();
    let moved = crate::support::binding(
        &s,
        E::Grid(zipzi_sdk::BindingGridEdit::Edit {
            owner: BINDING_ID.into(),
            operation: zipzi_sdk::GridOperation::MoveSample {
                sample_id: crate::support::sample(&s, BINDING_ID, PARAMETER_ID, 0.5),
                value: 0.6,
            },
        }),
    )
    .unwrap();
    assert!(moved.0.report.generated.is_empty());
    assert!(moved.0.report.deleted.is_empty());
    assert!(moved
        .0
        .report
        .remapped
        .iter()
        .any(|r| r.before.value == Some(0.5) && r.after.value == Some(0.6)));
    assert!(!moved.0.report.losses.is_empty());
}

#[test]
fn cancellation_signal_can_interrupt_a_separate_planning_worker() {
    use std::sync::{Arc, Barrier};
    use zipzi_sdk::ConversionControl;
    let entered = Arc::new(Barrier::new(2));
    let resume = Arc::new(Barrier::new(2));
    let (worker_entered, worker_resume) = (entered.clone(), resume.clone());
    let control = ConversionControl::with_progress(move |n| {
        if n == 2 {
            worker_entered.wait();
            worker_resume.wait();
        }
        true
    });
    let worker_control = control.clone();
    let thread = std::thread::spawn(move || {
        let s = bound();
        let version = s.version();
        let result = crate::support::binding_with_control(
            &s,
            E::Grid(zipzi_sdk::BindingGridEdit::Edit {
                owner: BINDING_ID.into(),
                operation: zipzi_sdk::GridOperation::InsertSample {
                    axis_id: crate::support::axis(&s, BINDING_ID, PARAMETER_ID),
                    id: uuid::Uuid::new_v4().to_string(),
                    value: 0.5,
                },
            }),
            Default::default(),
            &worker_control,
        );
        assert_eq!(result.unwrap_err().code.as_str(), "AUTHORING_CANCELLED");
        assert_eq!(s.version(), version);
    });
    entered.wait();
    control.cancel();
    resume.wait();
    thread.join().unwrap();
    assert_eq!(control.checkpoints(), 2);
}

#[test]
fn report_budget_rejection_precedes_cartesian_construction() {
    let s = bound();
    let control = zipzi_sdk::ConversionControl::default();
    let version = s.version();
    let history = s.history_state();
    let edit = E::Grid(zipzi_sdk::BindingGridEdit::Edit {
        owner: BINDING_ID.into(),
        operation: zipzi_sdk::GridOperation::InsertSample {
            axis_id: crate::support::axis(&s, BINDING_ID, PARAMETER_ID),
            id: uuid::Uuid::new_v4().to_string(),
            value: 0.5,
        },
    });
    assert_eq!(
        crate::support::binding_with_control(
            &s,
            edit,
            ConversionLimits {
                max_workspace_bytes: 1,
                ..Default::default()
            },
            &control
        )
        .unwrap_err()
        .code,
        "AUTHORING_MEMORY_LIMIT"
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
}
