use super::*;
use zipzi_core::{
    document::{CdiParameterEntry, DisplayInfoOrigin, ObjectEditorState},
    BindingAxis, MeshBinding, MeshKeyform, ParameterKind, PartKeyform, SceneBinding, SceneKeyform,
    SceneTrack,
};

fn form(value: f32, shift: f32) -> MeshKeyformSpec {
    MeshKeyformSpec {
        keys: vec![value],
        positions: mesh()
            .base_positions
            .iter()
            .map(|p| Vec2::new(p.x + shift, p.y))
            .collect(),
        opacity: None,
        multiply: None,
        screen: None,
        draw_order: None,
    }
}
fn spec() -> MeshBindingCreateSpec {
    MeshBindingCreateSpec {
        id: Some(id(10)),
        mesh_id: id(4),
        axes: vec![MeshBindingAxisSpec {
            parameter_id: id(2),
            keys: vec![0., 1.],
        }],
        keyforms: vec![form(1., 10.), form(0., 0.)],
    }
}
fn binding() -> MeshBinding {
    MeshBinding {
        id: id(10),
        mesh_id: id(4),
        axes: vec![BindingAxis {
            id: String::new(),
            parameter_id: id(2),
            keys: vec![0., 1.],
            samples: vec![],
        }],
        keyforms: [0., 1.]
            .into_iter()
            .map(|v| MeshKeyform {
                keys: vec![v],
                positions: mesh().base_positions,
                ..Default::default()
            })
            .collect(),
    }
}
fn scene() -> SceneBinding {
    SceneBinding {
        id: id(11),
        axes: vec![BindingAxis {
            id: String::new(),
            parameter_id: id(2),
            keys: vec![0., 1.],
            samples: vec![],
        }],
        track: SceneTrack::Part {
            target_id: id(3).into(),
            keyforms: [0., 1.]
                .into_iter()
                .map(|v| PartKeyform {
                    keys: vec![v],
                    draw_order: v * 100.,
                    ..Default::default()
                })
                .collect(),
        },
    }
}
fn with_bindings() -> Snapshot {
    let mut b = snapshot().document().builder();
    b.create_binding(binding()).unwrap();
    b.create_scene_binding(scene()).unwrap();
    Snapshot::new(b.freeze().unwrap().state, ResourceView::default()).unwrap()
}

#[test]
fn batch_validation_retains_generated_identities_in_the_working_builder() {
    // Raw construction deliberately defers normalization. The batch boundary
    // must keep its result, rather than normalizing only a disposable preview.
    let mut b = snapshot().document().builder();
    b.create_binding(binding()).unwrap();
    b.create_scene_binding(scene()).unwrap();
    b.validate_candidate().unwrap();
    let mesh = b.binding(&id(10)).unwrap().clone();
    let scene = b.scene_binding(&id(11)).unwrap().clone();
    assert!(!mesh.axes[0].samples[0].id.is_empty());
    for _ in 0..2 {
        let preview = b.preview().unwrap();
        assert_eq!(preview.binding(&id(10)), Some(&mesh));
        assert_eq!(preview.scene_binding(&id(11)), Some(&scene));
        b.validate_candidate().unwrap();
    }
    let frozen = b.freeze().unwrap();
    assert_eq!(frozen.state.binding(&id(10)), Some(&mesh));
    assert_eq!(frozen.state.scene_binding(&id(11)), Some(&scene));
}

#[test]
fn numeric_binding_create_and_keyform_edit_share_one_batch_and_history_step() {
    let initial = snapshot();
    let mut session = AuthoringSession::new(initial.clone());
    let mut draft = session.begin_draft();
    let binding_id = draft
        .batch(draft.source(), |c| {
            let id = c.create_mesh_binding_spec(spec())?;
            c.set_mesh_keyform_spec(&id, form(1., 20.))?;
            Ok(id)
        })
        .unwrap();
    let canonical = draft
        .document()
        .unwrap()
        .binding(&binding_id)
        .unwrap()
        .clone();
    assert_eq!(canonical.keyforms[0].keys, [0.]);
    assert_eq!(canonical.keyforms[0].appearance, Default::default());
    assert_eq!(canonical.keyforms[1].positions[0].x, 20.);
    assert_eq!(
        canonical.keyforms[1].samples,
        [canonical.axes[0].samples[1].id.clone()]
    );
    for _ in 0..2 {
        let queried = draft.read_snapshot().unwrap();
        assert_eq!(queried.source, QuerySource::Draft(draft.source()));
        assert_eq!(
            queried.snapshot.document().binding(&binding_id),
            Some(&canonical)
        );
    }
    let prepared = draft.prepare("create and edit binding").unwrap();
    assert_eq!(prepared.writes().records().len(), 1);
    session.publish(prepared).unwrap();
    assert_eq!(
        session.snapshot().document().binding(&binding_id),
        Some(&canonical)
    );
    assert!(initial.document().binding(&binding_id).is_none());
    session.undo(session.version()).unwrap();
    assert_eq!(session.snapshot().content_id(), initial.content_id());
    session.redo(session.version()).unwrap();
    assert_eq!(
        session.snapshot().document().binding(&binding_id),
        Some(&canonical)
    );
}

