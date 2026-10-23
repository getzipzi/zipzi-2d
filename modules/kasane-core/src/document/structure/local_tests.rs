use super::*;
use crate::{Appearance, BindingAxis, MeshKeyform, ModelSetup, Parameter, Vec2};

fn id(n: u32) -> String {
    format!("30000000-0000-4000-8000-{n:012x}")
}

fn fixture() -> Document {
    let mut d = Document::new();
    assert!(d.initialize(id(1), ModelSetup::default()).is_ok());
    for n in [2, 3] {
        assert!(d
            .create_mesh(Mesh {
                id: id(n),
                vertex_ids: vec![1, 2, 3],
                base_positions: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                uvs: vec![Vec2::default(); 3],
                triangles: vec![[1, 2, 3]],
                ..Default::default()
            })
            .status
            .is_ok());
    }
    assert!(d
        .create_parameter(Parameter {
            id: id(4),
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(d
        .create_binding(MeshBinding {
            id: id(5),
            mesh_id: id(2),
            axes: vec![BindingAxis {
                parameter_id: id(4),
                keys: vec![0., 1.],
                ..Default::default()
            }],
            keyforms: [0., 1.]
                .into_iter()
                .map(|key| MeshKeyform {
                    keys: vec![key],
                    positions: d.get_mesh(&id(2)).unwrap().base_positions.clone(),
                    ..Default::default()
                })
                .collect(),
        })
        .status
        .is_ok());
    assert!(d.validate_structure().is_empty());
    d
}

/// Exercise the final validator even for values that public setters reject.
/// This writes only the audited Appearance field and uses the real pre-write
/// token/changed boundary; it never injects a tracker or validity certificate.
fn install_appearance(d: &mut Document, n: u32, appearance: Appearance) {
    let write = d.track_mesh_write(vec![id(n)]);
    d.content.meshes.get_mut(&id(n)).unwrap().appearance = appearance;
    assert!(d
        .changed_tracked(write, ChangeKind::Appearance, vec![id(n)], vec![id(n)])
        .status
        .is_ok());
}

fn differential(before: &Document, after: &Document, local: bool) -> Vec<StructureIssue> {
    // A separate clone drops validation and tracking proofs. Always run the
    // complete oracle before the measured call without validating `after`.
    let expected = after.clone().validate_structure();
    let snapshot = after.snapshot_identity.clone();
    let revision = after.revision();
    #[cfg(feature = "bench-instrumentation")]
    let capture = crate::benchmark::Capture::start();
    let actual = after.validate_structure_since(before);
    assert_eq!(actual, expected);
    assert_eq!(after.structure_is_validated(), expected.is_empty());
    assert_eq!(after.revision(), revision);
    assert!(Arc::ptr_eq(&snapshot, &after.snapshot_identity));
    #[cfg(feature = "bench-instrumentation")]
    {
        let work = capture.finish();
        assert_eq!(
            work.contains_key("core.validate_structure.local"),
            local,
            "{work:?}"
        );
        assert_eq!(
            work.contains_key("core.validate_structure.full"),
            !local,
            "{work:?}"
        );
    }
    #[cfg(not(feature = "bench-instrumentation"))]
    let _ = local;
    actual
}

#[test]
fn local_appearance_matches_full_acceptance_rejection_and_exact_diagnostics() {
    let before = fixture();
    for opacity in [
        0.,
        -0.,
        0.4,
        1.,
        1.00005,
        f32::MAX,
        -0.1,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
    ] {
        let mut after = before.fork_tracked_candidate();
        install_appearance(
            &mut after,
            2,
            Appearance {
                opacity,
                ..Default::default()
            },
        );
        let issues = differential(&before, &after, true);
        assert_eq!(issues.is_empty(), opacity.is_finite() && opacity >= 0.);
        if let Some(issue) = issues.first() {
            assert_eq!(issue.object_id, id(2));
            assert_eq!(issue.status, Status::error("INVALID_OPACITY", id(2)));
        }
    }
    for channel in 0..6 {
        for value in [
            0.,
            0.5,
            1.,
            -0.1,
            1.01,
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
        ] {
            let mut appearance = Appearance::default();
            if channel < 3 {
                appearance.multiply[channel] = value;
            } else {
                appearance.screen[channel - 3] = value;
            }
            let mut after = before.fork_tracked_candidate();
            install_appearance(&mut after, 2, appearance);
            let issues = differential(&before, &after, true);
            assert_eq!(
                issues.is_empty(),
                value.is_finite() && (0. ..=1.).contains(&value)
            );
            if let Some(issue) = issues.first() {
                assert_eq!(issue.status, Status::error("INVALID_COLOR", id(2)));
            }
        }
    }
    // Full validation returns one status per Mesh, in sorted object order.
    let mut after = before.fork_tracked_candidate();
    install_appearance(
        &mut after,
        3,
        Appearance {
            opacity: -1.,
            multiply: [f32::NAN; 3],
            ..Default::default()
        },
    );
    install_appearance(
        &mut after,
        2,
        Appearance {
            screen: [2.; 3],
            ..Default::default()
        },
    );
    let issues = differential(&before, &after, true);
    assert_eq!(
        issues
            .iter()
            .map(|i| (i.object_id.clone(), i.status.code.as_str()))
            .collect::<Vec<_>>(),
        [(id(2), "INVALID_COLOR"), (id(3), "INVALID_OPACITY")]
    );
    assert!(!after.structure_is_validated());
    install_appearance(&mut after, 2, Appearance::default());
    install_appearance(&mut after, 3, Appearance::default());
    assert!(differential(&before, &after, true).is_empty());
}

#[test]
fn labels_enabled_repeated_writes_and_net_noops_reuse_only_exact_valid_baselines() {
    let before = fixture();
    let mut after = before.fork_tracked_candidate();
    assert!(differential(&before, &after, true).is_empty());
    for name in ["", "\0", "日本語 Mesh", "same name"] {
        for n in [2, 3] {
            assert!(after.rename_mesh(&id(n), name.into()).status.is_ok());
            let mut mesh = after.get_mesh(&id(n)).unwrap().clone();
            mesh.enabled = !mesh.enabled;
            assert!(after.replace_mesh(mesh).status.is_ok());
            assert!(differential(&before, &after, true).is_empty());
        }
    }
    for n in [2, 3] {
        assert!(after
            .replace_mesh(before.get_mesh(&id(n)).unwrap().clone())
            .status
            .is_ok());
    }
    assert!(after.same_content(&before));
    assert!(differential(&before, &after, true).is_empty());
    let unvalidated = before.clone();
    let mut after = unvalidated.fork_tracked_candidate();
    assert!(after
        .rename_mesh(&id(2), "First after import".into())
        .status
        .is_ok());
    assert!(differential(&unvalidated, &after, false).is_empty());
    // The fallback established a valid baseline; a later call can use it.
    assert!(differential(&unvalidated, &after, true).is_empty());
    let mut other = before.fork_tracked_candidate();
    assert!(other
        .rename_mesh(&id(2), "Other branch".into())
        .status
        .is_ok());
    assert!(other.validate_structure().is_empty());
    assert_eq!(other.revision(), after.revision());
    assert!(differential(&other, &after, false).is_empty());
}

#[test]
fn invalid_imports_remain_repairable_and_cannot_borrow_mesh_only_proofs() {
    let valid = fixture();
    let mut invalid = valid.clone();
    invalid
        .content
        .meshes
        .get_mut(&id(3))
        .unwrap()
        .appearance
        .opacity = -1.;
    let mut after = invalid.fork_tracked_candidate();
    assert!(after
        .rename_mesh(&id(2), "Unrelated repair".into())
        .status
        .is_ok());
    assert!(!differential(&invalid, &after, false).is_empty());
    assert!(!invalid.structure_is_validated());
    assert!(after.set_mesh_opacity(&id(3), 0.5).status.is_ok());
    assert!(differential(&invalid, &after, false).is_empty());
    assert!(!invalid.structure_is_validated());
    let next = after.fork_tracked_candidate();
    assert!(differential(&after, &next, true).is_empty());
}

#[test]
fn uncovered_geometry_dependencies_binding_payloads_and_shared_tables_fall_back() {
    let before = fixture();
    for field in 0..6 {
        let mut after = before.fork_tracked_candidate();
        // Model an untracked installation through its real invalidating exit.
        match field {
            0 => {
                after
                    .content
                    .meshes
                    .get_mut(&id(2))
                    .unwrap()
                    .vertex_ids
                    .pop()
                    .unwrap();
            }
            1 => after.content.parameters.get_mut(&id(4)).unwrap().maximum = 0.5,
            2 => after.content.meshes.get_mut(&id(2)).unwrap().part_id = id(99),
            3 => after.content.bindings.get_mut(&id(5)).unwrap().axes[0].samples[0].id = id(2),
            4 => after.content.meshes.get_mut(&id(2)).unwrap().draw_order = Some(f32::NAN),
            _ => after.content.blend_key_table_order.push(id(99)),
        }
        after.changed(ChangeKind::Structure, vec![], vec![]);
        assert!(after
            .rename_mesh(&id(3), "Covered after unknown".into())
            .status
            .is_ok());
        assert!(!differential(&before, &after, false).is_empty());
    }
    let mut after = before.fork_tracked_candidate();
    let mut binding = after.get_binding(&id(5)).unwrap().clone();
    binding.keyforms[0].appearance.opacity = 0.3;
    assert!(after.replace_binding(binding).status.is_ok());
    assert!(after.tracked_records_since(&before).is_some());
    assert!(differential(&before, &after, false).is_empty());
    assert!(after
        .rename_mesh(&id(3), "Mixed Mesh and Binding".into())
        .status
        .is_ok());
    assert!(differential(&before, &after, false).is_empty());
}

#[test]
fn validation_rechecks_field_boundaries_even_for_a_tracked_installation() {
    let before = fixture();
    for field in 0..4 {
        let mut after = before.fork_tracked_candidate();
        let write = after.track_mesh_write(vec![id(2)]);
        let mesh = after.content.meshes.get_mut(&id(2)).unwrap();
        match field {
            0 => mesh.base_positions.pop().map(|_| ()).unwrap(),
            1 => mesh.draw_order = Some(f32::NAN),
            2 => mesh.deformer_id = id(99),
            _ => mesh.id = id(99),
        }
        after.changed_tracked(write, ChangeKind::Appearance, vec![id(2)], vec![id(2)]);
        assert!(!differential(&before, &after, false).is_empty());
    }
}

#[test]
fn full_entrypoint_and_checkpoint_clone_restore_boundaries_keep_full_validation() {
    let before = fixture();
    let mut after = before.fork_tracked_candidate();
    assert!(after
        .rename_mesh(&id(2), "Validated locally".into())
        .status
        .is_ok());
    assert!(differential(&before, &after, true).is_empty());
    #[cfg(feature = "bench-instrumentation")]
    let capture = crate::benchmark::Capture::start();
    assert!(after.validate_structure().is_empty());
    #[cfg(feature = "bench-instrumentation")]
    {
        let work = capture.finish();
        assert_eq!(work["core.validate_structure.full"].calls, 1);
        assert_eq!(work["core.validate_structure.full.mesh_checks"].calls, 2);
        assert!(!work.contains_key("core.validate_structure.local"));
    }
    let cloned = after.clone();
    assert!(!cloned.structure_is_validated());
    assert!(differential(&before, &cloned, false).is_empty());
    let mut checkpoint = before.checkpoint();
    after.exchange_checkpoint(&mut checkpoint).unwrap();
    assert!(after.structure_is_validated());
    assert!(differential(&before, &after, false).is_empty());
    after.exchange_checkpoint(&mut checkpoint).unwrap();
    assert!(after.structure_is_validated());
    assert!(differential(&before, &after, false).is_empty());
    let next = after.fork_tracked_candidate();
    assert!(differential(&after, &next, true).is_empty());
    after.restore_from(&before);
    assert!(differential(&before, &after, false).is_empty());
}
