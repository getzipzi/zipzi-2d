use super::*;

#[test]
fn zero_axis_uses_latest_base_and_last_axis_writes_selected_shape_back() {
    let mut s = fixture();
    apply(
        &mut s,
        E::MeshGrid(G::Create {
            id: BINDING_ID.into(),
            mesh_id: MESH_ID.into(),
        }),
    );
    let b = s.binding(BINDING_ID).unwrap();
    assert!(b.axes.is_empty() && b.keyforms.is_empty());
    let mut m = s.document().get_mesh(MESH_ID).unwrap().clone();
    m.base_positions.iter_mut().for_each(|p| p.x += 3.0);
    m.appearance.opacity = 0.6;
    s.edit("base", None, |e| {
        e.replace_mesh(m)?;
        e.create_parameter(parameter(false))
    })
    .unwrap();
    assert_eq!(frame(&s, 0.0, 0.0).drawables[0].positions[0].x, 43.0);
    grid_roundtrip(&s);
    apply(
        &mut s,
        E::MeshGrid(G::AddAxis {
            binding_id: BINDING_ID.into(),
            axis: BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: PARAMETER_ID.into(),
                keys: vec![0.0, 1.0],
            },
        }),
    );
    assert!(s
        .binding(BINDING_ID)
        .unwrap()
        .keyforms
        .iter()
        .all(|f| f.positions[0].x == 43.0 && f.appearance.opacity == 0.6));
    let mut f = s.binding(BINDING_ID).unwrap().keyforms[1].clone();
    f.positions.iter_mut().for_each(|p| p.y += 7.0);
    s.edit("shape", None, |e| e.set_mesh_keyform(BINDING_ID, f))
        .unwrap();
    let expected = frame(&s, 0.25, 0.0).drawables[0].clone();
    apply(
        &mut s,
        E::MeshGrid(G::RemoveAxis {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            reduction: AxisReduction::Resample(0.25),
        }),
    );
    let actual = frame(&s, 0.0, 0.0).drawables[0].clone();
    assert_eq!(actual.positions, expected.positions);
    assert_eq!(actual.opacity, expected.opacity);
    assert!(s.binding(BINDING_ID).unwrap().keyforms.is_empty());
    grid_roundtrip(&s);
    s.undo().unwrap();
    assert_eq!(s.binding(BINDING_ID).unwrap().keyforms.len(), 2);
    s.redo().unwrap();
    assert_eq!(
        frame(&s, 1.0, 0.0).drawables[0].positions,
        expected.positions
    );
}

#[test]
fn expand_three_distinct_shapes_and_reorder_preserve_old_domain() {
    let mut s = bound();
    apply(
        &mut s,
        E::MeshGrid(G::InsertSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            value: 0.5,
        }),
    );
    let mut f = s.binding(BINDING_ID).unwrap().keyforms[1].clone();
    f.positions.iter_mut().for_each(|p| p.y += 8.0);
    f.appearance.opacity = 0.25;
    f.draw_order = Some(8.0);
    s.edit("middle shape", None, |e| e.set_mesh_keyform(BINDING_ID, f))
        .unwrap();
    s.edit("new driver", None, |e| {
        e.create_parameter(Parameter {
            id: Q.into(),
            ..Parameter::default()
        })
    })
    .unwrap();
    let before: Vec<_> = (0..=20)
        .map(|i| frame(&s, i as f32 / 20., 0.).drawables[0].clone())
        .collect();
    let old_forms = s.binding(BINDING_ID).unwrap().keyforms;
    apply(
        &mut s,
        E::MeshGrid(G::AddAxis {
            binding_id: BINDING_ID.into(),
            axis: BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: Q.into(),
                keys: vec![-1.0, 0.0, 1.0],
            },
        }),
    );
    let b = s.binding(BINDING_ID).unwrap();
    assert_eq!(b.keyforms.len(), 9);
    for chunk in b.keyforms.chunks(3) {
        for (f, old) in chunk.iter().zip(&old_forms) {
            assert_eq!(f.positions, old.positions);
            assert_eq!(f.appearance, old.appearance);
            assert_eq!(f.draw_order, old.draw_order);
        }
    }
    apply(
        &mut s,
        E::MeshGrid(G::ReorderAxes {
            binding_id: BINDING_ID.into(),
            parameter_ids: vec![Q.into(), PARAMETER_ID.into()],
        }),
    );
    for (i, expected) in before.iter().enumerate() {
        for q in [-1.0, -0.65, 0.0, 0.23, 1.0] {
            let actual = &frame(&s, i as f32 / 20., q).drawables[0];
            for (a, b) in actual.positions.iter().zip(&expected.positions) {
                assert!((a.x - b.x).abs() < 2e-5 && (a.y - b.y).abs() < 2e-5);
            }
            assert!((actual.opacity - expected.opacity).abs() < 1e-6);
            assert_eq!(actual.draw_order, expected.draw_order);
        }
    }
    grid_roundtrip(&s);
}

