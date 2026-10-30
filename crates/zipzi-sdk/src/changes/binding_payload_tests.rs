use super::mesh_tests::{fixture as mesh_fixture, full_summary, id};
use super::*;
use crate::changes::ChangeSetExt;
use crate::{AuthoringSession, EditSession};
use zipzi_core::{document::*, preview::PreviewState};

fn fixture(scale: u32) -> AuthoringSession {
    let mut sdk = mesh_fixture(scale);
    sdk.history_limits.max_steps = 256;
    sdk
}

/// Actual property writes use set_mesh_keyform; scalar previews use PropertyPatch.
/// Other paths cover direct replacement, positions, draw order and normalization.
fn apply(edit: &mut EditSession<'_>, path: u32, opacity: f32) {
    let mut binding = edit
        .candidate_document()
        .get_binding(&id(8))
        .unwrap()
        .clone();
    let mut form = binding.keyforms[0].clone();
    form.appearance.opacity = opacity;
    match path {
        0 | 5 | 6 => {
            if path == 5 {
                form.positions[0].x += 0.25;
                form.appearance.multiply = [0.5, 0.6, 0.7];
                form.appearance.screen = [0.1, 0.2, 0.3];
            }
            binding.keyforms[0] = form;
            if path == 6 {
                binding.keyforms.reverse();
            }
            edit.replace_binding(binding).unwrap();
        }
        1 | 4 => {
            if path == 4 {
                form.draw_order = Some(7.);
            }
            edit.set_mesh_keyform(&id(8), form).unwrap();
        }
        2 | 3 => {
            let patch = PropertyPatch::Mesh {
                id: id(2),
                keyform: Some((id(8), form.samples)),
                appearance: form.appearance,
                draw_order: form.draw_order,
            };
            if path == 2 {
                edit.patch_property(&patch).unwrap();
            } else {
                edit.preview_property(&patch, &mut PreviewState::default(), Default::default())
                    .unwrap();
            }
        }
        _ => unreachable!(),
    }
}

fn check(
    sdk: &mut AuthoringSession,
    edit: EditSession<'_>,
    records: Option<BTreeSet<TrackedRecord>>,
) -> crate::EditReceipt {
    assert_eq!(
        edit.candidate_document()
            .tracked_records_since(sdk.document())
            .cloned(),
        records
    );
    let full = if sdk.document().same_content(edit.candidate_document()) {
        ChangeSet::default()
    } else {
        full_summary(sdk, &edit)
    };
    let history = sdk.history_lengths();
    let version = sdk.version();
    #[cfg(feature = "bench-instrumentation")]
    let capture = zipzi_core::benchmark::Capture::start();
    let receipt = edit.commit_to(sdk).unwrap();
    #[cfg(feature = "bench-instrumentation")]
    {
        let work = capture.finish();
        let count = |key| work.get(key).map_or(0, |v| v.calls);
        assert_eq!(count("sdk.publication.local"), u64::from(records.is_some()));
        assert_eq!(
            count("sdk.publication.fallback"),
            u64::from(records.is_none())
        );
        if records.is_some() {
            for key in [
                "sdk.keys_in",
                "core.authoring_identity_scan",
                "sdk.summarize.fallback",
            ] {
                assert_eq!(count(key), 0, "{work:?}");
            }
        }
    }
    assert_eq!(
        serde_json::to_value(&receipt.changes).unwrap(),
        serde_json::to_value(full).unwrap()
    );
    let events = sdk.drain_events();
    assert_eq!(events.len(), usize::from(receipt.changed));
    if receipt.changed {
        assert_eq!(events[0], receipt);
        assert_eq!(sdk.history_lengths(), (history.0 + 1, 0));
    } else {
        assert_eq!(sdk.version(), version);
        assert_eq!(sdk.history_lengths(), history);
    }
    receipt
}

fn binding_record() -> Option<BTreeSet<TrackedRecord>> {
    Some(BTreeSet::from([TrackedRecord::MeshBinding(id(8))]))
}

