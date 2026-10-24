use super::*;

#[test]
fn offscreen_blend_colors_survive_insert_and_rebase() {
    let mut s = shared();
    s.edit("offscreen color fixture", None, |e| {
        e.create_part(Part {
            id: id(2100),
            ..Default::default()
        })?;
        e.create_offscreen(Offscreen {
            id: id(2101),
            part_id: id(2100),
            keyforms: vec![OffscreenKeyform {
                opacity: 0.7,
                multiply: Some([0.8; 3]),
                screen: Some([0.1; 3]),
            }],
            ..Default::default()
        })?;
        e.create_blend_binding(BlendShapeBinding {
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
        })
    })
    .unwrap();
    let expected: Vec<_> = (0..=20)
        .map(|i| frame(&s, 0.37, i as f32 / 10. - 1.).offscreens[0].clone())
        .collect();
    let base = s.document().get_blend_key_table(&id(1701)).unwrap().samples[2]
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
        s.undo().unwrap();
        check(&s);
        s.redo().unwrap();
        check(&s);
    }
    roundtrip(&s);
}

#[test]
fn offscreen_blend_edits_respect_owner_locks_and_retargeting() {
    let mut s = shared();
    s.edit("offscreen targets", None, |e| {
        for n in [2110, 2111] {
            e.create_part(Part {
                id: id(n),
                ..Default::default()
            })?;
            e.create_offscreen(Offscreen {
                id: id(n + 2),
                part_id: id(n),
                runtime_id: format!("offscreen{n}"),
                ..Default::default()
            })?;
        }
        e.create_blend_binding(BlendShapeBinding {
            id: id(2114),
            target_id: id(2112),
            target_kind: BlendShapeTargetKind::Offscreen,
            key_table_id: id(1701),
            constraint_ids: vec![],
            keyforms: DeltaKeyforms::Offscreen(vec![DeltaOffscreenKeyform::default(); 3]),
        })
    })
    .unwrap();
    let mut flags = s.document().editor_state().clone();
    flags.objects.insert(
        id(2111),
        zipzi_core::document::ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    s.edit("lock owner", None, |e| e.replace_editor_state(flags))
        .unwrap();
    let version = s.version();
    let intent = E::Blend(B::CreateBinding {
        id: id(2115),
        target_id: id(2113),
        target_kind: BlendShapeTargetKind::Offscreen,
        key_table_id: id(1701),
    });
    assert_eq!(
        s.plan_binding_authoring(intent).unwrap_err().code.as_ref(),
        "OBJECT_LOCKED"
    );
    let mut b = s.document().get_blend_binding(&id(2114)).unwrap().clone();
    b.target_id = id(2113);
    assert_eq!(
        s.edit("retarget", None, |e| e.replace_blend_binding(b))
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    let mut offscreen = s.document().get_offscreen(&id(2113)).unwrap().clone();
    offscreen.enabled = false;
    assert_eq!(
        s.edit("replace locked offscreen", None, |e| e
            .replace_offscreen(offscreen))
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.version(), version);
}