#[test]
fn insert_move_delete_and_slice_are_explicit_and_atomic() {
    let mut s = bound();
    let expected: Vec<_> = (0..=10).map(|i| sample(&s, i as f32 / 10.)).collect();
    apply(
        &mut s,
        E::MeshGrid(G::InsertSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            value: 0.4,
        }),
    );
    for (i, expected) in expected.iter().enumerate() {
        assert!((sample(&s, i as f32 / 10.).1 - expected.1).abs() < 1e-5);
    }
    apply(
        &mut s,
        E::MeshGrid(G::MoveSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            from: 0.4,
            to: 0.6,
        }),
    );
    assert_eq!(
        s.binding(BINDING_ID).unwrap().keyforms[1].positions[0].x,
        44.0
    );
    assert_eq!(s.binding(BINDING_ID).unwrap().keyforms[1].keys, vec![0.6]);
    let before = s.version();
    let bad = s
        .plan_binding_authoring(E::MeshGrid(G::MoveSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            from: 0.6,
            to: 1.0,
        }))
        .unwrap_err();
    assert_eq!(bad.code.as_ref(), "INVALID_KEYS");
    assert_eq!(s.version(), before);
    apply(
        &mut s,
        E::MeshGrid(G::DeleteSample {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            value: 0.6,
        }),
    );
    assert_eq!(s.binding(BINDING_ID).unwrap().keyforms.len(), 2);
    let before = s.version();
    assert!(s
        .plan_binding_authoring(E::MeshGrid(G::RemoveAxis {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            reduction: AxisReduction::Slice(0.5)
        }))
        .is_err());
    assert_eq!(s.version(), before);
    apply(
        &mut s,
        E::MeshGrid(G::RemoveAxis {
            binding_id: BINDING_ID.into(),
            parameter_id: PARAMETER_ID.into(),
            reduction: AxisReduction::Slice(1.0),
        }),
    );
    assert_eq!(frame(&s, 0.0, 0.0).drawables[0].positions[0].x, 50.0);
}

#[test]
fn zero_axis_external_export_requires_explicit_acceptance_of_omission() {
    let mut s = fixture();
    apply(
        &mut s,
        E::MeshGrid(G::Create {
            id: BINDING_ID.into(),
            mesh_id: MESH_ID.into(),
        }),
    );
    let error = s
        .encode_cmo3(&kasane_sdk::Cmo3Options::default())
        .unwrap_err();
    let kasane_sdk::Cmo3Error::Unsupported(report) = error else {
        panic!("{error:?}");
    };
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.object_id == BINDING_ID && d.code == "ZERO_AXIS_BINDING"));
    let artifact = s
        .encode_cmo3(&kasane_sdk::Cmo3Options {
            loss_policy: kasane_sdk::LossPolicy::Allow,
            ..Default::default()
        })
        .unwrap();
    assert!(!artifact.bytes.is_empty());
    assert!(artifact
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == "ZERO_AXIS_BINDING"));
    // The native authoring object remains present after either export path.
    assert!(s.binding(BINDING_ID).unwrap().keyforms.is_empty());
}
