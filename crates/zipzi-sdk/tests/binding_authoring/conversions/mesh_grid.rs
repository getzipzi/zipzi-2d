use super::*;

#[test]
fn zero_axis_uses_latest_base_and_last_axis_writes_selected_shape_back() {
    let mut s = fixture();
    apply(
        &mut s,
        E::Grid(zipzi_sdk::BindingGridEdit::Create {
            id: BINDING_ID.into(),
            target: zipzi_sdk::BindingGridTarget::Mesh(MESH_ID.into()),
        }),
    );
    let b = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap();
    assert!(b.axes.is_empty() && b.keyforms.is_empty());
    let mut m = s.snapshot().document().mesh(MESH_ID).unwrap().clone();
    m.base_positions.iter_mut().for_each(|p| p.x += 3.0);
    m.appearance.opacity = 0.6;
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.replace_mesh(m)?;
                e.create_parameter(parameter(false))
            })
            .and_then(|value| {
                let prepared = draft.prepare("base")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(frame(&s, 0.0, 0.0).drawables[0].positions[0].x, 43.0);
    grid_roundtrip(&s);
    apply(
        &mut s,
        E::Grid(zipzi_sdk::BindingGridEdit::Edit {
            owner: BINDING_ID.into(),
            operation: zipzi_sdk::GridOperation::AddAxis {
                axis: BindingAxis {
                    id: Default::default(),
                    samples: Default::default(),
                    parameter_id: PARAMETER_ID.into(),
                    keys: vec![0.0, 1.0],
                },
            },
        }),
    );
    assert!(s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap()
        .keyforms
        .iter()
        .all(|f| f.positions[0].x == 43.0 && f.appearance.opacity == 0.6));
    let mut f = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap()
        .keyforms[1]
        .clone();
    f.positions.iter_mut().for_each(|p| p.y += 7.0);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.set_mesh_keyform(BINDING_ID, f))
            .and_then(|value| {
                let prepared = draft.prepare("shape")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let expected = frame(&s, 0.25, 0.0).drawables[0].clone();
    {
        let intent = E::Grid(zipzi_sdk::BindingGridEdit::Edit {
            owner: BINDING_ID.into(),
            operation: zipzi_sdk::GridOperation::RemoveAxis {
                axis_id: crate::support::axis(&s, BINDING_ID, PARAMETER_ID),
                reduction: Some(AxisReduction::Resample(0.25)),
                keep_pending: false,
            },
        });
        apply(&mut s, intent)
    };
    let actual = frame(&s, 0.0, 0.0).drawables[0].clone();
    assert_eq!(actual.positions, expected.positions);
    assert_eq!(actual.opacity, expected.opacity);
    assert!(s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap()
        .keyforms
        .is_empty());
    grid_roundtrip(&s);
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .binding(BINDING_ID)
            .cloned()
            .unwrap()
            .keyforms
            .len(),
        2
    );
    s.redo(s.version()).unwrap();
    assert_eq!(
        frame(&s, 1.0, 0.0).drawables[0].positions,
        expected.positions
    );
}

#[test]
fn expand_three_distinct_shapes_and_reorder_preserve_old_domain() {
    let mut s = bound();
    {
        let intent = E::Grid(zipzi_sdk::BindingGridEdit::Edit {
            owner: BINDING_ID.into(),
            operation: zipzi_sdk::GridOperation::InsertSample {
                axis_id: crate::support::axis(&s, BINDING_ID, PARAMETER_ID),
                id: uuid::Uuid::new_v4().to_string(),
                value: 0.5,
            },
        });
        apply(&mut s, intent)
    };
    let mut f = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap()
        .keyforms[1]
        .clone();
    f.positions.iter_mut().for_each(|p| p.y += 8.0);
    f.appearance.opacity = 0.25;
    f.draw_order = Some(8.0);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.set_mesh_keyform(BINDING_ID, f))
            .and_then(|value| {
                let prepared = draft.prepare("middle shape")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: Q.into(),
                        ..Parameter::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("new driver")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let before: Vec<_> = (0..=20)
        .map(|i| frame(&s, i as f32 / 20., 0.).drawables[0].clone())
        .collect();
    let old_forms = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap()
        .keyforms;
    apply(
        &mut s,
        E::Grid(zipzi_sdk::BindingGridEdit::Edit {
            owner: BINDING_ID.into(),
            operation: zipzi_sdk::GridOperation::AddAxis {
                axis: BindingAxis {
                    id: Default::default(),
                    samples: Default::default(),
                    parameter_id: Q.into(),
                    keys: vec![-1.0, 0.0, 1.0],
                },
            },
        }),
    );
    let b = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap();
    assert_eq!(b.keyforms.len(), 9);
    for chunk in b.keyforms.chunks(3) {
        for (f, old) in chunk.iter().zip(&old_forms) {
            assert_eq!(f.positions, old.positions);
            assert_eq!(f.appearance, old.appearance);
            assert_eq!(f.draw_order, old.draw_order);
        }
    }
    {
        let intent = E::Grid(zipzi_sdk::BindingGridEdit::Edit {
            owner: BINDING_ID.into(),
            operation: zipzi_sdk::GridOperation::ReorderAxes {
                axis_ids: (vec![Q.into(), PARAMETER_ID.into()])
                    .into_iter()
                    .map(|p: String| crate::support::axis(&s, BINDING_ID, &p))
                    .collect(),
            },
        });
        apply(&mut s, intent)
    };
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
    {
        let intent = E::Grid(zipzi_sdk::BindingGridEdit::Edit {
            owner: BINDING_ID.into(),
            operation: zipzi_sdk::GridOperation::InsertSample {
                axis_id: crate::support::axis(&s, BINDING_ID, PARAMETER_ID),
                id: uuid::Uuid::new_v4().to_string(),
                value: 0.4,
            },
        });
        apply(&mut s, intent)
    };
    for (i, expected) in expected.iter().enumerate() {
        assert!((sample(&s, i as f32 / 10.).1 - expected.1).abs() < 1e-5);
    }
    {
        let intent = E::Grid(zipzi_sdk::BindingGridEdit::Edit {
            owner: BINDING_ID.into(),
            operation: zipzi_sdk::GridOperation::MoveSample {
                sample_id: crate::support::sample(&s, BINDING_ID, PARAMETER_ID, 0.4),
                value: 0.6,
            },
        });
        apply(&mut s, intent)
    };
    assert_eq!(
        s.snapshot()
            .document()
            .binding(BINDING_ID)
            .cloned()
            .unwrap()
            .keyforms[1]
            .positions[0]
            .x,
        44.0
    );
    assert_eq!(
        s.snapshot()
            .document()
            .binding(BINDING_ID)
            .cloned()
            .unwrap()
            .keyforms[1]
            .keys,
        vec![0.6]
    );
    let before = s.version();
    let bad = crate::support::binding(
        &s,
        E::Grid(zipzi_sdk::BindingGridEdit::Edit {
            owner: BINDING_ID.into(),
            operation: zipzi_sdk::GridOperation::MoveSample {
                sample_id: crate::support::sample(&s, BINDING_ID, PARAMETER_ID, 0.6),
                value: 1.0,
            },
        }),
    )
    .unwrap_err();
    assert_eq!(bad.code.as_str(), "INVALID_KEYS");
    assert_eq!(s.version(), before);
    {
        let intent = E::Grid(zipzi_sdk::BindingGridEdit::Edit {
            owner: BINDING_ID.into(),
            operation: zipzi_sdk::GridOperation::DeleteSample {
                sample_id: crate::support::sample(&s, BINDING_ID, PARAMETER_ID, 0.6),
            },
        });
        apply(&mut s, intent)
    };
    assert_eq!(
        s.snapshot()
            .document()
            .binding(BINDING_ID)
            .cloned()
            .unwrap()
            .keyforms
            .len(),
        2
    );
    let before = s.version();
    assert!(crate::support::binding(
        &s,
        E::Grid(zipzi_sdk::BindingGridEdit::Edit {
            owner: BINDING_ID.into(),
            operation: zipzi_sdk::GridOperation::RemoveAxis {
                axis_id: crate::support::axis(&s, BINDING_ID, PARAMETER_ID),
                reduction: Some(AxisReduction::Slice(0.5)),
                keep_pending: false
            }
        })
    )
    .is_err());
    assert_eq!(s.version(), before);
    {
        let intent = E::Grid(zipzi_sdk::BindingGridEdit::Edit {
            owner: BINDING_ID.into(),
            operation: zipzi_sdk::GridOperation::RemoveAxis {
                axis_id: crate::support::axis(&s, BINDING_ID, PARAMETER_ID),
                reduction: Some(AxisReduction::Slice(1.0)),
                keep_pending: false,
            },
        });
        apply(&mut s, intent)
    };
    assert_eq!(frame(&s, 0.0, 0.0).drawables[0].positions[0].x, 50.0);
}

#[test]
fn zero_axis_external_export_requires_explicit_acceptance_of_omission() {
    let mut s = fixture();
    apply(
        &mut s,
        E::Grid(zipzi_sdk::BindingGridEdit::Create {
            id: BINDING_ID.into(),
            target: zipzi_sdk::BindingGridTarget::Mesh(MESH_ID.into()),
        }),
    );
    let error = zipzi_project::encode_document_cmo3(
        s.snapshot().document(),
        s.snapshot().resources(),
        &zipzi_project::Cmo3Options::default(),
    )
    .unwrap_err();
    let zipzi_project::Cmo3Error::Unsupported(report) = error else {
        panic!("{error:?}");
    };
    assert!(report
        .diagnostics
        .iter()
        .any(|d| d.object_id == BINDING_ID && d.code == "ZERO_AXIS_BINDING"));
    let artifact = zipzi_project::encode_document_cmo3(
        s.snapshot().document(),
        s.snapshot().resources(),
        &zipzi_project::Cmo3Options {
            loss_policy: zipzi_project::LossPolicy::Allow,
            ..Default::default()
        },
    )
    .unwrap();
    assert!(!artifact.bytes.is_empty());
    assert!(artifact
        .report
        .diagnostics
        .iter()
        .any(|d| d.code == "ZERO_AXIS_BINDING"));
    // The native authoring object remains present after either export path.
    assert!(s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap()
        .keyforms
        .is_empty());
}
