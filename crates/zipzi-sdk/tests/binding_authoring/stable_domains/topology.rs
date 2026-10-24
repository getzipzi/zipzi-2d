use super::*;

#[test]
fn topology_replacement_cannot_revive_deleted_child_handles_in_one_edit() {
    let mut s = base();
    let original = s.binding(BINDING_ID).unwrap();
    let handle = s
        .handle(ObjectKind::BindingSample, &original.axes[0].samples[0].id)
        .unwrap();
    let source = s.geometry(MESH_ID).unwrap();
    let mesh = s.document().get_mesh(MESH_ID).unwrap().clone();
    let replacement = zipzi_sdk::TopologyReplacement {
        vertex_mapping: mesh.vertex_ids.iter().map(|id| (*id, Some(*id))).collect(),
        mesh,
        binding: Some(MeshBinding {
            id: BINDING_ID.into(),
            mesh_id: MESH_ID.into(),
            axes: vec![],
            keyforms: vec![],
        }),
        blend_bindings: vec![],
        glues: vec![],
    };
    let (_, receipt) = s
        .edit("recreate through topology", None, |e| {
            e.replace_topology(&source, replacement)?;
            e.replace_binding(original.clone())
        })
        .unwrap();
    assert!(receipt.changed);
    assert_eq!(s.binding(BINDING_ID).unwrap(), original);
    assert!(s.resolve_handle(&handle).is_err());
}

#[test]
fn topology_conversion_respects_owned_sample_locks() {
    let mut s = base();
    let b = s.binding(BINDING_ID).unwrap();
    let mut flags = s.document().editor_state().clone();
    flags.objects.insert(
        b.axes[0].samples[0].id.clone(),
        zipzi_core::document::ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    s.edit("lock sample", None, |e| e.replace_editor_state(flags))
        .unwrap();
    let source = s.geometry(MESH_ID).unwrap();
    let mesh = s.document().get_mesh(MESH_ID).unwrap().clone();
    let mut changed = b;
    changed.keyforms[0].positions[0].x += 1.;
    let replacement = zipzi_sdk::TopologyReplacement {
        vertex_mapping: mesh.vertex_ids.iter().map(|id| (*id, Some(*id))).collect(),
        mesh,
        binding: Some(changed),
        blend_bindings: vec![],
        glues: vec![],
    };
    let version = s.version();
    assert_eq!(
        s.edit("bypass locked sample", None, |e| e
            .replace_topology(&source, replacement))
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.version(), version);
}

#[test]
fn warp_replacement_tracks_removed_sample_lifetime_and_locks() {
    let mut s = base();
    let transform = Transform {
        id: id(2120),
        data: TransformData::Warp(WarpTransform {
            rows: 1,
            columns: 1,
            quad: true,
            points: vec![
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(0., 1.),
                Vec2::new(1., 1.),
            ],
            bezier: None,
        }),
        ..Default::default()
    };
    let binding = SceneBinding {
        id: id(2121),
        axes: vec![BindingAxis::new("", vec![0.])],
        track: SceneTrack::Warp {
            target_id: transform.id.clone().into(),
            keyforms: vec![],
        },
    };
    s.edit("warp fixture", None, |e| {
        e.create_transform(transform.clone())?;
        e.create_scene_binding(binding.clone())
    })
    .unwrap();
    let transform = s.document().get_transform(&transform.id).unwrap().clone();
    let sample = &binding.axes[0].samples[0].id;
    let handle = s.handle(ObjectKind::BindingSample, sample).unwrap();
    let mut flags = s.document().editor_state().clone();
    flags.objects.insert(
        sample.clone(),
        zipzi_core::document::ObjectEditorState {
            locked: true,
            ..Default::default()
        },
    );
    s.edit("lock sample", None, |e| e.replace_editor_state(flags))
        .unwrap();
    let mut empty = binding.clone();
    empty.axes.clear();
    let version = s.version();
    assert_eq!(
        s.edit("bypass warp sample lock", None, |e| e
            .replace_warp_authoring(transform.clone(), Some(empty.clone())))
            .unwrap_err()
            .code
            .as_ref(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.version(), version);
    s.undo().unwrap();
    s.edit("remove and restore warp sample", None, |e| {
        e.replace_warp_authoring(transform.clone(), Some(empty))?;
        e.replace_warp_authoring(transform, Some(binding.clone()))
    })
    .unwrap();
    assert!(s.resolve_handle(&handle).is_err());
    assert_eq!(s.document().get_scene_binding(&binding.id), Some(&binding));
}
