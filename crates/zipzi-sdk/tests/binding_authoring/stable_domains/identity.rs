use super::*;
use zipzi_core::BindingAxisEditing;

#[test]
fn stable_axis_sample_ids_roundtrip_and_stale_sources_do_not_revive() {
    let mut s = base();
    let old = s.snapshot().document().binding(BINDING_ID).unwrap().clone();
    let axis = old.axes[0].id.clone();
    let sample = old.axes[0].samples[0].id.clone();
    op(
        &mut s,
        BINDING_ID,
        O::AddAxis {
            axis: BindingAxis::new(id(1600), vec![-1., 1.]),
        },
    );
    let stale = s.begin_draft();
    let b = s.snapshot().document().binding(BINDING_ID).unwrap().clone();
    op(
        &mut s,
        BINDING_ID,
        O::ReorderAxes {
            axis_ids: vec![b.axes[1].id.clone(), b.axes[0].id.clone()],
        },
    );
    let receipt = op(
        &mut s,
        BINDING_ID,
        O::MoveSample {
            sample_id: sample.clone(),
            value: 0.1,
        },
    );
    assert!(receipt.writes.records().iter().any(|r| r.id == BINDING_ID));
    assert_eq!(
        s.snapshot().document().binding(BINDING_ID).unwrap().axes[1].samples[0].id,
        sample
    );
    assert_eq!(
        roundtrip(&s).binding(BINDING_ID),
        s.snapshot().document().binding(BINDING_ID)
    );
    assert_eq!(
        s.publish(stale.prepare("stale").unwrap())
            .unwrap_err()
            .code
            .as_str(),
        "STALE_SOURCE"
    );
    let stale = s.begin_draft();
    op(
        &mut s,
        BINDING_ID,
        O::SetParameter {
            axis_id: axis.clone(),
            parameter_id: String::new(),
            reduction: Some(R::Slice(0.1)),
        },
    );
    s.undo(s.version()).unwrap();
    s.snapshot()
        .document()
        .require_object(ObjectKind::BindingSample, &sample)
        .unwrap();
    s.snapshot()
        .document()
        .require_object(ObjectKind::BindingAxis, &axis)
        .unwrap();
    assert_eq!(
        s.publish(stale.prepare("stale after undo").unwrap())
            .unwrap_err()
            .code
            .as_str(),
        "STALE_SOURCE"
    );
    op(
        &mut s,
        BINDING_ID,
        O::RemoveAxis {
            axis_id: axis,
            reduction: Some(R::Slice(0.1)),
            keep_pending: false,
        },
    );
    s.undo(s.version()).unwrap();
    assert!(s
        .snapshot()
        .document()
        .contains_object(ObjectKind::BindingSample, &sample));
}

#[test]
fn driver_remapping_is_atomic_and_preserves_sample_identity() {
    let mut s = base();
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(1910),
                        minimum: 10.,
                        maximum: 20.,
                        default_value: 15.,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("new domain")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let old = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap();
    op(
        &mut s,
        BINDING_ID,
        O::RemapParameter {
            axis_id: old.axes[0].id.clone(),
            parameter_id: id(1910),
            policy: zipzi_sdk::ParameterRangePolicy::MapKeyValues,
        },
    );
    let new = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap();
    assert_eq!(new.axes[0].id, old.axes[0].id);
    assert_eq!(new.axes[0].samples[0].id, old.axes[0].samples[0].id);
    assert_eq!(new.axes[0].keys, vec![10., 20.]);
    assert_eq!(new.keyforms[1].samples, old.keyforms[1].samples);
    roundtrip(&s);
}

