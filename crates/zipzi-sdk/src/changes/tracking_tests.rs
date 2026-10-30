//! Shadow local publication against full summaries using actual transaction
//! inputs. The reference runs without tracker pruning on independent snapshots.
use super::*;
use crate::changes::ChangeSetExt;
use crate::{AuthoringSession, EditSession};
use zipzi_core::{document::*, Appearance, Mesh, ModelSetup};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}

fn session() -> AuthoringSession {
    let mut sdk = AuthoringSession::with_history_limits(
        &id(1),
        ModelSetup::default(),
        crate::HistoryLimits {
            max_steps: 256,
            ..Default::default()
        },
    )
    .unwrap();
    sdk.edit("Setup", None, |e| {
        for n in [2, 3] {
            e.create_mesh(Mesh {
                id: id(n),
                name: "Mesh".into(),
                ..Default::default()
            })?;
        }
        Ok(())
    })
    .unwrap();
    sdk.drain_events();
    sdk
}

fn label(edit: &mut EditSession<'_>, n: u32, value: String) {
    edit.edit_scene(SceneEdit::Patch {
        id: id(n),
        expected: edit
            .candidate_document()
            .scene_field(&id(n), SceneField::Label)
            .unwrap(),
        value: SceneFieldValue::Label(value),
        clear_glue_pairs: false,
    })
    .unwrap();
}

fn check(
    sdk: &mut AuthoringSession,
    edit: EditSession<'_>,
    expected: Option<&[u32]>,
) -> crate::EditReceipt {
    check_records(
        sdk,
        edit,
        expected.map(|ids| ids.iter().map(|n| TrackedRecord::Mesh(id(*n))).collect()),
    )
}

fn check_records(
    sdk: &mut AuthoringSession,
    edit: EditSession<'_>,
    expected: Option<BTreeSet<TrackedRecord>>,
) -> crate::EditReceipt {
    let before = sdk.document();
    let after = edit.candidate_document();
    let records = after.tracked_records_since(before);
    assert_eq!(records.cloned(), expected);
    if let Some(records) = records {
        let mut restored = after.clone();
        for record in records {
            let result = match record {
                TrackedRecord::Mesh(id) => {
                    restored.replace_mesh(before.get_mesh(id).unwrap().clone())
                }
                TrackedRecord::MeshBinding(id) => {
                    restored.replace_binding(before.get_binding(id).unwrap().clone())
                }
            };
            assert!(result.status.is_ok());
        }
        assert!(
            before.same_content(&restored),
            "tracker missed a persistent write"
        );
        assert!(after.clone().same_authoring_identities(before));
    }
    assert_eq!(
        after.clone().validate_structure(),
        after.validate_structure_since(before)
    );
    let keys = crate::session::keys_in(before);
    let after_keys = crate::session::keys_in(after);
    let full = if before.same_content(after) {
        ChangeSet::default()
    } else {
        summarize_impl(
            before,
            after,
            &keys,
            &after_keys,
            &edit.object_ids,
            &edit.categories,
            false,
        )
    };
    let old_version = sdk.version();
    let old_history = sdk.history_lengths();
    #[cfg(feature = "bench-instrumentation")]
    let capture = zipzi_core::benchmark::Capture::start();
    let receipt = edit.commit_to(sdk).unwrap();
    #[cfg(feature = "bench-instrumentation")]
    {
        let work = capture.finish();
        let calls = |name| work.get(name).map_or(0, |m| m.calls);
        let local_validation = expected.as_ref().is_some_and(|records| {
            records
                .iter()
                .all(|record| matches!(record, TrackedRecord::Mesh(_)))
        });
        assert_eq!(
            calls("core.validate_structure.local"),
            u64::from(local_validation)
        );
        assert_eq!(
            calls("core.validate_structure.full"),
            u64::from(!local_validation)
        );
        if local_validation {
            assert_eq!(
                calls("core.validate_structure.local.mesh_checks"),
                expected.as_ref().unwrap().len() as u64
            );
        }
        assert_eq!(
            calls("sdk.publication.local"),
            u64::from(expected.is_some())
        );
        assert_eq!(
            calls("sdk.publication.fallback"),
            u64::from(expected.is_none())
        );
        if expected.is_some() {
            assert_eq!(calls("sdk.keys_in"), 0);
            assert_eq!(calls("core.authoring_identity_scan"), 0);
            assert_eq!(calls("sdk.summarize.fallback"), 0);
        }
    }
    assert_eq!(
        serde_json::to_value(&receipt.changes).unwrap(),
        serde_json::to_value(full).unwrap()
    );
    let events = sdk.drain_events();
    assert_eq!(events.len(), usize::from(receipt.changed));
    if receipt.changed {
        assert_eq!(sdk.history_lengths(), (old_history.0 + 1, 0));
        assert_eq!(events[0], receipt);
    } else {
        assert_eq!(sdk.version(), old_version);
        assert_eq!(sdk.history_lengths(), old_history);
    }
    receipt
}

