use super::*;

#[test]
fn limits_stale_plans_cancel_and_noop_leave_history_unchanged() {
    let mut s = bound();
    s.edit("another parameter", None, |e| {
        e.create_parameter(Parameter {
            id: Q.into(),
            ..Default::default()
        })
    })
    .unwrap();
    s.undo().unwrap();
    let history = s.history_state();
    let noop = E::MeshGrid(G::ReorderAxes {
        binding_id: BINDING_ID.into(),
        parameter_ids: vec![PARAMETER_ID.into()],
    });
    assert!(!apply(&mut s, noop.clone()).changed);
    assert_eq!(s.history_state(), history);
    let version = s.version();
    drop(s.plan_binding_authoring(noop.clone()).unwrap());
    assert_eq!(s.version(), version);
    let old_plan = s.plan_binding_authoring(noop).unwrap();
    let mut limits = AuthoringLimits::default();
    limits.max_sample_combinations = 1;
    s.set_authoring_limits(limits).unwrap();
    assert_eq!(
        s.execute_binding_plan(&old_plan).unwrap_err().code.as_ref(),
        "AUTHORING_GRID_LIMIT"
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    assert_eq!(
        AuthoringLimits::default()
            .check_grid(&[usize::MAX, 2], 1, 0)
            .unwrap_err()
            .code,
        "AUTHORING_CAPACITY_OVERFLOW"
    );
    s.set_authoring_limits(AuthoringLimits::default()).unwrap();
    s.redo().unwrap();
    assert!(s.execute_binding_plan(&old_plan).is_err());
    let p = s
        .plan_binding_authoring(E::MeshGrid(G::InsertSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            value: 0.5,
        }))
        .unwrap();
    let mut edit = s.begin_edit("changed candidate baseline", None).unwrap();
    let mut q = edit.candidate_document().get_parameter(Q).unwrap().clone();
    q.name = "rename".into();
    edit.replace_parameter(q).unwrap();
    assert_eq!(
        edit.apply_binding_plan(&p).unwrap_err().code.as_ref(),
        "STALE_PLAN"
    );
    assert!(edit.commit().is_err());
}

#[test]
fn locked_target_blocks_grid_range_and_direct_replacement() {
    let mut s = bound();
    let mut flags = s.document().editor_state().clone();
    flags.objects.insert(
        MESH_ID.into(),
        zipzi_core::document::ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    s.edit("lock", None, |e| e.replace_editor_state(flags))
        .unwrap();
    let version = s.version();
    assert_eq!(
        s.plan_binding_authoring(E::MeshGrid(G::InsertSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            value: 0.5
        }))
        .unwrap_err()
        .code
        .as_ref(),
        "OBJECT_LOCKED"
    );
    assert_eq!(
        s.plan_binding_authoring(E::ParameterRange(ParameterRangeEdit {
            parameter_id: PARAMETER_ID.into(),
            minimum: 10.0,
            maximum: 20.0,
            default_value: 15.0,
            policy: ParameterRangePolicy::MapKeyValues
        }))
        .unwrap_err()
        .code
        .as_ref(),
        "OBJECT_LOCKED"
    );
    let b = s.binding(BINDING_ID).unwrap();
    assert!(s.edit("bypass", None, |e| e.replace_binding(b)).is_err());
    assert_eq!(s.version(), version);
}

#[test]
fn each_budget_boundary_is_checked_without_allocating_a_grid() {
    let cost = AuthoringLimits::default()
        .check_grid(&[5, 20], 64, 512)
        .unwrap();
    assert_eq!(cost.sample_combinations, 100);
    assert_eq!(cost.payload_bytes, 6400);
    let exact = AuthoringLimits {
        max_sample_combinations: 100,
        max_conversion_bytes: cost.temporary_bytes,
        max_history_bytes: cost.history_bytes,
    };
    assert_eq!(exact.check_grid(&[5, 20], 64, 512).unwrap(), cost);
    assert_eq!(
        AuthoringLimits {
            max_sample_combinations: 99,
            ..exact
        }
        .check_grid(&[5, 20], 64, 512)
        .unwrap_err()
        .code,
        "AUTHORING_GRID_LIMIT"
    );
    assert_eq!(
        AuthoringLimits {
            max_conversion_bytes: cost.temporary_bytes - 1,
            ..exact
        }
        .check_grid(&[5, 20], 64, 512)
        .unwrap_err()
        .code,
        "AUTHORING_MEMORY_LIMIT"
    );
    assert_eq!(
        AuthoringLimits {
            max_history_bytes: cost.history_bytes - 1,
            ..exact
        }
        .check_grid(&[5, 20], 64, 512)
        .unwrap_err()
        .code,
        "AUTHORING_HISTORY_LIMIT"
    );
    assert_eq!(
        exact.check_grid(&[0], 64, 512).unwrap_err().code,
        "INVALID_KEYS"
    );
}