#[test]
fn keyform_payload_summaries_match_full_results_epochs_and_history_for_every_entry() {
    let mut sdk = fixture(3);
    let retained = sdk.read_snapshot();
    for path in 0..7 {
        let binding = sdk.document().get_binding(&id(8)).unwrap();
        let samples = binding.keyforms[0].samples.clone();
        let axis = sdk
            .handle(ObjectKind::BindingAxis, &binding.axes[0].id)
            .unwrap();
        let sample = sdk.handle(ObjectKind::BindingSample, &samples[0]).unwrap();
        let binding_handle = sdk.handle(ObjectKind::MeshBinding, &id(8)).unwrap();
        let sample_address = sdk.binding_field_address(&samples[0]).unwrap();
        let content_address = sdk
            .scene_content_address(
                &id(2),
                SceneContentTarget::VertexPosition {
                    vertex_id: 1,
                    shape: ShapeTarget::Ordinary {
                        binding_id: id(8),
                        samples,
                    },
                },
            )
            .unwrap();
        let mut edit = sdk.begin_owned_edit("Payload", None).unwrap();
        apply(&mut edit, path, 0.4);
        let receipt = check(&mut sdk, edit, binding_record());
        assert_eq!(
            sdk.document().get_mesh(&id(2)),
            retained.document().get_mesh(&id(2))
        );
        assert_eq!(
            receipt.changes.content_changes,
            vec![SceneContentChange {
                entity: EntityRef::new(sdk.document().id(), ObjectKind::Mesh, &id(2)),
                domain: SceneContentDomain::MeshTopology,
                topology: false,
            }]
        );
        assert!(receipt.changes.collections.is_empty());
        assert!(receipt.changes.binding_references.is_empty());
        assert!(receipt.changes.scene_fields.is_empty());
        assert!(receipt
            .changes
            .modified
            .iter()
            .any(|e| e.kind == ObjectKind::MeshBinding && e.id == id(8)));
        assert_eq!(
            receipt
                .changes
                .categories
                .contains(&ChangeCategory::Topology),
            matches!(path, 0 | 4 | 5 | 6)
        );
        assert_eq!(
            sdk.content_epochs[&(id(2), SceneContentDomain::MeshTopology)],
            sdk.version().revision
        );
        assert!(sdk
            .plan_scene_content_patch(&content_address, content_address.expected().clone())
            .is_err());
        // Payload changes expire content addresses but leave sample context intact.
        assert!(sdk
            .plan_binding_field_patch(&sample_address, crate::BindingFieldPatch::SampleValue(0.1))
            .is_ok());
        for handle in [&axis, &sample, &binding_handle] {
            sdk.resolve_handle(handle).unwrap();
        }
        let undo = sdk.undo().unwrap();
        assert_eq!(undo.changes, receipt.changes.inverted());
        assert!(sdk.document().same_content(retained.document()));
        assert!(sdk
            .plan_scene_content_patch(&content_address, content_address.expected().clone())
            .is_err());
        let redo = sdk.redo().unwrap();
        assert_eq!(redo.changes, receipt.changes);
        sdk.undo().unwrap();
        sdk.drain_events();
        assert_eq!(
            retained.document().get_binding(&id(8)).unwrap().keyforms[0]
                .appearance
                .opacity,
            1.
        );
    }
    let mut edit = sdk
        .begin_owned_edit("Mesh and owned payload", None)
        .unwrap();
    apply(&mut edit, 1, 0.4);
    edit.rename_mesh(&id(2), "Target label").unwrap();
    let receipt = check(
        &mut sdk,
        edit,
        Some(BTreeSet::from([
            TrackedRecord::Mesh(id(2)),
            TrackedRecord::MeshBinding(id(8)),
        ])),
    );
    assert_eq!(receipt.changes.content_changes.len(), 1);
    assert_eq!(receipt.changes.scene_fields.len(), 1);
    assert_eq!(receipt.changes.modified.len(), 2);
}

#[test]
fn derived_mesh_notifications_do_not_require_a_touched_mesh_or_replace_input_ids() {
    let mut sdk = fixture(1);
    let mut edit = sdk
        .begin_owned_edit("Binding-only notification", None)
        .unwrap();
    apply(&mut edit, 1, 0.4);
    edit.object_ids.retain(|value| *value == id(8));
    let receipt = check(&mut sdk, edit, binding_record());
    assert_eq!(receipt.object_ids, vec![id(8)]);
    assert_eq!(receipt.changes.content_changes[0].entity.id, id(2));
    assert!(receipt.changes.modified.iter().any(|e| e.id == id(2)));
    let mut edit = sdk
        .begin_owned_edit("Roundtrip with extra notified peer", None)
        .unwrap();
    apply(&mut edit, 0, 0.2);
    apply(&mut edit, 0, 0.4);
    edit.object_ids.push(id(5));
    edit.rename_mesh(&id(3), "Other").unwrap();
    let receipt = check(
        &mut sdk,
        edit,
        Some(BTreeSet::from([
            TrackedRecord::MeshBinding(id(8)),
            TrackedRecord::Mesh(id(3)),
        ])),
    );
    assert!(receipt.changes.content_changes.is_empty());
    assert!(receipt
        .changes
        .categories
        .contains(&ChangeCategory::Topology));
    assert!(receipt
        .changes
        .modified
        .iter()
        .any(|e| e.kind == ObjectKind::MeshBinding && e.id == id(5)));
    assert!(!receipt
        .changes
        .modified
        .iter()
        .any(|e| e.kind == ObjectKind::Mesh && e.id == id(10)));
}

