use super::*;
use crate::{preview::PreviewState, Appearance, ModelSetup, Parameter, Vec2};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn baseline() -> Document {
    let mut document = Document::new();
    assert!(document.initialize(id(1), ModelSetup::default()).is_ok());
    for n in [2, 3] {
        assert!(document
            .create_mesh(Mesh {
                id: id(n),
                name: "Mesh".into(),
                vertex_ids: vec![1, 2, 3],
                base_positions: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                triangles: vec![[1, 2, 3]],
                ..Default::default()
            })
            .status
            .is_ok());
    }
    assert!(document.validate_structure().is_empty());
    document
}

/// Shadow the write journal against the entire persistent document. Removing
/// the touched records must remove every content difference, including flags.
fn assert_complete(before: &Document, after: &Document, ids: &[u32]) {
    assert_records(
        before,
        after,
        ids.iter().map(|n| TrackedRecord::Mesh(id(*n))).collect(),
    );
}

fn assert_records(before: &Document, after: &Document, expected: BTreeSet<TrackedRecord>) {
    let records = after
        .tracked_records_since(before)
        .expect("complete coverage");
    assert_eq!(*records, expected);
    let mut restored = after.clone();
    for record in records {
        match record {
            TrackedRecord::Mesh(id) => {
                restored
                    .content
                    .meshes
                    .insert(id.clone(), before.get_mesh(id).unwrap().clone());
            }
            TrackedRecord::MeshBinding(id) => {
                restored
                    .content
                    .bindings
                    .insert(id.clone(), before.get_binding(id).unwrap().clone());
            }
        }
    }
    assert!(
        restored.same_content(before),
        "unrecorded persistent difference"
    );
    // Validate independently: do not populate a proof used by the candidate.
    assert_eq!(
        after.clone().validate_structure(),
        after.validate_structure_since(before)
    );
}

fn scene_patch(document: &Document, n: u32, value: SceneFieldValue) -> ScenePlan {
    document
        .plan_scene_edit(SceneEdit::Patch {
            id: id(n),
            expected: document.scene_field(&id(n), value.field()).unwrap(),
            value,
            clear_glue_pairs: false,
        })
        .unwrap()
}