#[test]
fn numeric_replacement_preserves_identities_and_noop_does_not_publish() {
    let initial = with_bindings();
    let mut session = AuthoringSession::new(initial.clone());
    let mut draft = session.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.replace_binding(binding())?;
            c.replace_scene_binding(scene())
        })
        .unwrap();
    let prepared = draft.prepare("same numeric input").unwrap();
    assert!(prepared.writes().is_empty());
    session.publish(prepared).unwrap();
    assert_eq!(session.snapshot().content_id(), initial.content_id());
    assert_eq!(session.version().revision, 0);
}

#[test]
fn invalid_grid_or_ignored_keyform_error_aborts_the_entire_batch() {
    for invalid_grid in [false, true] {
        let mut session = AuthoringSession::new(snapshot());
        let before = session.snapshot().content_id();
        let mut draft = session.begin_draft();
        let result = draft.batch(draft.source(), |c| {
            c.rename(NameTarget::Mesh(&id(4)), "unpublished".into())?;
            let mut input = spec();
            if invalid_grid {
                input.axes[0].keys = vec![0., 0.];
            }
            let created = c.create_mesh_binding_spec(input);
            if !invalid_grid {
                let mut invalid = form(1., 10.);
                invalid.positions.pop();
                assert!(c.set_mesh_keyform_spec(&created.unwrap(), invalid).is_err());
            } else {
                assert!(created.is_err());
            }
            Ok(())
        });
        assert!(result.is_err());
        assert!(!draft.is_active());
        assert!(draft.prepare("failed").is_err());
        assert_eq!(session.snapshot().content_id(), before);
        assert!(session.take_events().is_empty());
    }
}

#[test]
fn parameter_range_and_kind_revalidate_existing_bindings_at_batch_boundary() {
    for patch in [
        ParameterPatch {
            maximum: Some(0.5),
            ..Default::default()
        },
        ParameterPatch {
            kind: Some(ParameterKind::BlendShape),
            ..Default::default()
        },
    ] {
        let initial = with_bindings();
        let session = AuthoringSession::new(initial.clone());
        let mut draft = session.begin_draft();
        assert!(draft
            .batch(draft.source(), |c| c.patch_parameter(&id(2), patch))
            .is_err());
        assert!(!draft.is_active());
        assert_eq!(session.snapshot().content_id(), initial.content_id());
        assert_eq!(session.version().revision, 0);
    }
}

#[test]
fn binding_edits_honor_target_owner_axis_and_sample_locks() {
    let initial = with_bindings();
    let binding = initial.document().binding(&id(10)).unwrap();
    let scene = initial.document().scene_binding(&id(11)).unwrap();
    for locked in [
        id(3),
        id(4),
        id(10),
        binding.axes[0].id.clone(),
        binding.axes[0].samples[0].id.clone(),
    ] {
        let mut b = initial.document().builder();
        let mut editor = b.editor_state().clone();
        editor.objects.insert(
            locked.clone(),
            ObjectEditorState {
                locked: true,
                ..Default::default()
            },
        );
        b.set_editor_state(editor).unwrap();
        let session = AuthoringSession::new(
            Snapshot::new(b.freeze().unwrap().state, ResourceView::default()).unwrap(),
        );
        for replace in [false, true] {
            let mut draft = session.begin_draft();
            let failure = draft
                .batch(draft.source(), |c| {
                    if replace {
                        c.replace_binding(binding.clone())
                    } else {
                        c.set_mesh_keyform_spec(&id(10), form(1., 5.))
                    }
                })
                .unwrap_err();
            assert_eq!(failure.code, "OBJECT_LOCKED", "{locked}");
            assert!(!draft.is_active());
        }
    }
    for locked in [
        id(3),
        id(11),
        scene.axes[0].id.clone(),
        scene.axes[0].samples[0].id.clone(),
    ] {
        let mut b = initial.document().builder();
        let mut editor = b.editor_state().clone();
        editor.objects.insert(
            locked,
            ObjectEditorState {
                locked: true,
                ..Default::default()
            },
        );
        b.set_editor_state(editor).unwrap();
        let session = AuthoringSession::new(
            Snapshot::new(b.freeze().unwrap().state, ResourceView::default()).unwrap(),
        );
        let mut draft = session.begin_draft();
        assert_eq!(
            draft
                .batch(draft.source(), |c| c.set_scene_keyform(
                    &id(11),
                    SceneKeyform::Part(PartKeyform {
                        keys: vec![1.],
                        draw_order: 500.,
                        ..Default::default()
                    })
                ))
                .unwrap_err()
                .code,
            "OBJECT_LOCKED"
        );
    }
}