#[test]
fn payload_then_axis_sample_recreation_falls_back_and_never_revives_old_handles() {
    let mut sdk = fixture(1);
    let original = sdk.document().get_binding(&id(8)).unwrap().clone();
    let axis_id = original.axes[0].id.clone();
    let sample_id = original.axes[0].samples[0].id.clone();
    let axis = sdk.handle(ObjectKind::BindingAxis, &axis_id).unwrap();
    let sample = sdk.handle(ObjectKind::BindingSample, &sample_id).unwrap();
    let address = sdk.binding_field_address(&sample_id).unwrap();
    let mut edit = sdk.begin_owned_edit("Recreate identities", None).unwrap();
    apply(&mut edit, 1, 0.4);
    let mut replacement = original.clone();
    replacement.axes[0].id = id(500);
    replacement.axes[0].samples[0].id = id(501);
    for form in &mut replacement.keyforms {
        for value in &mut form.samples {
            if *value == sample_id {
                *value = id(501);
            }
        }
    }
    edit.replace_binding(replacement).unwrap();
    edit.replace_binding(original).unwrap();
    assert!(edit.candidate_document().same_content(sdk.document()));
    assert!(edit
        .candidate_document()
        .tracked_records_since(sdk.document())
        .is_none());
    #[cfg(feature = "bench-instrumentation")]
    let capture = zipzi_core::benchmark::Capture::start();
    let receipt = edit.commit_to(&mut sdk).unwrap();
    #[cfg(feature = "bench-instrumentation")]
    assert_eq!(
        capture.finish()["sdk.publication.fallback.retired_identities"].calls,
        1
    );
    assert!(receipt.changed);
    for identity in [&axis_id, &sample_id] {
        assert!(receipt.changes.created.iter().any(|e| e.id == *identity));
        assert!(receipt.changes.deleted.iter().any(|e| e.id == *identity));
    }
    for handle in [&axis, &sample] {
        assert!(sdk.resolve_handle(handle).is_err());
    }
    assert!(sdk
        .plan_binding_field_patch(&address, crate::BindingFieldPatch::SampleValue(0.2))
        .is_err());
    sdk.undo().unwrap();
    sdk.redo().unwrap();
    for handle in [&axis, &sample] {
        assert!(sdk.resolve_handle(handle).is_err());
    }
    assert!(sdk
        .plan_binding_field_patch(&address, crate::BindingFieldPatch::SampleValue(0.2))
        .is_err());
}

#[test]
fn payload_net_noops_preserve_redo_but_reverted_notifications_survive_mixed_edits() {
    let mut sdk = fixture(1);
    let mut edit = sdk.begin_owned_edit("Payload", None).unwrap();
    apply(&mut edit, 1, 0.4);
    check(&mut sdk, edit, binding_record());
    sdk.undo().unwrap();
    sdk.drain_events();
    let mut edit = sdk.begin_owned_edit("Net no-op", None).unwrap();
    for opacity in [0.25, 0.5, 0.75, 1.] {
        apply(&mut edit, 1, opacity);
    }
    assert!(!check(&mut sdk, edit, binding_record()).changed);
    sdk.redo().unwrap();
    sdk.drain_events();
    let mut edit = sdk
        .begin_owned_edit("Reverted payload plus label", None)
        .unwrap();
    apply(&mut edit, 1, 0.2);
    apply(&mut edit, 1, 0.4);
    edit.rename_mesh(&id(3), "Other").unwrap();
    let receipt = check(
        &mut sdk,
        edit,
        Some(BTreeSet::from([
            TrackedRecord::MeshBinding(id(8)),
            TrackedRecord::Mesh(id(3)),
        ])),
    );
    assert!(receipt.changes.content_changes.is_empty());
    assert!(receipt.changes.modified.iter().any(|e| e.id == id(2)));
    assert!(receipt.changes.modified.iter().any(|e| e.id == id(8)));
    assert!(receipt.changes.modified.iter().any(|e| e.id == id(3)));
}

