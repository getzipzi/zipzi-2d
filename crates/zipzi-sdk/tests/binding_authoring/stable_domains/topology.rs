use super::*;
use zipzi_core::BindingAxisEditing;

#[test]
fn restoring_binding_in_one_draft_is_an_exact_noop() {
    let mut s = base();
    let original = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap();
    let handle = {
        let kind = ObjectKind::BindingSample;
        let object_id = (&original.axes[0].samples[0].id).to_owned();
        s.snapshot()
            .document()
            .require_object(kind, &object_id)
            .map(|()| (kind, object_id, s.read_snapshot().source))
    }
    .unwrap();
    let replacement = MeshBinding {
        id: BINDING_ID.into(),
        mesh_id: MESH_ID.into(),
        axes: vec![],
        keyforms: vec![],
    };
    let (_, receipt) = {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.replace_binding(replacement)?;
                e.replace_binding(original.clone())
            })
            .and_then(|value| {
                let prepared = draft.prepare("recreate through topology")?;
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
        original
    );
    assert_eq!(s.read_snapshot().source, handle.2);
}

#[test]
fn topology_conversion_respects_owned_sample_locks() {
    let mut s = base();
    let b = s
        .snapshot()
        .document()
        .binding(BINDING_ID)
        .cloned()
        .unwrap();
    let mut flags = s.snapshot().document().editor_state().clone();
    flags.objects.insert(
        b.axes[0].samples[0].id.clone(),
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
                let prepared = draft.prepare("lock sample")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let mut changed = b;
    changed.keyforms[0].positions[0].x += 1.;
    let version = s.version();
    assert_eq!(
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_binding(changed))
                .and_then(|value| {
                    let prepared = draft.prepare("bypass locked sample")?;
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
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                crate::support::insert_transform(e, transform.clone())?;
                e.create_scene_binding(binding.clone())
            })
            .and_then(|value| {
                let prepared = draft.prepare("warp fixture")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let binding = s
        .snapshot()
        .document()
        .scene_binding(&binding.id)
        .unwrap()
        .clone();
    let sample = &binding.axes[0].samples[0].id;
    let mut flags = s.snapshot().document().editor_state().clone();
    flags.objects.insert(
        sample.clone(),
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
                let prepared = draft.prepare("lock sample")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let mut empty = binding.clone();
    empty.axes.clear();
    if let SceneTrack::Warp { keyforms, .. } = &mut empty.track {
        keyforms.clear();
    }
    let version = s.version();
    assert_eq!(
        {
            let mut draft = s.begin_draft();
            draft
                .batch(draft.source(), |e| e.replace_scene_binding(empty.clone()))
                .and_then(|value| {
                    let prepared = draft.prepare("bypass warp sample lock")?;
                    s.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap_err()
        .code
        .as_str(),
        "OBJECT_LOCKED"
    );
    assert_eq!(s.version(), version);
    s.undo(s.version()).unwrap();
    let source = s.read_snapshot().source;
    {
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |e| {
                e.replace_scene_binding(empty)?;
                e.replace_scene_binding(binding.clone())
            })
            .and_then(|value| {
                let prepared = draft.prepare("remove and restore warp sample")?;
                s.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(s.read_snapshot().source, source);
    assert_eq!(
        s.snapshot().document().scene_binding(&binding.id),
        Some(&binding)
    );
}
