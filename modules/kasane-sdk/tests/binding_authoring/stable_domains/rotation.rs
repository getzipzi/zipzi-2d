use super::*;

#[test]
fn pending_axis_activation_reads_current_rotation_base_and_keeps_identity() {
    let mut s = base();
    let target = id(1601);
    let binding = id(1602);
    let pending = BindingAxis::new("", vec![0., 1.]);
    let axis = pending.id.clone();
    s.edit("rotation", None, |e| {
        e.create_transform(Transform {
            id: target.clone(),
            ..Default::default()
        })
    })
    .unwrap();
    apply(
        &mut s,
        E::Grid(G::Create {
            id: binding.clone(),
            target: T::Scene(target.clone()),
        }),
    );
    op(&mut s, &binding, O::AddAxis { axis: pending });
    assert!(s
        .document()
        .get_scene_binding(&binding)
        .unwrap()
        .track
        .is_empty());
    let mut t = s.document().get_transform(&target).unwrap().clone();
    t.rotation_mut().unwrap().pose.angle = 42.;
    s.edit("latest base", None, |e| e.replace_transform(t))
        .unwrap();
    op(
        &mut s,
        &binding,
        O::SetParameter {
            axis_id: axis.clone(),
            parameter_id: PARAMETER_ID.into(),
            reduction: None,
        },
    );
    let b = s.document().get_scene_binding(&binding).unwrap();
    assert_eq!(b.axes[0].id, axis);
    assert!(b.track.samples().all(|s| s.rotation.angle == 42.));
    let mut f = match &b.track {
        SceneTrack::Rotation { keyforms, .. } => keyforms[1].clone(),
        _ => panic!(),
    };
    f.rotation.angle = 62.;
    s.edit("pose", None, |e| {
        e.set_scene_keyform(&binding, SceneKeyform::Rotation(f))
    })
    .unwrap();
    op(
        &mut s,
        &binding,
        O::RemoveAxis {
            axis_id: axis.clone(),
            reduction: Some(R::Resample(0.25)),
            keep_pending: true,
        },
    );
    assert_eq!(
        s.document()
            .get_transform(&target)
            .unwrap()
            .rotation()
            .unwrap()
            .pose
            .angle,
        47.
    );
    assert!(s
        .document()
        .get_scene_binding(&binding)
        .unwrap()
        .track
        .is_empty());
    roundtrip(&s);
    op(
        &mut s,
        &binding,
        O::SetParameter {
            axis_id: axis.clone(),
            parameter_id: PARAMETER_ID.into(),
            reduction: None,
        },
    );
    assert!(s
        .document()
        .get_scene_binding(&binding)
        .unwrap()
        .track
        .samples()
        .all(|s| s.rotation.angle == 47.));
}

#[test]
fn rotation_reflection_resampling_rejects_atomically() {
    let mut s = base();
    let t = id(1610);
    let b = id(1611);
    s.edit("rotation", None, |e| {
        e.create_transform(Transform {
            id: t.clone(),
            ..Default::default()
        })
    })
    .unwrap();
    apply(
        &mut s,
        E::Grid(G::Create {
            id: b.clone(),
            target: T::Scene(t),
        }),
    );
    op(
        &mut s,
        &b,
        O::AddAxis {
            axis: BindingAxis::new(PARAMETER_ID, vec![0., 1.]),
        },
    );
    let binding = s.document().get_scene_binding(&b).unwrap();
    let axis = binding.axes[0].id.clone();
    let mut f = match &binding.track {
        SceneTrack::Rotation { keyforms, .. } => keyforms[1].clone(),
        _ => panic!(),
    };
    f.rotation.reflect_x = true;
    s.edit("reflect", None, |e| {
        e.set_scene_keyform(&b, SceneKeyform::Rotation(f))
    })
    .unwrap();
    let v = s.version();
    let h = s.history_state();
    let error = s
        .plan_binding_authoring(E::Grid(G::Edit {
            owner: b,
            operation: O::InsertSample {
                axis_id: axis,
                id: id(1612),
                value: 0.5,
            },
        }))
        .unwrap_err();
    assert_eq!(error.code.as_ref(), "DISCRETE_ROTATION_NOT_REPRESENTABLE");
    assert_eq!(s.version(), v);
    assert_eq!(s.history_state(), h);
}