#[test]
fn fixed_seed_payload_mesh_fallback_and_preview_failure_sequences_match_full_summaries() {
    let mut sdk = fixture(2);
    let retained = sdk.read_snapshot();
    let mut seed = 0xb15d_43c1_u32;
    for step in 0..64 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let mut edit = sdk.begin_owned_edit("Mixed sequence", None).unwrap();
        let mut expected = BTreeSet::new();
        let fallback = step % 9 == 0;
        if fallback && step % 2 == 0 {
            edit.edit_motion(MotionEdit::CreateGroup {
                id: id(5000 + step),
                name: format!("Untracked {step}"),
            })
            .unwrap();
        }
        for slot in 0..1 + seed % 4 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            apply(&mut edit, seed % 7, (1 + seed % 90) as f32 / 100.);
            expected.insert(TrackedRecord::MeshBinding(id(8)));
            if slot % 2 == 0 {
                edit.rename_mesh(&id(3), format!("Label {step}/{slot}"))
                    .unwrap();
                expected.insert(TrackedRecord::Mesh(id(3)));
            }
            if seed % 3 == 0 {
                let snapshot = edit.candidate_document().clone();
                let form = edit
                    .candidate_document()
                    .get_binding(&id(8))
                    .unwrap()
                    .keyforms[0]
                    .clone();
                let patch = PropertyPatch::Mesh {
                    id: id(2),
                    keyform: Some((id(8), form.samples)),
                    appearance: form.appearance,
                    draw_order: Some(5.),
                };
                assert!(edit
                    .preview_property(
                        &patch,
                        &mut PreviewState::default(),
                        [(id(99999), 0.)].into()
                    )
                    .is_err());
                assert!(edit.candidate_document().same_content(&snapshot));
            }
        }
        if fallback && step % 2 == 1 {
            edit.edit_motion(MotionEdit::CreateGroup {
                id: id(5000 + step),
                name: format!("Untracked {step}"),
            })
            .unwrap();
        }
        let receipt = check(&mut sdk, edit, (!fallback).then_some(expected));
        if step % 3 == 0 && receipt.changed {
            assert_eq!(sdk.undo().unwrap().changes, receipt.changes.inverted());
            assert_eq!(sdk.redo().unwrap().changes, receipt.changes);
            sdk.drain_events();
        }
        assert_eq!(
            retained.document().get_binding(&id(8)).unwrap().keyforms[0]
                .appearance
                .opacity,
            1.
        );
    }
}

#[test]
fn payload_late_failures_and_budget_rejection_publish_nothing_and_preserve_redo() {
    let mut sdk = fixture(1);
    let mut edit = sdk.begin_owned_edit("Payload", None).unwrap();
    apply(&mut edit, 0, 0.4);
    check(&mut sdk, edit, binding_record());
    sdk.undo().unwrap();
    sdk.drain_events();
    let version = sdk.version();
    let history = sdk.history_lengths();
    let mut edit = sdk.begin_owned_edit("Late invalid keyform", None).unwrap();
    apply(&mut edit, 0, 0.5);
    let mut invalid = edit
        .candidate_document()
        .get_binding(&id(8))
        .unwrap()
        .keyforms[0]
        .clone();
    invalid.positions.pop();
    assert!(edit.set_mesh_keyform(&id(8), invalid).is_err());
    assert_eq!(
        edit.commit_to(&mut sdk).unwrap_err().code.as_ref(),
        "EDIT_ABORTED"
    );
    assert_eq!(sdk.version(), version);
    assert_eq!(sdk.history_lengths(), history);
    assert!(sdk.drain_events().is_empty());
    let mut edit = sdk.begin_owned_edit("Budget", None).unwrap();
    apply(&mut edit, 0, 0.5);
    let budget = sdk.history_limits.max_bytes;
    sdk.history_limits.max_bytes = 0;
    assert_eq!(
        edit.commit_to(&mut sdk).unwrap_err().code.as_ref(),
        "HISTORY_LIMIT_EXCEEDED"
    );
    sdk.history_limits.max_bytes = budget;
    assert_eq!(sdk.version(), version);
    assert_eq!(sdk.history_lengths(), history);
    assert!(sdk.drain_events().is_empty());
    assert!(sdk.redo().unwrap().changed);
}

