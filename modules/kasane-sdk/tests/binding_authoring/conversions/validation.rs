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
        kasane_core::document::ObjectEditorState {
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