#[test]
fn scene_keyform_edit_uses_stable_samples_and_rejects_wrong_track_kind() {
    let initial = with_bindings();
    let mut session = AuthoringSession::new(initial.clone());
    let old = initial.document().scene_binding(&id(11)).unwrap();
    let mut draft = session.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.set_scene_keyform(
                &id(11),
                SceneKeyform::Part(PartKeyform {
                    samples: old.track.sample_ids(1).to_vec(),
                    draw_order: 500.,
                    ..Default::default()
                }),
            )
        })
        .unwrap();
    let prepared = draft.prepare("scene sample").unwrap();
    assert_eq!(prepared.writes().records().len(), 1);
    session.publish(prepared).unwrap();
    let changed = session
        .snapshot()
        .document()
        .scene_binding(&id(11))
        .unwrap();
    assert_eq!(changed.axes, old.axes);
    let SceneTrack::Part { keyforms, .. } = &changed.track else {
        panic!()
    };
    assert_eq!(keyforms[1].draw_order, 500.);
    assert_eq!(keyforms[0].draw_order, 0.);
    let mut draft = session.begin_draft();
    assert!(draft
        .batch(draft.source(), |c| c.set_scene_keyform(
            &id(11),
            SceneKeyform::Rotation(zipzi_core::RotationKeyform {
                keys: vec![1.],
                ..Default::default()
            })
        ))
        .is_err());
    session.undo(session.version()).unwrap();
    assert_eq!(
        session.snapshot().document().scene_binding(&id(11)),
        Some(old)
    );
}

#[test]
fn authored_parameters_update_generated_cdi_but_preserve_imported_scope() {
    for origin in [DisplayInfoOrigin::Generated, DisplayInfoOrigin::Imported] {
        let mut b = snapshot().document().builder();
        let mut display = b.display_info().clone();
        display.origin = origin;
        display.parameters = Some(vec![]);
        b.set_display_info(display).unwrap();
        let session = AuthoringSession::new(
            Snapshot::new(b.freeze().unwrap().state, ResourceView::default()).unwrap(),
        );
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |c| {
                c.create_parameter(Parameter {
                    id: id(20),
                    ..Default::default()
                })
            })
            .unwrap();
        let entries = draft
            .document()
            .unwrap()
            .display_info()
            .parameters
            .as_ref()
            .unwrap();
        if origin == DisplayInfoOrigin::Generated {
            assert!(
                matches!(&entries[..], [CdiParameterEntry::Resolved { parameter_id, group_id: None, .. }] if parameter_id == &id(20))
            );
            let prepared = draft.prepare("parameter and CDI").unwrap();
            assert!(prepared
                .writes()
                .root_changed(zipzi_core::state::RootDomain::DisplayInfo));
        } else {
            assert!(entries.is_empty());
        }
    }
}

#[test]
fn failed_batch_validation_poison_prevents_repair_and_freezing() {
    let mut b = snapshot().document().builder();
    let mut invalid = binding();
    invalid.keyforms.pop();
    b.create_binding(invalid).unwrap();
    let failure = b.validate_candidate().unwrap_err();
    assert_eq!(failure.code, "INCOMPLETE_KEYFORMS");
    assert_eq!(b.remove_binding(&id(10)).unwrap_err().code, failure.code);
    assert_eq!(b.freeze().unwrap_err().code, failure.code);
}