#[cfg(feature = "bench-instrumentation")]
#[test]
fn binding_payload_work_is_independent_of_sibling_bindings_samples_and_motion_keys() {
    for scale in [1, 10, 100] {
        let mut sdk = fixture(scale);
        for retained in [false, true] {
            let reader = retained.then(|| sdk.read_snapshot());
            for path in 0..7 {
                let base_keys = sdk.object_keys();
                let capture = zipzi_core::benchmark::Capture::start();
                let mut edit = sdk.begin_owned_edit("Payload work", None).unwrap();
                apply(&mut edit, path, 0.4);
                let receipt = edit.commit_to(&mut sdk).unwrap();
                let work = capture.finish();
                let count = |key| work.get(key).map_or(0, |v| v.calls);
                assert!(receipt.changed);
                assert_eq!(
                    count("sdk.publication.local"),
                    1,
                    "{scale}/{path}: {work:?}"
                );
                assert_eq!(count("sdk.publication.touched_records"), 1);
                assert_eq!(count("core.tracking.record_reads"), 4);
                assert_eq!(count("core.validate_structure.full"), 1);
                assert_eq!(count("core.validate_structure.local"), 0);
                assert_eq!(count("core.tracking.binding_record_reads"), 4);
                assert_eq!(count("sdk.binding_summary.record_reads"), 2);
                assert_eq!(count("sdk.binding_summary.derived_meshes"), 1);
                assert_eq!(count("sdk.mesh_summary.record_reads"), 0);
                assert_eq!(count("sdk.mesh_summary.notification_reads"), 4);
                assert_eq!(count("sdk.binding_summary.notification_reads"), 2);
                assert_eq!(
                    count("core.canonicalize_binding"),
                    if matches!(path, 2 | 3) { 1 } else { 2 },
                    "{scale}/{path}: {work:?}"
                );
                for key in [
                    "sdk.publication.fallback",
                    "sdk.keys_in",
                    "core.authoring_identity_scan",
                    "sdk.summarize.fallback",
                    "core.validate_motion",
                ] {
                    assert_eq!(
                        count(key),
                        0,
                        "{scale}/{retained}/{path}: unexpected {key}: {work:?}"
                    );
                }
                assert_eq!(receipt.changes.content_changes.len(), 1);
                assert!(std::sync::Arc::ptr_eq(&base_keys, &sdk.object_keys()));
                if let Some(reader) = &reader {
                    assert_eq!(
                        reader.document().get_binding(&id(8)).unwrap().keyforms[0]
                            .appearance
                            .opacity,
                        1.
                    );
                }
                sdk.undo().unwrap();
                sdk.drain_events();
                println!("binding_work scale={scale} sibling_bindings={scale} unrelated_samples={} motion_keys={} retained={retained} path={path} touched=1 proof_reads=4 binding_summary_reads=2 derived_meshes=1 identity_scans=0 key_enumerations=0", 4 * scale, 8 * scale);
            }
        }
    }
}

#[cfg(feature = "bench-instrumentation")]
#[test]
fn two_changed_bindings_read_two_records_and_emit_two_sorted_derived_targets() {
    let mut sdk = fixture(1);
    let positions = sdk
        .document()
        .get_mesh(&id(3))
        .unwrap()
        .base_positions
        .clone();
    let mut binding = sdk.document().get_binding(&id(8)).unwrap().clone();
    binding.id = id(9);
    binding.mesh_id = id(3);
    for a in &mut binding.axes {
        a.id.clear();
        a.samples.clear();
    }
    for f in &mut binding.keyforms {
        f.samples.clear();
        f.positions = positions.clone();
    }
    sdk.edit("Peer", None, |e| e.create_binding(binding))
        .unwrap();
    sdk.drain_events();
    let mut edit = sdk.begin_owned_edit("Two payloads", None).unwrap();
    apply(&mut edit, 1, 0.4);
    let mut form = edit
        .candidate_document()
        .get_binding(&id(9))
        .unwrap()
        .keyforms[0]
        .clone();
    form.appearance.opacity = 0.5;
    edit.set_mesh_keyform(&id(9), form).unwrap();
    let full = full_summary(&sdk, &edit);
    let capture = zipzi_core::benchmark::Capture::start();
    let receipt = edit.commit_to(&mut sdk).unwrap();
    let work = capture.finish();
    assert_eq!(receipt.changes, full);
    assert_eq!(work["sdk.publication.touched_records"].calls, 2);
    assert_eq!(work["sdk.binding_summary.record_reads"].calls, 4);
    assert_eq!(work["sdk.binding_summary.derived_meshes"].calls, 2);
    assert_eq!(
        receipt
            .changes
            .content_changes
            .iter()
            .map(|c| c.entity.id.clone())
            .collect::<Vec<_>>(),
        vec![id(2), id(3)]
    );
}
