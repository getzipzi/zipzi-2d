use super::*;

#[test]
fn shared_blend_insert_move_rebase_updates_every_user_and_ordinary_grid() {
    let mut s = shared();
    // The base entry is authoring content even though evaluation ignores it.
    let mut authored = s.document().get_blend_binding(&id(1702)).unwrap().clone();
    if let DeltaKeyforms::Mesh(f) = &mut authored.keyforms {
        f[1].positions.fill(Vec2::new(91., 37.));
    }
    s.edit("authored base entry", None, |e| {
        e.replace_blend_binding(authored)
    })
    .unwrap();
    let original = s
        .document()
        .get_blend_binding(&id(1702))
        .unwrap()
        .keyforms
        .clone();
    let table = id(1701);
    let before: Vec<_> = (0..=16)
        .map(|i| {
            frame(&s, 0.37, i as f32 / 8. - 1.).drawables[0]
                .positions
                .clone()
        })
        .collect();
    let old = s.document().get_blend_key_table(&table).unwrap().clone();
    let sample = id(1704);
    apply(
        &mut s,
        E::Blend(B::Table {
            id: table.clone(),
            operation: BO::Insert {
                sample: BindingSample {
                    id: sample.clone(),
                    value: 0.5,
                },
            },
        }),
    );
    for n in [1702, 1703] {
        let DeltaKeyforms::Mesh(f) = &s.document().get_blend_binding(&id(n)).unwrap().keyforms
        else {
            panic!()
        };
        assert_eq!(f[2].sample_id, sample);
        assert_eq!(f[2].positions[0].x, 4.);
    }
    let DeltaKeyforms::Mesh(original_forms) = &original else {
        panic!()
    };
    let DeltaKeyforms::Mesh(after_insert) =
        &s.document().get_blend_binding(&id(1702)).unwrap().keyforms
    else {
        panic!()
    };
    for (before, after) in
        original_forms
            .iter()
            .zip([&after_insert[0], &after_insert[1], &after_insert[3]])
    {
        assert_eq!(before, after);
    }
    let base = old.samples[2].id.clone();
    apply(
        &mut s,
        E::Blend(B::Table {
            id: table.clone(),
            operation: BO::SetBase {
                sample_id: base.clone(),
            },
        }),
    );
    assert_eq!(
        s.document()
            .get_blend_key_table(&table)
            .unwrap()
            .base_sample_id,
        base
    );
    assert_eq!(
        s.document().get_mesh(MESH_ID).unwrap().base_positions[0].x,
        56.
    );
    assert_eq!(
        s.binding(BINDING_ID).unwrap().keyforms[0].positions[0].x,
        56.
    );
    for (i, expected) in before.iter().enumerate() {
        for (a, b) in frame(&s, 0.37, i as f32 / 8. - 1.).drawables[0]
            .positions
            .iter()
            .zip(expected)
        {
            assert!((a.x - b.x).abs() < 2e-5);
        }
    }
    roundtrip(&s);
    s.undo().unwrap();
    s.redo().unwrap();
    let address = s.binding_field_address(&sample).unwrap();
    let p = s
        .plan_binding_field_patch(&address, BindingFieldPatch::SampleValue(0.6))
        .unwrap();
    s.execute_binding_plan(&p).unwrap();
    let t = s.document().get_blend_key_table(&table).unwrap();
    assert_eq!(
        t.samples.iter().find(|k| k.id == sample).unwrap().value,
        0.6
    );
    let handle = s.handle(ObjectKind::BindingSample, &sample).unwrap();
    apply(
        &mut s,
        E::Blend(B::Table {
            id: table.clone(),
            operation: BO::Delete {
                sample_id: sample.clone(),
                replacement_base: None,
            },
        }),
    );
    s.undo().unwrap();
    assert!(s.document().binding_identity(&sample).is_some());
    assert!(s.resolve_handle(&handle).is_err());
}

