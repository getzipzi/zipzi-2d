use super::*;
use zipzi_core::BindingAxisEditing;

#[test]
fn stable_axis_sample_ids_roundtrip_and_old_addresses_do_not_revive() {
    let mut s = base();
    let old = s.binding(BINDING_ID).unwrap();
    let axis = &old.axes[0];
    let sample = axis.samples[0].id.clone();
    let sample_handle = s.handle(ObjectKind::BindingSample, &sample).unwrap();
    let axis_handle = s.handle(ObjectKind::BindingAxis, &axis.id).unwrap();
    op(
        &mut s,
        BINDING_ID,
        O::AddAxis {
            axis: BindingAxis::new(id(1600), vec![-1., 1.]),
        },
    );
    let b = s.binding(BINDING_ID).unwrap();
    let address = s.binding_field_address(&sample).unwrap();
    op(
        &mut s,
        BINDING_ID,
        O::ReorderAxes {
            axis_ids: vec![b.axes[1].id.clone(), b.axes[0].id.clone()],
        },
    );
    let p = s
        .plan_binding_field_patch(&address, BindingFieldPatch::SampleValue(0.1))
        .unwrap();
    let receipt = s.execute_binding_plan(&p).unwrap();
    assert!(receipt
        .changes
        .modified
        .iter()
        .any(|e| e.kind == ObjectKind::BindingSample && e.id == sample));
    assert_eq!(
        s.document().get_binding(BINDING_ID).unwrap().axes[1].samples[0].id,
        sample
    );
    let loaded = roundtrip(&s);
    assert_eq!(
        loaded.get_binding(BINDING_ID),
        s.document().get_binding(BINDING_ID)
    );
    assert!(s
        .plan_binding_field_patch(&address, BindingFieldPatch::SampleValue(0.2))
        .is_err());
    let address = s.binding_field_address(&sample).unwrap();
    op(
        &mut s,
        BINDING_ID,
        O::SetParameter {
            axis_id: axis.id.clone(),
            parameter_id: String::new(),
            reduction: Some(R::Slice(0.1)),
        },
    );
    s.undo().unwrap();
    s.resolve_handle(&sample_handle).unwrap();
    s.resolve_handle(&axis_handle).unwrap();
    assert_eq!(
        s.plan_binding_field_patch(&address, BindingFieldPatch::SampleValue(0.2))
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_FIELD_ADDRESS"
    );
    let b = s.binding(BINDING_ID).unwrap();
    let axis = b.axes.iter().find(|a| a.id == axis.id).unwrap();
    op(
        &mut s,
        BINDING_ID,
        O::RemoveAxis {
            axis_id: axis.id.clone(),
            reduction: Some(R::Slice(0.1)),
            keep_pending: false,
        },
    );
    s.undo().unwrap();
    assert!(s.resolve_handle(&sample_handle).is_err());
    assert!(s.resolve_handle(&axis_handle).is_err());
    assert!(s.document().binding_identity(&sample).is_some());
    let _ = address;
}