#[test]
fn mesh_entrypoints_match_full_summaries_with_their_original_declared_categories() {
    let mut outputs = Vec::new();
    for path in 0..3 {
        let mut sdk = session();
        let mut edit = sdk.begin_owned_edit("Label", None).unwrap();
        match path {
            0 => edit.rename_mesh(&id(2), "Renamed").unwrap(),
            1 => {
                let mut mesh = edit.candidate_document().get_mesh(&id(2)).unwrap().clone();
                mesh.name = "Renamed".into();
                edit.replace_mesh(mesh).unwrap();
            }
            _ => label(&mut edit, 2, "Renamed".into()),
        }
        let mut receipt = check(&mut sdk, edit, Some(&[2]));
        assert_eq!(
            receipt.changes.categories,
            if path == 1 {
                BTreeSet::from([
                    ChangeCategory::Labels,
                    ChangeCategory::References,
                    ChangeCategory::EvaluationInputs,
                ])
            } else {
                BTreeSet::from([ChangeCategory::Labels])
            }
        );
        // Each whole summary was compared above using its actual transaction
        // categories. Only field/content outputs are equivalent across APIs.
        receipt.changes.categories.clear();
        outputs.push((
            sdk.document().clone(),
            serde_json::to_value(receipt.changes).unwrap(),
        ));
    }
    for (document, summary) in &outputs[1..] {
        assert!(outputs[0].0.same_content(document));
        assert_eq!(&outputs[0].1, summary);
    }
}

#[test]
fn mixed_fields_roundtrips_and_noops_preserve_publication_and_redo() {
    let mut sdk = session();
    let retained = sdk.read_snapshot();
    let mut edit = sdk.begin_owned_edit("Name and appearance", None).unwrap();
    edit.rename_mesh(&id(2), "Renamed").unwrap();
    edit.set_mesh_opacity(&id(2), 0.4).unwrap();
    let receipt = check(&mut sdk, edit, Some(&[2]));
    assert!(receipt.changes.categories.contains(&ChangeCategory::Labels));
    assert!(receipt
        .changes
        .categories
        .contains(&ChangeCategory::EvaluationInputs));
    let undo = sdk.undo().unwrap();
    assert_eq!(
        serde_json::to_value(undo.changes).unwrap(),
        serde_json::to_value(receipt.changes.inverted()).unwrap()
    );
    sdk.drain_events();
    let mut edit = sdk.begin_owned_edit("No-op", None).unwrap();
    edit.set_mesh_opacity(&id(2), 0.25).unwrap();
    edit.set_mesh_opacity(&id(2), 1.).unwrap();
    assert!(!check(&mut sdk, edit, Some(&[2])).changed);
    let redo = sdk.redo().unwrap();
    assert_eq!(
        serde_json::to_value(redo.changes).unwrap(),
        serde_json::to_value(&receipt.changes).unwrap()
    );
    sdk.drain_events();
    let mut edit = sdk
        .begin_owned_edit("Reverted touch plus another object", None)
        .unwrap();
    edit.set_mesh_opacity(&id(2), 0.2).unwrap();
    edit.set_mesh_opacity(&id(2), 0.4).unwrap();
    label(&mut edit, 3, "Other".into());
    let receipt = check(&mut sdk, edit, Some(&[2, 3]));
    // Touched IDs are notification inputs even when one record has no net diff.
    assert!(receipt
        .changes
        .modified
        .iter()
        .any(|entity| entity.id == id(2)));
    assert_eq!(retained.document().get_mesh(&id(2)).unwrap().name, "Mesh");
    assert_eq!(
        retained
            .document()
            .get_mesh(&id(2))
            .unwrap()
            .appearance
            .opacity,
        1.
    );
}

#[test]
fn mixed_motion_and_mesh_edits_fall_back_in_both_orders() {
    for mesh_first in [false, true] {
        let mut sdk = session();
        let mut edit = sdk.begin_owned_edit("Mixed domains", None).unwrap();
        if mesh_first {
            label(&mut edit, 2, "Before".into());
        }
        edit.edit_motion(MotionEdit::CreateGroup {
            id: id(4),
            name: "Motion group".into(),
        })
        .unwrap();
        if !mesh_first {
            label(&mut edit, 2, "After".into());
        }
        check(&mut sdk, edit, None);
    }
}