#[test]
fn pending_shared_table_builds_from_empty_and_samples_its_retained_domain() {
    let mut s = shared();
    let table = id(1950);
    apply(
        &mut s,
        E::Blend(B::CreateTable {
            table: BlendShapeKeyTable {
                id: table.clone(),
                parameter_id: String::new(),
                samples: vec![],
                base_sample_id: String::new(),
                keys: vec![],
                base_key_idx: 0,
            },
        }),
    );
    apply(
        &mut s,
        E::Blend(B::CreateBinding {
            id: id(1951),
            target_id: MESH_ID.into(),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: table.clone(),
        }),
    );
    for (n, v) in [(1952, -1.), (1953, 1.)] {
        apply(
            &mut s,
            E::Blend(B::Table {
                id: table.clone(),
                operation: BO::Insert {
                    sample: BindingSample {
                        id: id(n),
                        value: v,
                    },
                },
            }),
        );
    }
    let mut binding = s.document().get_blend_binding(&id(1951)).unwrap().clone();
    if let DeltaKeyforms::Mesh(f) = &mut binding.keyforms {
        f[1].positions.fill(Vec2::new(6., 0.));
    }
    s.edit("pending deltas", None, |e| e.replace_blend_binding(binding))
        .unwrap();
    apply(
        &mut s,
        E::Blend(B::Table {
            id: table.clone(),
            operation: BO::Insert {
                sample: BindingSample {
                    id: id(1954),
                    value: 0.,
                },
            },
        }),
    );
    let DeltaKeyforms::Mesh(f) = &s.document().get_blend_binding(&id(1951)).unwrap().keyforms
    else {
        panic!()
    };
    assert_eq!(f[1].positions[0].x, 3.);
    let before = frame(&s, 0.4, 0.).drawables[0].positions[0].x;
    apply(
        &mut s,
        E::Blend(B::Table {
            id: table.clone(),
            operation: BO::SetParameter {
                parameter_id: id(1700),
            },
        }),
    );
    assert_eq!(frame(&s, 0.4, 0.).drawables[0].positions[0].x, before + 3.);
    let address = s.binding_field_address(&id(1954)).unwrap();
    apply(
        &mut s,
        E::Blend(B::Table {
            id: table.clone(),
            operation: BO::SetParameter {
                parameter_id: String::new(),
            },
        }),
    );
    s.undo().unwrap();
    assert!(s
        .plan_binding_field_patch(&address, BindingFieldPatch::SampleValue(0.2))
        .is_err());
    roundtrip(&s);
}

#[test]
fn shared_base_removal_and_table_switch_preserve_retained_samples_or_require_reset() {
    let mut s = shared();
    let old = s.document().get_blend_key_table(&id(1701)).unwrap().clone();
    let endpoints = [
        frame(&s, 0.37, -1.).drawables[0].positions.clone(),
        frame(&s, 0.37, 1.).drawables[0].positions.clone(),
    ];
    let handle = s
        .handle(ObjectKind::BindingSample, &old.base_sample_id)
        .unwrap();
    let plan = s
        .plan_binding_authoring(E::Blend(B::Table {
            id: old.id.clone(),
            operation: BO::Delete {
                sample_id: old.base_sample_id.clone(),
                replacement_base: Some(old.samples[0].id.clone()),
            },
        }))
        .unwrap();
    let kasane_sdk::BindingConversion::Blend(conversion) = plan.conversion() else {
        panic!()
    };
    assert_eq!(conversion.losses().len(), 1);
    s.execute_binding_plan(&plan).unwrap();
    for (i, value) in [-1., 1.].into_iter().enumerate() {
        assert_eq!(frame(&s, 0.37, value).drawables[0].positions, endpoints[i]);
    }
    let source = s.document().get_blend_key_table(&old.id).unwrap().clone();
    apply(
        &mut s,
        E::Blend(B::CreateTable {
            table: BlendShapeKeyTable {
                id: id(1990),
                parameter_id: source.parameter_id.clone(),
                keys: source.keys.clone(),
                base_key_idx: source.base_key_idx,
                samples: vec![],
                base_sample_id: String::new(),
            },
        }),
    );
    let switch = s
        .plan_binding_authoring(E::Blend(B::SetBindingTable {
            id: id(1702),
            key_table_id: id(1990),
            reset: false,
        }))
        .unwrap();
    let target = s.document().get_blend_key_table(&id(1990)).unwrap();
    assert_eq!(switch.report().reconstructed.len(), source.samples.len());
    for domains in [&switch.report().domain, &switch.report().result_domain] {
        let domain = domains.iter().find(|d| d.owner == id(1702)).unwrap();
        assert_eq!(domain.interval.parameter_id, source.parameter_id);
        assert_eq!(
            domain.interval.minimum,
            source.samples.first().unwrap().value
        );
        assert_eq!(
            domain.interval.maximum,
            source.samples.last().unwrap().value
        );
    }

    for ((transfer, before), after) in switch
        .report()
        .reconstructed
        .iter()
        .zip(&source.samples)
        .zip(&target.samples)
    {
        use kasane_core::document::ConversionElement::Keyform;
        assert_eq!(
            transfer.destination.element,
            Keyform(vec![after.id.clone()])
        );
        assert_eq!(
            transfer.sources[0].0.element,
            Keyform(vec![before.id.clone()])
        );
        assert_eq!(transfer.sources[0].1, 1.);
    }
    s.execute_binding_plan(&switch).unwrap();
    for (i, value) in [-1., 1.].into_iter().enumerate() {
        assert_eq!(frame(&s, 0.37, value).drawables[0].positions, endpoints[i]);
    }
    assert_eq!(
        s.document()
            .get_blend_binding(&id(1703))
            .unwrap()
            .key_table_id,
        old.id
    );
    let before = s.version();
    let history = s.history_state();
    assert_eq!(
        s.plan_binding_authoring(E::Blend(B::SetBindingTable {
            id: id(1702),
            key_table_id: String::new(),
            reset: false
        }))
        .unwrap_err()
        .code
        .as_ref(),
        "BLEND_REMAP_REQUIRED"
    );
    assert_eq!(s.version(), before);
    assert_eq!(s.history_state(), history);
    let plan = s
        .plan_binding_authoring(E::Blend(B::SetBindingTable {
            id: id(1702),
            key_table_id: String::new(),
            reset: true,
        }))
        .unwrap();
    let kasane_sdk::BindingConversion::Blend(conversion) = plan.conversion() else {
        panic!()
    };
    assert_eq!(conversion.losses().len(), 1);
    s.execute_binding_plan(&plan).unwrap();
    assert!(!s
        .document()
        .blend_binding_active(s.document().get_blend_binding(&id(1702)).unwrap()));
    roundtrip(&s);
    while s.document().binding_identity(&old.base_sample_id).is_none() {
        s.undo().unwrap();
    }
    assert!(s.resolve_handle(&handle).is_err());
}