#[test]
fn native_rejects_missing_shared_and_glue_identities_and_numeric_adapters_keep_ids() {
    let mut s = shared();
    let g = id(1960);
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_glue(
                    e,
                    Glue {
                        id: g.clone(),
                        runtime_id: g.clone(),
                        name: String::new(),
                        mesh_a_id: MESH_ID.into(),
                        mesh_b_id: MESH_ID.into(),
                        pairs: vec![],
                        intensity: 1.,
                        binding: Some(GlueBinding {
                            axes: vec![BindingAxis::new(PARAMETER_ID, vec![0., 1.])],
                            keyforms: vec![
                                GlueKeyform {
                                    samples: vec![],
                                    intensity: 1.
                                };
                                2
                            ],
                        }),
                    },
                )
            })
            .and_then(|value| {
                let prepared = draft.prepare("glue")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    apply(
        &mut s,
        E::Blend(B::CreateConstraint {
            constraint: BlendShapeConstraint {
                id: id(1961),
                parameter_id: PARAMETER_ID.into(),
                samples: vec![],
                keys: vec![0., 1.],
                weights: vec![0., 1.],
            },
        }),
    );
    let old = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap();
    let mut numeric = old.clone();
    for axis in &mut numeric.axes {
        axis.id.clear();
        axis.samples.clear();
    }
    for form in &mut numeric.keyforms {
        form.samples.clear();
    }
    let (_, receipt) = {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_binding(numeric))
            .and_then(|value| {
                let prepared = draft.prepare("numeric import adapter")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(receipt.writes.is_empty());
    assert_eq!(
        s.snapshot()
            .document()
            .binding(BINDING_ID)
            .cloned()
            .unwrap(),
        old
    );
    let bytes = crate::support::archive::native_bytes_with_resources(
        s.snapshot().document(),
        s.snapshot().resources(),
    );
    let wire = crate::support::archive::project_json(&bytes);
    for path in [
        "/document/glues/0/binding/axes/0/id",
        "/document/glues/0/binding/axes/0/samples/0/id",
        "/document/blend_key_tables/0/base_sample_id",
        "/document/blend_key_tables/0/samples/0/id",
        "/document/blend_constraints/0/samples/0/id",
        "/document/blend_bindings/0/keyforms/items/0/sample_id",
    ] {
        let mut invalid = wire.clone();
        *invalid.pointer_mut(path).unwrap() = serde_json::json!("");
        assert!(
            crate::support::archive::load_native(&crate::support::archive::replace_project_json(
                &bytes, &invalid
            ))
            .is_err(),
            "{path}"
        );
    }
    let mut invalid = wire.clone();
    invalid["document"]["glues"][0]["binding"]["keyforms"][0]["samples"] = serde_json::json!([]);
    assert!(
        crate::support::archive::load_native(&crate::support::archive::replace_project_json(
            &bytes, &invalid
        ))
        .is_err()
    );
    assert!(s
        .snapshot()
        .document()
        .references_to(&old.axes[0].samples[0].id)
        .contains(&BINDING_ID.to_string()));
}

#[test]
fn keyform_setters_resolve_stable_combinations_after_axis_reorder() {
    let mut s = base();
    op(
        &mut s,
        BINDING_ID,
        O::AddAxis {
            axis: BindingAxis::new(id(1600), vec![-1., 1.]),
        },
    );
    let source = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap();
    let mut form = source.keyforms[1].clone();
    form.keys.clear();
    form.appearance.opacity = 0.7;
    op(
        &mut s,
        BINDING_ID,
        O::ReorderAxes {
            axis_ids: source.axes.iter().rev().map(|a| a.id.clone()).collect(),
        },
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.set_mesh_keyform(BINDING_ID, form))
            .and_then(|value| {
                let prepared = draft.prepare("stable pose")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let after = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap();
    let f = after
        .keyforms
        .iter()
        .find(|f| f.keys == vec![-1., 1.])
        .unwrap();
    assert_eq!(f.appearance.opacity, 0.7);
    assert_eq!(
        f.samples,
        vec![
            source.axes[1].samples[0].id.clone(),
            source.axes[0].samples[1].id.clone()
        ]
    );
    roundtrip(&s);
}

#[test]
fn table_field_context_tracks_driver_domains_even_when_values_do_not_move() {
    let mut s = shared();
    let table = s
        .snapshot()
        .document()
        .blend_key_table(&id(1701))
        .unwrap()
        .clone();
    let stale = s.begin_draft();
    let mut parameter = s
        .snapshot()
        .document()
        .parameter(&table.parameter_id)
        .unwrap()
        .clone();
    parameter.minimum = -2.;
    parameter.maximum = 2.;
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_parameter(parameter))
            .and_then(|value| {
                let prepared = draft.prepare("domain")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    s.undo(s.version()).unwrap();
    assert_eq!(
        s.publish(stale.prepare("stale driver").unwrap())
            .unwrap_err()
            .code
            .as_str(),
        "STALE_SOURCE"
    );
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.create_parameter({
                    let mut value = Parameter {
                        id: id(1991),
                        kind: ParameterKind::BlendShape,
                        minimum: 10.,
                        maximum: 20.,
                        default_value: 15.,
                        ..Default::default()
                    };
                    value.runtime_id = value.id.clone();
                    value
                })
            })
            .and_then(|value| {
                let prepared = draft.prepare("new blend driver")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    apply(
        &mut s,
        E::Blend(B::Table {
            id: table.id.clone(),
            operation: BO::RemapParameter {
                parameter_id: id(1991),
                policy: zipzi_sdk::ParameterRangePolicy::MapKeyValues,
            },
        }),
    );
    let mapped = s.snapshot().document().blend_key_table(&table.id).unwrap();
    assert_eq!(mapped.keys, vec![10., 15., 20.]);
    assert_eq!(mapped.base_sample_id, table.base_sample_id);
    assert_eq!(
        mapped.samples.iter().map(|s| &s.id).collect::<Vec<_>>(),
        table.samples.iter().map(|s| &s.id).collect::<Vec<_>>()
    );
    roundtrip(&s);
}

#[test]
fn keyform_setter_rejects_duplicate_sample_coordinates() {
    let mut s = base();
    op(
        &mut s,
        BINDING_ID,
        O::AddAxis {
            axis: BindingAxis::new(id(1600), vec![-1., 1.]),
        },
    );
    let f = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .unwrap()
        .keyforms[0]
        .clone();
    let mut invalid = f.clone();
    invalid.keys.clear();
    invalid.samples = vec![f.samples[0].clone(); 2];
    invalid.appearance.opacity = 0.25;
    let before = s.read_snapshot().source;
    assert!(crate::support::prepared(&s, |c| c.set_mesh_keyform(BINDING_ID, invalid)).is_err());
    assert_eq!(s.read_snapshot().source, before);
    let mut valid = f;
    valid.keys.clear();
    valid.samples.reverse();
    valid.appearance.opacity = 0.25;
    let (_, p) = crate::support::prepared(&s, |c| c.set_mesh_keyform(BINDING_ID, valid)).unwrap();
    s.publish(p).unwrap();
    assert_eq!(
        s.snapshot()
            .document()
            .binding(BINDING_ID)
            .unwrap()
            .keyforms[0]
            .appearance
            .opacity,
        0.25
    );
}