#[test]
fn mesh_binding_payload_and_keyform_property_writes_are_recorded_with_mesh_edits() {
    use zipzi_core::{BindingAxis, MeshBinding, MeshKeyform, Parameter};
    let mut sdk = session();
    sdk.edit("Binding", None, |e| {
        e.create_parameter(Parameter {
            id: id(4),
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })?;
        e.create_binding(MeshBinding {
            id: id(5),
            mesh_id: id(2),
            axes: vec![BindingAxis {
                parameter_id: id(4),
                keys: vec![0., 1.],
                ..Default::default()
            }],
            keyforms: [0., 1.]
                .into_iter()
                .map(|key| MeshKeyform {
                    keys: vec![key],
                    ..Default::default()
                })
                .collect(),
        })
    })
    .unwrap();
    sdk.drain_events();
    for property in [false, true] {
        let mut edit = sdk.begin_owned_edit("Binding payload", None).unwrap();
        label(&mut edit, 3, format!("Label {property}"));
        let mut binding = edit
            .candidate_document()
            .get_binding(&id(5))
            .unwrap()
            .clone();
        binding.keyforms[0].appearance.opacity = if property { 0.6 } else { 0.4 };
        if property {
            edit.patch_property(&PropertyPatch::Mesh {
                id: id(2),
                keyform: Some((id(5), binding.keyforms[0].samples.clone())),
                appearance: binding.keyforms[0].appearance,
                draw_order: binding.keyforms[0].draw_order,
            })
            .unwrap();
        } else {
            edit.replace_binding(binding).unwrap();
        }
        label(&mut edit, 3, format!("After {property}"));
        check_records(
            &mut sdk,
            edit,
            Some(BTreeSet::from([
                TrackedRecord::Mesh(id(3)),
                TrackedRecord::MeshBinding(id(5)),
            ])),
        );
    }
}

#[test]
fn delete_recreate_falls_back_and_retires_handles_despite_identical_content() {
    let mut sdk = session();
    // Recreate the last mesh so native collection order is also identical.
    let old = sdk.handle(ObjectKind::Mesh, &id(3)).unwrap();
    let mut edit = sdk.begin_owned_edit("Recreate", None).unwrap();
    let mesh = edit.candidate_document().get_mesh(&id(3)).unwrap().clone();
    edit.erase_object(&id(3)).unwrap();
    edit.create_mesh(mesh).unwrap();
    assert!(edit.candidate_document().same_content(sdk.document()));
    assert!(edit
        .candidate_document()
        .tracked_records_since(sdk.document())
        .is_none());
    let receipt = edit.commit_to(&mut sdk).unwrap();
    assert!(receipt.changed);
    assert_eq!(
        sdk.resolve_handle(&old).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    assert!(receipt
        .changes
        .created
        .iter()
        .any(|entity| entity.id == id(3)));
    assert!(receipt
        .changes
        .deleted
        .iter()
        .any(|entity| entity.id == id(3)));
}

#[test]
fn fixed_seed_sequences_shadow_coverage_and_summaries_across_history_and_failures() {
    let mut sdk = session();
    let mut seed = 0x1c49_670e_u32;
    for step in 0..64 {
        seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
        let mut edit = sdk.begin_owned_edit("Sequence", None).unwrap();
        let mut fallback = false;
        let mut touched = BTreeSet::new();
        for slot in 0..1 + seed % 4 {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let n = 2 + (seed >> 8) % 2;
            let revision = edit.candidate_document().revision();
            match seed % 5 {
                0 => {
                    label(&mut edit, n, format!("Label {step}/{slot}"));
                }
                1 => {
                    edit.set_mesh_opacity(&id(n), (seed % 99) as f32 / 100.)
                        .unwrap();
                }
                2 => {
                    let mut mesh = edit.candidate_document().get_mesh(&id(n)).unwrap().clone();
                    mesh.enabled = !mesh.enabled;
                    edit.replace_mesh(mesh).unwrap();
                }
                3 => {
                    let mut preview = zipzi_core::preview::PreviewState::default();
                    let patch = PropertyPatch::Mesh {
                        id: id(n),
                        keyform: None,
                        appearance: Appearance {
                            opacity: 0.25,
                            ..Default::default()
                        },
                        draw_order: Some(2.),
                    };
                    assert!(edit
                        .preview_property(&patch, &mut preview, [(id(99), 0.)].into())
                        .is_err());
                    continue; // Failed preview contributes neither writes nor categories.
                }
                _ => {
                    let mut mesh = edit.candidate_document().get_mesh(&id(n)).unwrap().clone();
                    mesh.double_sided = !mesh.double_sided;
                    edit.replace_mesh(mesh).unwrap();
                    fallback = true;
                }
            }
            if edit.candidate_document().revision() != revision {
                touched.insert(n);
            }
        }
        let expected: Vec<_> = touched.into_iter().collect();
        let receipt = check(&mut sdk, edit, (!fallback).then_some(expected.as_slice()));
        if receipt.changed && step % 3 == 0 {
            sdk.undo().unwrap();
            let redo = sdk.redo().unwrap();
            assert_eq!(
                serde_json::to_value(redo.changes).unwrap(),
                serde_json::to_value(receipt.changes).unwrap()
            );
            sdk.drain_events();
        }
    }
}