#[test]
fn affine_reparent_transforms_shared_mesh_deltas_without_translation_or_identity_loss() {
    use kasane_sdk::{ReparentContext, ReparentEdit, ReparentStrategy};
    let mut s = shared();
    let root = id(2700);
    s.edit("static reflected rotation", None, |e| {
        e.create_transform(Transform {
            id: root.clone(),
            data: TransformData::Rotation(RotationTransform {
                base_angle: 17.,
                pose: RotationPose {
                    angle: 23.,
                    scale: 1.2,
                    origin: PreciseVec2::new(5., -3.),
                    reflect_x: true,
                    ..Default::default()
                },
            }),
            ..Default::default()
        })
    })
    .unwrap();
    let handles: Vec<_> = [1702, 1703]
        .iter()
        .map(|&n| s.handle(ObjectKind::BlendBinding, &id(n)).unwrap())
        .collect();
    let original: Vec<_> = (0..=8)
        .flat_map(|p| (0..=8).map(move |b| (p, b)))
        .map(|(p, b)| {
            (
                (p, b),
                frame(&s, p as f32 / 8., b as f32 / 4. - 1.).drawables[0]
                    .positions
                    .clone(),
            )
        })
        .collect();
    let plan = s
        .plan_binding_authoring(E::Reparent(ReparentEdit {
            sources: vec![MESH_ID.into()],
            parent_id: root.clone(),
            strategy: ReparentStrategy::PreserveParameterDomain,
            tolerance: 0.1,
            relative_tolerance: 1e-6,
            context: Some(ReparentContext {
                parameter_values: Default::default(),
                parameter_domain: vec![
                    kasane_core::document::ConversionDomain {
                        parameter_id: PARAMETER_ID.into(),
                        minimum: 0.,
                        maximum: 1.,
                    },
                    kasane_core::document::ConversionDomain {
                        parameter_id: id(1700),
                        minimum: -1.,
                        maximum: 1.,
                    },
                ],
                edit_time_seconds: 0.,
            }),
        }))
        .unwrap();
    assert!(plan.affected_ids().contains(&id(1702)));
    assert!(plan.affected_ids().contains(&id(1703)));
    assert!(plan.report().generated.is_empty());
    assert!(plan.report().deleted.is_empty());
    s.execute_binding_plan(&plan).unwrap();
    for ((p, b), expected) in &original {
        for (a, b) in frame(&s, *p as f32 / 8., *b as f32 / 4. - 1.).drawables[0]
            .positions
            .iter()
            .zip(expected)
        {
            assert!((a.x - b.x).hypot(a.y - b.y) < 0.0001);
        }
    }
    for handle in handles {
        assert!(s.resolve_handle(&handle).is_ok());
    }
    roundtrip(&s);
    s.undo().unwrap();
    s.redo().unwrap();
    assert_eq!(s.document().get_mesh(MESH_ID).unwrap().deformer_id, root);
}