#[test]
fn driver_remapping_is_atomic_and_preserves_sample_identity() {
    let mut s = base();
    s.edit("new domain", None, |e| {
        e.create_parameter(Parameter {
            id: id(1910),
            minimum: 10.,
            maximum: 20.,
            default_value: 15.,
            ..Default::default()
        })
    })
    .unwrap();
    let old = s.binding(BINDING_ID).unwrap();
    op(
        &mut s,
        BINDING_ID,
        O::RemapParameter {
            axis_id: old.axes[0].id.clone(),
            parameter_id: id(1910),
            policy: zipzi_sdk::ParameterRangePolicy::MapKeyValues,
        },
    );
    let new = s.binding(BINDING_ID).unwrap();
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
    s.edit("glue", None, |e| {
        e.create_glue(Glue {
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
        })
    })
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
    let old = s.binding(BINDING_ID).unwrap();
    let mut numeric = old.clone();
    for axis in &mut numeric.axes {
        axis.id.clear();
        axis.samples.clear();
    }
    for form in &mut numeric.keyforms {
        form.samples.clear();
    }
    let (_, receipt) = s
        .edit("numeric import adapter", None, |e| {
            e.replace_binding(numeric)
        })
        .unwrap();
    assert!(!receipt.changed);
    assert_eq!(s.binding(BINDING_ID).unwrap(), old);
    let mut d = s.document().fork_candidate();
    let mut a = d.get_asset(ASSET_ID).unwrap().clone();
    a.content.as_mut().unwrap().source = "assets/texture.png".into();
    assert!(d.replace_asset(a).status.is_ok());
    let wire: serde_json::Value =
        serde_json::from_str(&zipzi_project::encode_project(&d).unwrap()).unwrap();
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
            zipzi_project::decode_project(&invalid.to_string()).is_err(),
            "{path}"
        );
    }
    let mut invalid = wire.clone();
    invalid["document"]["glues"][0]["binding"]["keyforms"][0]["samples"] = serde_json::json!([]);
    assert!(zipzi_project::decode_project(&invalid.to_string()).is_err());
    assert!(s
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
    let source = s.binding(BINDING_ID).unwrap();
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
    s.edit("stable pose", None, |e| {
        e.set_mesh_keyform(BINDING_ID, form)
    })
    .unwrap();
    let after = s.binding(BINDING_ID).unwrap();
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
    let table = s.document().get_blend_key_table(&id(1701)).unwrap().clone();
    let address = s.binding_field_address(&table.samples[0].id).unwrap();
    let mut parameter = s
        .document()
        .get_parameter(&table.parameter_id)
        .unwrap()
        .clone();
    parameter.minimum = -2.;
    parameter.maximum = 2.;
    s.edit("domain", None, |e| e.replace_parameter(parameter))
        .unwrap();
    s.undo().unwrap();
    assert_eq!(
        s.plan_binding_field_patch(&address, BindingFieldPatch::SampleValue(-0.8))
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_FIELD_ADDRESS"
    );
    s.edit("new blend driver", None, |e| {
        e.create_parameter(Parameter {
            id: id(1991),
            kind: ParameterKind::BlendShape,
            minimum: 10.,
            maximum: 20.,
            default_value: 15.,
            ..Default::default()
        })
    })
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
    let mapped = s.document().get_blend_key_table(&table.id).unwrap();
    assert_eq!(mapped.keys, vec![10., 15., 20.]);
    assert_eq!(mapped.base_sample_id, table.base_sample_id);
    assert_eq!(
        mapped.samples.iter().map(|s| &s.id).collect::<Vec<_>>(),
        table.samples.iter().map(|s| &s.id).collect::<Vec<_>>()
    );
    roundtrip(&s);
}

#[test]
fn property_patch_rejects_duplicate_sample_coordinates() {
    let mut s = base();
    op(
        &mut s,
        BINDING_ID,
        O::AddAxis {
            axis: BindingAxis::new(id(1600), vec![-1., 1.]),
        },
    );
    let b = s.binding(BINDING_ID).unwrap();
    let f = &b.keyforms[0];
    let patch = zipzi_core::document::PropertyPatch::Mesh {
        id: MESH_ID.into(),
        keyform: Some((BINDING_ID.into(), vec![f.samples[0].clone(); 2])),
        appearance: Appearance {
            opacity: 0.25,
            ..f.appearance
        },
        draw_order: f.draw_order,
    };
    let version = s.version();
    assert_eq!(
        s.edit("invalid coordinate", None, |e| e.patch_property(&patch))
            .unwrap_err()
            .code
            .as_ref(),
        "MISSING_PROPERTY_TARGET"
    );
    assert_eq!(s.version(), version);
    let mut patch = patch;
    if let zipzi_core::document::PropertyPatch::Mesh { keyform, .. } = &mut patch {
        *keyform = Some((BINDING_ID.into(), f.samples.iter().rev().cloned().collect()));
    }
    s.edit("unordered stable coordinate", None, |e| {
        e.patch_property(&patch)
    })
    .unwrap();
    assert_eq!(
        s.binding(BINDING_ID).unwrap().keyforms[0]
            .appearance
            .opacity,
        0.25
    );
}
