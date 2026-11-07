use super::*;

#[test]
fn offscreen_blend_colors_survive_insert_and_rebase() {
    let mut s = shared();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_part({
                    let mut value = Part {
                        id: id(2100),
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })?;
                crate::support::insert_offscreen(
                    e,
                    Offscreen {
                        id: id(2101),
                        part_id: id(2100),
                        keyforms: vec![OffscreenKeyform {
                            opacity: 0.7,
                            multiply: Some([0.8; 3]),
                            screen: Some([0.1; 3]),
                        }],
                        ..Default::default()
                    },
                )?;
                {
                    let binding = BlendShapeBinding {
                        id: id(2102),
                        target_id: id(2101),
                        target_kind: BlendShapeTargetKind::Offscreen,
                        key_table_id: id(1701),
                        constraint_ids: vec![],
                        keyforms: DeltaKeyforms::Offscreen(
                            [-1., 0., 1.]
                                .into_iter()
                                .map(|v| DeltaOffscreenKeyform {
                                    opacity: v * 0.1,
                                    multiply: Some([1. + v * 0.2; 3]),
                                    screen: Some([v * 0.05; 3]),
                                    ..Default::default()
                                })
                                .collect(),
                        ),
                    };
                    e.edit_blend_shape(
                        zipzi_sdk::BlendAuthoringEdit::CreateBinding {
                            id: binding.id.clone(),
                            target_id: binding.target_id.clone(),
                            target_kind: binding.target_kind,
                            key_table_id: binding.key_table_id.clone(),
                        },
                        Default::default(),
                        &Default::default(),
                    )?;
                    e.replace_blend_binding(binding)
                }
            })
            .and_then(|value| {
                let prepared = draft.prepare("offscreen color fixture")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let expected: Vec<_> = (0..=20)
        .map(|i| frame(&s, 0.37, i as f32 / 10. - 1.).offscreens[0].clone())
        .collect();
    let base = s
        .snapshot()
        .document()
        .blend_key_table(&id(1701))
        .unwrap()
        .samples[2]
        .id
        .clone();
    for operation in [
        BO::Insert {
            sample: BindingSample {
                id: id(2103),
                value: 0.5,
            },
        },
        BO::SetBase { sample_id: base },
    ] {
        apply(
            &mut s,
            E::Blend(B::Table {
                id: id(1701),
                operation,
            }),
        );
        let check = |s: &AuthoringSession| {
            for (i, expected) in expected.iter().enumerate() {
                let actual = &frame(s, 0.37, i as f32 / 10. - 1.).offscreens[0];
                for (a, b) in actual
                    .multiply_color
                    .iter()
                    .chain(&actual.screen_color)
                    .chain([&actual.opacity])
                    .zip(
                        expected
                            .multiply_color
                            .iter()
                            .chain(&expected.screen_color)
                            .chain([&expected.opacity]),
                    )
                {
                    assert!(
                        (a - b).abs() < 1e-6,
                        "parameter {}: {a} != {b}",
                        i as f32 / 10. - 1.
                    );
                }
            }
        };
        check(&s);
        s.undo(s.version()).unwrap();
        check(&s);
        s.redo(s.version()).unwrap();
        check(&s);
    }
    roundtrip(&s);
}

#[test]
fn offscreen_blend_edits_respect_owner_locks_and_retargeting() {
    let mut s = shared();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                for n in [2110, 2111] {
                    e.create_part({
                        let mut value = Part {
                            id: id(n),
                            ..Default::default()
                        };
                        value.runtime_id = value.id.clone();
                        value
                    })?;
                    crate::support::insert_offscreen(
                        e,
                        Offscreen {
                            id: id(n + 2),
                            part_id: id(n),
                            runtime_id: format!("offscreen{n}"),
                            ..Default::default()
                        },
                    )?;
                }
                {
                    let binding = BlendShapeBinding {
                        id: id(2114),
                        target_id: id(2112),
                        target_kind: BlendShapeTargetKind::Offscreen,
                        key_table_id: id(1701),
                        constraint_ids: vec![],
                        keyforms: DeltaKeyforms::Offscreen(vec![
                            DeltaOffscreenKeyform::default();
                            3
                        ]),
                    };
                    e.edit_blend_shape(
                        zipzi_sdk::BlendAuthoringEdit::CreateBinding {
                            id: binding.id.clone(),
                            target_id: binding.target_id.clone(),
                            target_kind: binding.target_kind,
                            key_table_id: binding.key_table_id.clone(),
                        },
                        Default::default(),
                        &Default::default(),
                    )?;
                    e.replace_blend_binding(binding)
                }
            })
            .and_then(|value| {
                let prepared = draft.prepare("offscreen targets")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let mut flags = s.snapshot().document().editor_state().clone();
    flags.objects.insert(
        id(2111),
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
                let prepared = draft.prepare("lock owner")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let version = s.version();
    let intent = E::Blend(B::CreateBinding {
        id: id(2115),
        target_id: id(2113),
        target_kind: BlendShapeTargetKind::Offscreen,
        key_table_id: id(1701),
    });
    assert_eq!(
        crate::support::binding(&s, intent)
            .unwrap_err()
            .code
            .as_str(),
        "OBJECT_LOCKED"
    );
    let mut b = s
        .snapshot()
        .document()
        .blend_binding(&id(2114))
        .unwrap()
        .clone();
    b.target_id = id(2113);
    assert_eq!(
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_blend_binding(b))
                .and_then(|value| {
                    let prepared = draft.prepare("retarget")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    let mut offscreen = s
        .snapshot()
        .document()
        .offscreen(&id(2113))
        .unwrap()
        .clone();
    offscreen.enabled = false;
    assert_eq!(
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_offscreen(offscreen))
                .and_then(|value| {
                    let prepared = draft.prepare("replace locked offscreen")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.version(), version);
}