#[test]
fn cooperative_cancel_during_grid_construction_and_before_commit_preserves_history() {
    use zipzi_sdk::ConversionControl;
    let mut s = bound();
    s.edit("redo sentinel", None, |e| {
        e.create_parameter(Parameter {
            id: Q.into(),
            ..Default::default()
        })
    })
    .unwrap();
    s.undo().unwrap();
    s.drain_events();
    let version = s.version();
    let history = s.history_state();
    let intent = E::MeshGrid(G::InsertSample {
        binding_id: BINDING_ID.into(),
        parameter_id: PARAMETER_ID.into(),
        value: 0.5,
    });
    // Cancels after entering lattice construction rather than at initial preflight.
    let control = ConversionControl::with_progress(|n| n < 4);
    assert_eq!(
        s.plan_binding_authoring_with_control(intent.clone(), &control)
            .unwrap_err()
            .code
            .as_ref(),
        "AUTHORING_CANCELLED"
    );
    assert_eq!(control.checkpoints(), 4);
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    assert!(s.drain_events().is_empty());
    let control = ConversionControl::default();
    let p = s
        .plan_binding_authoring_with_control(intent.clone(), &control)
        .unwrap();
    control.cancel();
    assert_eq!(
        s.execute_binding_plan(&p).unwrap_err().code.as_ref(),
        "AUTHORING_CANCELLED"
    );
    let control = ConversionControl::default();
    let p = s
        .plan_binding_authoring_with_control(intent, &control)
        .unwrap();
    let mut candidate = s.begin_owned_edit("cancel at commit", None).unwrap();
    candidate.apply_binding_plan(&p).unwrap();
    control.cancel();
    assert_eq!(
        candidate.commit_to(&mut s).unwrap_err().code.as_ref(),
        "AUTHORING_CANCELLED"
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    assert!(s.drain_events().is_empty());
    let control = ConversionControl::default();
    let mut candidate = s.begin_owned_edit("composed candidate", None).unwrap();
    candidate
        .create_parameter(Parameter {
            id: Q.into(),
            ..Default::default()
        })
        .unwrap();
    candidate
        .binding_authoring_with_control(
            E::MeshGrid(G::InsertSample {
                binding_id: BINDING_ID.into(),
                parameter_id: PARAMETER_ID.into(),
                value: 0.5,
            }),
            &control,
        )
        .unwrap();
    control.cancel();
    assert_eq!(
        candidate.commit_to(&mut s).unwrap_err().code.as_ref(),
        "AUTHORING_CANCELLED"
    );
    assert!(s.parameter(Q).is_none());
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
    s.redo().unwrap();
    assert!(s.parameter(Q).is_some());
}

#[test]
fn grid_report_names_stable_addresses_and_reconstruction_weights() {
    use zipzi_core::document::ConversionElement as A;
    let mut s = bound();
    let old = s.document().get_binding(BINDING_ID).unwrap().clone();
    let p = s
        .plan_binding_authoring(E::MeshGrid(G::InsertSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            value: 0.5,
        }))
        .unwrap();
    let report = p.report();
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
    s.execute_binding_plan(&p).unwrap();
    let moved = s
        .plan_binding_authoring(E::MeshGrid(G::MoveSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            from: 0.5,
            to: 0.6,
        }))
        .unwrap();
    assert!(moved.report().generated.is_empty());
    assert!(moved.report().deleted.is_empty());
    assert!(moved
        .report()
        .remapped
        .iter()
        .any(|r| r.before.value == Some(0.5) && r.after.value == Some(0.6)));
    assert!(!moved.report().losses.is_empty());
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
        let result = s.plan_binding_authoring_with_control(
            E::MeshGrid(G::InsertSample {
                binding_id: BINDING_ID.into(),
                parameter_id: PARAMETER_ID.into(),
                value: 0.5,
            }),
            &worker_control,
        );
        assert_eq!(result.unwrap_err().code.as_ref(), "AUTHORING_CANCELLED");
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
    let mut s = bound();
    let baseline = s.document().estimated_content_bytes();
    s.set_authoring_limits(AuthoringLimits {
        max_conversion_bytes: baseline * 6 + 8_000,
        ..Default::default()
    })
    .unwrap();
    let control = zipzi_sdk::ConversionControl::default();
    let version = s.version();
    let history = s.history_state();
    let error = s
        .plan_binding_authoring_with_control(
            E::MeshGrid(G::InsertSample {
                binding_id: BINDING_ID.into(),
                parameter_id: PARAMETER_ID.into(),
                value: 0.5,
            }),
            &control,
        )
        .unwrap_err();
    assert_eq!(error.code.as_ref(), "AUTHORING_MEMORY_LIMIT");
    assert_eq!(
        control.checkpoints(),
        2,
        "only Mesh/Grid entry checkpoints, before lattice expansion"
    );
    assert_eq!(s.version(), version);
    assert_eq!(s.history_state(), history);
}
