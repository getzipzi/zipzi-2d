use super::*;
use zipzi_core::{
    BindingAxis, BindingAxisEditing, BindingSample, BlendShapeKeyTable, BlendShapeTargetKind,
    ParameterKind,
};
fn edit(s: &mut AuthoringSession, run: impl FnOnce(&mut Commands<'_>) -> Result<(), Status>) {
    let mut d = s.begin_draft();
    d.batch(d.source(), run).unwrap();
    s.publish(d.prepare("conversion").unwrap()).unwrap();
}
#[test]
fn inserted_grid_sample_interpolates_and_range_mapping_keeps_sample_ids() {
    let mut s = AuthoringSession::new(snapshot());
    let initial = s.snapshot().clone();
    let control = ConversionControl::default();
    edit(&mut s, |c| {
        c.edit_binding_grid(
            BindingGridEdit::Create {
                id: id(10),
                target: BindingGridTarget::Mesh(id(4)),
            },
            Default::default(),
            &control,
        )?;
        c.edit_binding_grid(
            BindingGridEdit::Edit {
                owner: id(10),
                operation: GridOperation::AddAxis {
                    axis: BindingAxis::new(id(2), vec![0., 1.]),
                },
            },
            Default::default(),
            &control,
        )?;
        let mut positions = mesh().base_positions;
        for p in &mut positions {
            p.x += 10.;
        }
        c.set_mesh_keyform(
            &id(10),
            zipzi_core::MeshKeyform {
                keys: vec![1.],
                positions,
                ..Default::default()
            },
        )
    });
    let axis = s.snapshot().document().binding(&id(10)).unwrap().axes[0]
        .id
        .clone();
    edit(&mut s, |c| {
        let result = c.edit_binding_grid(
            BindingGridEdit::Edit {
                owner: id(10),
                operation: GridOperation::InsertSample {
                    axis_id: axis,
                    id: id(11),
                    value: 0.5,
                },
            },
            Default::default(),
            &control,
        )?;
        assert!(!result.report.reconstructed.is_empty());
        Ok(())
    });
    let b = s.snapshot().document().binding(&id(10)).unwrap();
    assert_eq!(b.keyforms[1].positions[0].x, 5.);
    let samples = b.axes[0]
        .samples
        .iter()
        .map(|v| v.id.clone())
        .collect::<Vec<_>>();
    edit(&mut s, |c| {
        c.edit_parameter_range(
            ParameterRangeEdit {
                parameter_id: id(2),
                minimum: -2.,
                maximum: 2.,
                default_value: 0.,
                policy: ParameterRangePolicy::MapKeyValues,
            },
            Default::default(),
            &control,
        )?;
        Ok(())
    });
    let b = s.snapshot().document().binding(&id(10)).unwrap();
    // The original parameter range is [-1, 1], so 0 maps to 0.
    assert_eq!(b.axes[0].keys, [0., 1., 2.]);
    assert_eq!(
        b.axes[0]
            .samples
            .iter()
            .map(|v| v.id.clone())
            .collect::<Vec<_>>(),
        samples
    );
    for _ in 0..3 {
        s.undo(s.version()).unwrap();
    }
    assert_eq!(s.snapshot().content_id(), initial.content_id());
}
#[test]
fn conversion_cancellation_and_grid_limits_abort_entire_batch() {
    for cancelled in [false, true] {
        let s = AuthoringSession::new(snapshot());
        let initial = s.snapshot().content_id();
        let control = ConversionControl::default();
        if cancelled {
            control.cancel();
        }
        let mut d = s.begin_draft();
        let e = d
            .batch(d.source(), |c| {
                c.rename(NameTarget::Mesh(&id(4)), "discard".into())?;
                c.edit_binding_grid(
                    BindingGridEdit::Create {
                        id: id(10),
                        target: BindingGridTarget::Mesh(id(4)),
                    },
                    ConversionLimits {
                        max_sample_combinations: 0,
                        ..Default::default()
                    },
                    &control,
                )?;
                Ok(())
            })
            .unwrap_err();
        assert_eq!(
            e.code,
            if cancelled {
                "AUTHORING_CANCELLED"
            } else {
                "AUTHORING_GRID_LIMIT"
            }
        );
        assert!(!d.is_active());
        assert_eq!(s.snapshot().content_id(), initial);
        assert!(d.prepare("rejected").is_err());
        assert_eq!(s.history_state().undo_steps, 0);
    }
}
#[test]
fn blend_table_shared_users_rebase_and_roundtrip_through_history() {
    let mut s = AuthoringSession::new(snapshot());
    let control = ConversionControl::default();
    edit(&mut s, |c| {
        c.create_parameter(Parameter {
            id: id(20),
            kind: ParameterKind::BlendShape,
            ..Default::default()
        })?;
        c.edit_blend_shape(
            BlendAuthoringEdit::CreateTable {
                table: BlendShapeKeyTable {
                    id: id(21),
                    parameter_id: id(20),
                    samples: vec![
                        BindingSample {
                            id: id(22),
                            value: 0.,
                        },
                        BindingSample {
                            id: id(23),
                            value: 1.,
                        },
                    ],
                    base_sample_id: id(22),
                    keys: vec![],
                    base_key_idx: 0,
                },
            },
            Default::default(),
            &control,
        )?;
        c.edit_blend_shape(
            BlendAuthoringEdit::CreateBinding {
                id: id(24),
                target_id: id(4),
                target_kind: BlendShapeTargetKind::Mesh,
                key_table_id: id(21),
            },
            Default::default(),
            &control,
        )?;
        c.edit_blend_shape(
            BlendAuthoringEdit::CreateBinding {
                id: id(25),
                target_id: id(3),
                target_kind: BlendShapeTargetKind::Part,
                key_table_id: id(21),
            },
            Default::default(),
            &control,
        )?;
        Ok(())
    });
    let mut binding = s
        .snapshot()
        .document()
        .blend_binding(&id(24))
        .unwrap()
        .clone();
    let zipzi_core::DeltaKeyforms::Mesh(forms) = &mut binding.keyforms else {
        panic!()
    };
    for p in &mut forms[1].positions {
        p.x = 4.;
        p.y = 2.;
    }
    edit(&mut s, |c| c.replace_blend_binding(binding));
    let evaluated = |snapshot: &Snapshot| {
        [0., 0.5, 1.]
            .into_iter()
            .map(|value| {
                let mut frame = zipzi_core::DrawableFrame::default();
                assert!(zipzi_core::evaluation::evaluate_frame_including_hidden(
                    snapshot.document(),
                    &[(id(20), value)].into(),
                    &mut frame
                )
                .is_ok());
                frame.drawables[0].positions.clone()
            })
            .collect::<Vec<_>>()
    };
    let before = s.snapshot().clone();
    let shapes = evaluated(&before);
    edit(&mut s, |c| {
        let result = c.edit_blend_shape(
            BlendAuthoringEdit::Table {
                id: id(21),
                operation: BlendTableOperation::Insert {
                    sample: BindingSample {
                        id: id(26),
                        value: 0.5,
                    },
                },
            },
            Default::default(),
            &control,
        )?;
        assert!(result.report.affected.contains(&id(24)));
        assert!(result.report.affected.contains(&id(25)));
        Ok(())
    });
    assert_eq!(
        s.snapshot()
            .document()
            .blend_binding(&id(24))
            .unwrap()
            .keyforms
            .len(),
        3
    );
    edit(&mut s, |c| {
        c.edit_blend_shape(
            BlendAuthoringEdit::Table {
                id: id(21),
                operation: BlendTableOperation::SetBase { sample_id: id(26) },
            },
            Default::default(),
            &control,
        )?;
        Ok(())
    });
    assert_eq!(
        s.snapshot()
            .document()
            .blend_key_table(&id(21))
            .unwrap()
            .base_sample_id,
        id(26)
    );
    assert_eq!(evaluated(s.snapshot()), shapes);
    s.undo(s.version()).unwrap();
    s.undo(s.version()).unwrap();
    assert_eq!(s.snapshot().content_id(), before.content_id());
}
#[test]
fn conversion_admission_depends_on_work_not_history() {
    let limits = ConversionLimits {
        max_sample_combinations: 8,
        max_workspace_bytes: 128,
    };
    assert_eq!(limits.check_grid(&[2, 4], 8).unwrap().workspace_bytes, 128);
    assert_eq!(
        limits.check_grid(&[2, 5], 8).unwrap_err().code,
        "AUTHORING_GRID_LIMIT"
    );
    assert_eq!(
        limits.check_grid(&[2, 4], 9).unwrap_err().code,
        "AUTHORING_MEMORY_LIMIT"
    );
    assert_eq!(
        limits.check_grid(&[usize::MAX, 2], 1).unwrap_err().code,
        "AUTHORING_CAPACITY_OVERFLOW"
    );
}