fn bound_baseline() -> Document {
    let mut d = baseline();
    assert!(d
        .create_parameter(Parameter {
            id: id(4),
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(d
        .create_binding(MeshBinding {
            id: id(5),
            mesh_id: id(2),
            axes: vec![crate::BindingAxis::new(id(4), vec![0., 1.])],
            keyforms: [0., 1.]
                .into_iter()
                .map(|key| crate::MeshKeyform {
                    keys: vec![key],
                    positions: d.get_mesh(&id(2)).unwrap().base_positions.clone(),
                    ..Default::default()
                })
                .collect(),
        })
        .status
        .is_ok());
    assert!(d.validate_structure().is_empty());
    d
}

#[test]
fn binding_payload_installations_track_canonical_records_and_net_noops() {
    let before = bound_baseline();
    let mut candidate = before.fork_tracked_candidate();
    let mut binding = candidate.get_binding(&id(5)).unwrap().clone();
    binding.keyforms[0].appearance.opacity = 0.4;
    binding.keyforms.reverse(); // Input order is normalized before coverage is decided.
    assert!(candidate.replace_binding(binding).status.is_ok());
    let expected = BTreeSet::from([TrackedRecord::MeshBinding(id(5))]);
    assert_records(&before, &candidate, expected.clone());
    for n in 0..12 {
        let mut form = candidate.get_binding(&id(5)).unwrap().keyforms[0].clone();
        form.positions[0].x = n as f32 / 10.;
        form.draw_order = Some(n as f32);
        assert!(candidate.set_mesh_keyform(&id(5), form).status.is_ok());
        assert_records(&before, &candidate, expected.clone());
    }
    assert!(candidate
        .replace_binding(before.get_binding(&id(5)).unwrap().clone())
        .status
        .is_ok());
    assert!(candidate.same_content(&before));
    assert_records(&before, &candidate, expected);
    assert!(candidate
        .rename_mesh(&id(3), "Mesh label".into())
        .status
        .is_ok());
    assert_records(
        &before,
        &candidate,
        BTreeSet::from([
            TrackedRecord::Mesh(id(3)),
            TrackedRecord::MeshBinding(id(5)),
        ]),
    );
    assert!(candidate.same_authoring_identities(&before));
    assert!(candidate.clone().same_authoring_identities(&before));
}

#[test]
fn binding_axis_sample_and_target_changes_are_sticky_fallbacks() {
    let before = bound_baseline();
    for field in 0..4 {
        let mut candidate = before.fork_tracked_candidate();
        let mut binding = candidate.get_binding(&id(5)).unwrap().clone();
        match field {
            0 => binding.mesh_id = id(3),
            1 => binding.axes[0].id = id(40),
            2 => {
                let old = binding.axes[0].samples[0].id.clone();
                binding.axes[0].samples[0].id = id(41);
                for f in &mut binding.keyforms {
                    for sample in &mut f.samples {
                        if *sample == old {
                            *sample = id(41);
                        }
                    }
                }
            }
            _ => {
                binding.axes[0].set_values(vec![0.2, 1.]);
                for f in &mut binding.keyforms {
                    f.keys.clear();
                }
            }
        }
        assert!(
            candidate.replace_binding(binding).status.is_ok(),
            "field {field}"
        );
        assert!(candidate.tracked_records_since(&before).is_none());
        let mut form = candidate.get_binding(&id(5)).unwrap().keyforms[0].clone();
        form.appearance.opacity = 0.5;
        assert!(candidate.set_mesh_keyform(&id(5), form).status.is_ok());
        assert!(candidate.tracked_records_since(&before).is_none());
    }
}

#[test]
fn binding_preview_failures_restore_coverage_content_and_snapshot_together() {
    let before = bound_baseline();
    let mut candidate = before.fork_tracked_candidate();
    let samples = before.get_binding(&id(5)).unwrap().keyforms[0]
        .samples
        .clone();
    let patch = |opacity, draw_order| PropertyPatch::Mesh {
        id: id(2),
        keyform: Some((id(5), samples.clone())),
        appearance: Appearance {
            opacity,
            ..Default::default()
        },
        draw_order,
    };
    let mut preview = PreviewState::default();
    assert!(candidate
        .preview_property(&patch(0.4, None), &mut preview, Default::default())
        .status
        .is_ok());
    let saved = candidate.clone();
    let snapshot = candidate.snapshot_identity.clone();
    let frame = preview.frame(&candidate, 0).unwrap();
    assert!(!candidate
        .preview_property(&patch(0.6, Some(2.)), &mut preview, [(id(99), 0.)].into())
        .status
        .is_ok());
    assert!(candidate.same_content(&saved));
    assert_eq!(candidate.revision(), saved.revision());
    assert!(Arc::ptr_eq(&snapshot, &candidate.snapshot_identity));
    assert!(Arc::ptr_eq(&frame, &preview.frame(&candidate, 0).unwrap()));
    assert_records(
        &before,
        &candidate,
        BTreeSet::from([TrackedRecord::MeshBinding(id(5))]),
    );
    let mut invalid = candidate.get_binding(&id(5)).unwrap().clone();
    invalid.keyforms[0].positions.pop();
    assert!(!candidate.replace_binding(invalid).status.is_ok());
    assert!(candidate.same_content(&saved));
    assert_records(
        &before,
        &candidate,
        BTreeSet::from([TrackedRecord::MeshBinding(id(5))]),
    );
    assert!(candidate
        .preview_property(&patch(0.7, None), &mut preview, Default::default())
        .status
        .is_ok());
    assert_records(
        &before,
        &candidate,
        BTreeSet::from([TrackedRecord::MeshBinding(id(5))]),
    );
}

#[test]
fn binding_locks_reject_all_entries_and_editor_state_cleanup_loses_coverage() {
    for locked in [2, 5] {
        let mut before = bound_baseline();
        let mut flags = before.editor_state().clone();
        flags.objects.insert(
            id(locked),
            ObjectEditorState {
                locked: true,
                hidden_in_editor: false,
            },
        );
        assert!(before.replace_editor_state(flags).status.is_ok());
        let mut candidate = before.fork_tracked_candidate();
        let mut binding = candidate.get_binding(&id(5)).unwrap().clone();
        binding.keyforms[0].appearance.opacity = 0.4;
        assert!(!candidate.replace_binding(binding.clone()).status.is_ok());
        assert!(!candidate
            .set_mesh_keyform(&id(5), binding.keyforms[0].clone())
            .status
            .is_ok());
        assert!(!candidate
            .patch_property(&PropertyPatch::Mesh {
                id: id(2),
                keyform: Some((id(5), binding.keyforms[0].samples.clone())),
                appearance: binding.keyforms[0].appearance,
                draw_order: None
            })
            .status
            .is_ok());
        assert_complete(&before, &candidate, &[]);
    }
    // Imported raw content has no validation certificate.
    let mut before = bound_baseline().clone();
    before.content.editor_state.objects.insert(
        id(99),
        ObjectEditorState {
            locked: true,
            hidden_in_editor: false,
        },
    );
    let mut candidate = before.fork_tracked_candidate();
    let mut binding = candidate.get_binding(&id(5)).unwrap().clone();
    binding.keyforms[0].appearance.opacity = 0.4;
    assert!(candidate.replace_binding(binding).status.is_ok());
    assert!(candidate.tracked_records_since(&before).is_none());
}

#[test]
fn covered_installations_collect_distinct_records_and_preserve_net_noops() {
    let before = baseline();
    let mut candidate = before.fork_tracked_candidate();
    assert_complete(&before, &candidate, &[]);
    for n in 0..100 {
        assert!(candidate
            .rename_mesh(&id(2), format!("Mesh {n}"))
            .status
            .is_ok());
        assert!(candidate
            .set_mesh_opacity(&id(2), n as f32 / 100.)
            .status
            .is_ok());
    }
    let mut mesh = candidate.get_mesh(&id(2)).unwrap().clone();
    mesh.enabled = false;
    assert!(candidate.replace_mesh(mesh).status.is_ok());
    for value in [
        SceneFieldValue::Label("Label plan".into()),
        SceneFieldValue::Enabled(true),
        SceneFieldValue::Appearance(Appearance {
            opacity: 0.4,
            ..Default::default()
        }),
    ] {
        let plan = scene_patch(&candidate, 2, value);
        assert_complete(&before, &candidate, &[2]); // Planning is read-only.
        assert!(candidate.apply_scene_plan(&plan).status.is_ok());
        assert_complete(&before, &candidate, &[2]);
    }
    assert!(candidate
        .replace_mesh(before.get_mesh(&id(2)).unwrap().clone())
        .status
        .is_ok());
    assert!(candidate.same_content(&before));
    assert_complete(&before, &candidate, &[2]);
    assert!(candidate.rename_mesh(&id(3), "Other".into()).status.is_ok());
    assert_complete(&before, &candidate, &[2, 3]);
}

#[test]
fn unsupported_writes_are_sticky_in_both_operation_orders() {
    let before = baseline();
    for rename_first in [false, true] {
        let mut candidate = before.fork_tracked_candidate();
        if rename_first {
            assert!(candidate
                .rename_mesh(&id(2), "Before".into())
                .status
                .is_ok());
        }
        assert!(candidate
            .create_parameter(Parameter {
                id: id(4),
                minimum: 0.,
                maximum: 1.,
                ..Default::default()
            })
            .status
            .is_ok());
        assert!(candidate.tracked_records_since(&before).is_none());
        assert!(candidate.rename_mesh(&id(2), "After".into()).status.is_ok());
        assert!(candidate.tracked_records_since(&before).is_none());
    }
    for field in 0..4 {
        let mut candidate = before.fork_tracked_candidate();
        let mut mesh = candidate.get_mesh(&id(2)).unwrap().clone();
        match field {
            0 => mesh.draw_order = Some(2.),
            1 => mesh.runtime_id = "different-runtime-id".into(),
            2 => mesh.masks = vec![id(3)],
            _ => mesh.base_positions[0].x = -1.,
        }
        assert!(candidate.replace_mesh(mesh).status.is_ok());
        assert!(candidate.tracked_records_since(&before).is_none());
    }
    // A field plan preserving identities still has to respect the field boundary.
    let mut candidate = before.fork_tracked_candidate();
    let plan = scene_patch(&candidate, 2, SceneFieldValue::Masks(vec![id(3)]));
    assert!(plan.preserves_identities());
    assert!(candidate.apply_scene_plan(&plan).status.is_ok());
    assert!(candidate.tracked_records_since(&before).is_none());
}

#[test]
fn tracked_identity_answer_matches_untracked_comparison_across_snapshot_boundaries() {
    let before = baseline();
    let mut candidate = before.fork_tracked_candidate();
    assert!(candidate.rename_mesh(&id(2), "Label".into()).status.is_ok());
    assert!(candidate.set_mesh_opacity(&id(3), 0.5).status.is_ok());
    let reference = candidate.clone().same_authoring_identities(&before);
    #[cfg(feature = "bench-instrumentation")]
    let capture = crate::benchmark::Capture::start();
    assert_eq!(candidate.same_authoring_identities(&before), reference);
    #[cfg(feature = "bench-instrumentation")]
    {
        let work = capture.finish();
        assert!(!work.contains_key("core.authoring_identity_scan"));
        assert_eq!(work["core.tracking.record_reads"].calls, 4);
    }
    let mut branch = before.fork_tracked_candidate();
    assert!(branch
        .rename_mesh(&id(2), "Other branch".into())
        .status
        .is_ok());
    let reference = candidate.clone().same_authoring_identities(&branch);
    #[cfg(feature = "bench-instrumentation")]
    let capture = crate::benchmark::Capture::start();
    assert_eq!(candidate.same_authoring_identities(&branch), reference);
    #[cfg(feature = "bench-instrumentation")]
    assert_eq!(capture.finish()["core.authoring_identity_scan"].calls, 1);
    assert!(candidate
        .create_mesh(Mesh {
            id: id(4),
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(!candidate.same_authoring_identities(&before));
}

#[test]
fn exact_snapshot_identity_rejects_equal_revision_branches_and_wrong_endpoints() {
    let before = baseline();
    let mut a = before.fork_tracked_candidate();
    let mut b = before.fork_tracked_candidate();
    assert!(a.rename_mesh(&id(2), "Branch A".into()).status.is_ok());
    assert!(b.rename_mesh(&id(2), "Branch B".into()).status.is_ok());
    assert_eq!(a.revision(), b.revision());
    let c = a.fork_tracked_candidate();
    assert_complete(&a, &c, &[]);
    assert!(c.tracked_records_since(&b).is_none());
    a.snapshot_identity = Arc::new(());
    assert!(a.tracked_records_since(&before).is_none());
    assert!(a.set_mesh_opacity(&id(2), 0.4).status.is_ok());
    assert!(a.tracked_records_since(&before).is_none());
}

#[test]
fn clone_restore_checkpoint_and_publication_do_not_carry_transaction_coverage() {
    let before = baseline();
    let mut candidate = before.fork_tracked_candidate();
    assert!(candidate
        .rename_mesh(&id(2), "Renamed".into())
        .status
        .is_ok());
    let mut clone = candidate.clone();
    clone.content.meshes.get_mut(&id(3)).unwrap().enabled = false;
    assert!(clone.set_mesh_opacity(&id(2), 0.5).status.is_ok());
    assert!(clone.tracked_records_since(&before).is_none());
    assert!(candidate
        .fork_candidate()
        .tracked_records_since(&candidate)
        .is_none());
    clone.restore_from(&candidate);
    assert!(clone.tracked_records_since(&before).is_none());
    let mut checkpoint = before.checkpoint();
    candidate.exchange_checkpoint(&mut checkpoint).unwrap();
    assert!(candidate.tracked_records_since(&before).is_none());
    candidate.exchange_checkpoint(&mut checkpoint).unwrap();
    assert!(candidate.tracked_records_since(&before).is_none());
    let mut live = before.clone();
    let next = live.fork_tracked_candidate();
    assert!(live
        .publish_candidate(candidate, ChangeKind::Metadata, false)
        .unwrap());
    assert!(live.tracked_records_since(&before).is_none());
    assert!(next.tracked_records_since(&live).is_none());
    assert_complete(&live, &live.fork_tracked_candidate(), &[]);
}

#[test]
fn batch_build_and_relocated_checkpoints_require_a_new_tracking_baseline() {
    let mut document = Document::new();
    assert!(document.begin_batch_build().is_ok());
    assert!(document.initialize(id(1), ModelSetup::default()).is_ok());
    assert!(document
        .create_mesh(Mesh {
            id: id(2),
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(document.finish_batch_build().is_ok());
    assert!(document.tracked_records_since(&document).is_none());
    let asset = ImageAsset {
        id: id(4),
        name: "Asset".into(),
        unresolved_source: None,
        content: Some(crate::ImageContent {
            source: "/before.png".into(),
            width: 2,
            height: 2,
            sha256: String::new(),
        }),
    };
    assert!(document.add_asset(asset.clone()).status.is_ok());
    assert!(document.validate_structure().is_empty());
    let mut checkpoint = document.checkpoint();
    assert!(checkpoint.relocate_asset_storage(&asset, "/relocated.png".into(), "hash".into()));
    let mut candidate = document.fork_tracked_candidate();
    assert_complete(&document, &candidate, &[]);
    candidate.exchange_checkpoint(&mut checkpoint).unwrap();
    assert!(candidate.tracked_records_since(&document).is_none());
    assert!(!candidate.structure_is_validated());
    let next = candidate.fork_tracked_candidate();
    assert_complete(&candidate, &next, &[]);
}

#[test]
fn preview_rollback_restores_coverage_even_after_a_temporary_unsupported_write() {
    let before = baseline();
    let mut candidate = before.fork_tracked_candidate();
    let patch = |opacity, draw_order| PropertyPatch::Mesh {
        id: id(2),
        keyform: None,
        appearance: Appearance {
            opacity,
            ..Default::default()
        },
        draw_order,
    };
    let mut preview = PreviewState::default();
    assert!(candidate
        .preview_property(&patch(0.4, None), &mut preview, Default::default())
        .status
        .is_ok());
    let snapshot = candidate.snapshot_identity.clone();
    let revision = candidate.revision();
    let evaluation_revision = candidate.evaluation_revision;
    let frame = preview.frame(&candidate, 0).unwrap();
    // The draw-order edit makes coverage Unknown, then evaluation rejects an
    // unknown parameter. Recovery must restore the prior Complete state too.
    assert!(!candidate
        .preview_property(&patch(0.6, Some(2.)), &mut preview, [(id(99), 0.)].into())
        .status
        .is_ok());
    assert!(Arc::ptr_eq(&snapshot, &candidate.snapshot_identity));
    assert_eq!(candidate.revision(), revision);
    assert_eq!(candidate.evaluation_revision, evaluation_revision);
    assert!(Arc::ptr_eq(&frame, &preview.frame(&candidate, 0).unwrap()));
    assert_eq!(candidate.get_mesh(&id(2)).unwrap().appearance.opacity, 0.4);
    assert_eq!(candidate.get_mesh(&id(2)).unwrap().draw_order, None);
    assert_complete(&before, &candidate, &[2]);
    assert!(candidate
        .preview_property(&patch(0.7, None), &mut preview, Default::default())
        .status
        .is_ok());
    assert_complete(&before, &candidate, &[2]);
    assert!(candidate
        .patch_property(&patch(0.7, Some(2.)))
        .status
        .is_ok());
    assert!(candidate.tracked_records_since(&before).is_none());
    assert!(!candidate
        .preview_property(&patch(0.5, Some(3.)), &mut preview, [(id(99), 0.)].into())
        .status
        .is_ok());
    assert!(candidate.tracked_records_since(&before).is_none());
}

#[test]
fn rejected_preview_restores_pruned_imported_flags_before_restoring_coverage() {
    // Drop the valid fixture's certificate before constructing invalid import
    // data, just as raw planner/import clones do in production.
    let mut before = baseline().clone();
    before.content.editor_state.objects.insert(
        id(99),
        ObjectEditorState {
            locked: true,
            hidden_in_editor: false,
        },
    );
    let mut candidate = before.fork_tracked_candidate();
    let result = candidate.preview_property(
        &PropertyPatch::Mesh {
            id: id(2),
            keyform: None,
            appearance: Appearance::default(),
            draw_order: Some(2.),
        },
        &mut PreviewState::default(),
        [(id(100), 0.)].into(),
    );
    assert!(!result.status.is_ok());
    assert!(candidate.same_content(&before));
    assert_complete(&before, &candidate, &[]);
}

#[test]
fn failures_and_noop_plans_leave_coverage_intact_but_cleanup_invalidates_it() {
    let mut before = baseline();
    let mut state = before.editor_state().clone();
    state.objects.insert(
        id(2),
        ObjectEditorState {
            locked: true,
            hidden_in_editor: false,
        },
    );
    assert!(before.replace_editor_state(state).status.is_ok());
    let mut candidate = before.fork_tracked_candidate();
    assert!(!candidate
        .rename_mesh(&id(2), "Locked".into())
        .status
        .is_ok());
    assert!(!candidate.set_mesh_opacity(&id(3), f32::NAN).status.is_ok());
    assert_complete(&before, &candidate, &[]);
    let stale = scene_patch(&candidate, 3, SceneFieldValue::Label("Plan".into()));
    assert!(candidate
        .rename_mesh(&id(3), "Changed".into())
        .status
        .is_ok());
    assert!(!candidate.apply_scene_plan(&stale).status.is_ok());
    assert_complete(&before, &candidate, &[3]);
    let noop = scene_patch(&candidate, 3, SceneFieldValue::Label("Changed".into()));
    assert!(candidate.apply_scene_plan(&noop).status.is_ok());
    assert_complete(&before, &candidate, &[3]);

    // Imported invalid editor state can be cleaned by changed(Structure).
    // That cleanup is an additional persistent write, outside Mesh coverage.
    let mut before = before.clone();
    before.content.editor_state.objects.insert(
        id(99),
        ObjectEditorState {
            locked: true,
            hidden_in_editor: false,
        },
    );
    let mut candidate = before.fork_tracked_candidate();
    let mut mesh = candidate.get_mesh(&id(3)).unwrap().clone();
    mesh.enabled = false;
    assert!(candidate.replace_mesh(mesh).status.is_ok());
    assert!(!candidate.content.editor_state.objects.contains_key(&id(99)));
    assert!(candidate.tracked_records_since(&before).is_none());
}
