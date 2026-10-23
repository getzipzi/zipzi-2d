use super::*;
use crate::{AuthoringSession, EditSession};
use kasane_core::{
    document::*, Appearance, BindingAxis, Mesh, MeshBinding, MeshKeyform, ModelSetup, Parameter,
    Part, Vec2,
};

pub(super) fn id(n: u32) -> String {
    format!("20000000-0000-4000-8000-{n:012x}")
}

pub(super) fn fixture(scale: u32) -> AuthoringSession {
    let mut sdk = AuthoringSession::new(&id(1), ModelSetup::default()).unwrap();
    sdk.edit("Synthetic model", None, |e| {
        for n in [2, 3] {
            e.create_mesh(Mesh {
                id: id(n),
                name: "Mesh".into(),
                vertex_ids: vec![1, 2, 3],
                base_positions: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
                triangles: vec![[1, 2, 3]],
                ..Default::default()
            })?;
        }
        for n in 0..scale {
            e.create_part(Part {
                id: id(1000 + n),
                ..Default::default()
            })?;
            e.create_mesh(Mesh {
                id: id(10 + n),
                part_id: id(1000 + n),
                ..Default::default()
            })?;
        }
        e.create_parameter(Parameter {
            id: id(4),
            minimum: 0.,
            maximum: 1.,
            ..Default::default()
        })?;
        for (binding_id, mesh_id, count) in (0..scale)
            .map(|n| (if n == 0 { 5 } else { 2000 + n }, 10 + n, 4))
            .chain([(8, 2, 2)])
        {
            let keys: Vec<_> = (0..count).map(|i| i as f32 / (count - 1) as f32).collect();
            let positions = e
                .candidate_document()
                .get_mesh(&id(mesh_id))
                .unwrap()
                .base_positions
                .clone();
            e.create_binding(MeshBinding {
                id: id(binding_id),
                mesh_id: id(mesh_id),
                axes: vec![BindingAxis {
                    parameter_id: id(4),
                    keys: keys.clone(),
                    ..Default::default()
                }],
                keyforms: keys
                    .into_iter()
                    .map(|key| MeshKeyform {
                        keys: vec![key],
                        positions: positions.clone(),
                        ..Default::default()
                    })
                    .collect(),
            })?;
        }
        e.create_motion(MotionClip {
            id: id(6),
            name: "Unrelated motion".into(),
            duration: 1.,
            fps: 30.,
            looping: false,
            restricted_beziers: true,
            fade_in: None,
            fade_out: None,
            track_order: vec![],
            tracks: vec![MotionTrack {
                id: id(7),
                target: MotionTrackTarget::Parameter {
                    parameter_id: id(4),
                },
                initial: MotionKey {
                    id: id(100000),
                    time: 0.,
                    value: 0.,
                },
                segments: std::sync::Arc::new(
                    (1..8 * scale)
                        .map(|n| MotionSegment::Linear {
                            end: MotionKey {
                                id: id(100000 + n),
                                time: n as f32 / (8 * scale - 1) as f32,
                                value: 0.,
                            },
                        })
                        .collect(),
                ),
                fade_in: None,
                fade_out: None,
                extensions: Default::default(),
            }]
            .into(),
            events: vec![].into(),
            extensions: Default::default(),
            meta_extensions: Default::default(),
            opaque_source_ids: None,
            opaque_source_content_hash: None,
        })
    })
    .unwrap();
    sdk.drain_events();
    sdk
}

fn apply(edit: &mut EditSession<'_>, path: u32) {
    match path {
        0 => edit.rename_mesh(&id(2), "Renamed").unwrap(),
        1 | 6 => {
            let mut mesh = edit.candidate_document().get_mesh(&id(2)).unwrap().clone();
            if path == 1 {
                mesh.name = "Renamed".into();
            } else {
                mesh.appearance.opacity = 0.4;
            }
            edit.replace_mesh(mesh).unwrap();
        }
        2 | 5 | 7 => {
            let value = if path == 2 {
                SceneFieldValue::Label("Renamed".into())
            } else if path == 7 {
                SceneFieldValue::Appearance(Appearance {
                    opacity: 1.00005,
                    multiply: [0.5, 0.6, 0.7],
                    screen: [0.1, 0.2, 0.3],
                })
            } else {
                SceneFieldValue::Enabled(false)
            };
            let intent = SceneEdit::Patch {
                id: id(2),
                expected: edit
                    .candidate_document()
                    .scene_field(&id(2), value.field())
                    .unwrap(),
                value,
                clear_glue_pairs: false,
            };
            let plan = edit
                .candidate_document()
                .plan_scene_edit(intent.clone())
                .unwrap();
            assert!(plan.preserves_identities());
            edit.edit_scene(intent).unwrap();
        }
        3 => edit.set_mesh_opacity(&id(2), 0.4).unwrap(),
        4 => edit
            .patch_property(&PropertyPatch::Mesh {
                id: id(2),
                keyform: None,
                appearance: Appearance {
                    opacity: 0.4,
                    ..Default::default()
                },
                draw_order: None,
            })
            .unwrap(),
        8 => edit
            .preview_property(
                &PropertyPatch::Mesh {
                    id: id(2),
                    keyform: None,
                    appearance: Appearance {
                        opacity: 0.4,
                        ..Default::default()
                    },
                    draw_order: None,
                },
                &mut kasane_core::preview::PreviewState::default(),
                Default::default(),
            )
            .unwrap(),
        _ => unreachable!(),
    }
}

pub(super) fn full_summary(sdk: &AuthoringSession, edit: &EditSession<'_>) -> ChangeSet {
    let before = sdk.document();
    let after = edit.candidate_document();
    assert_eq!(
        after.clone().validate_structure(),
        after.validate_structure_since(before)
    );
    summarize_impl(
        before,
        after,
        &crate::session::keys_in(before),
        &crate::session::keys_in(after),
        &edit.object_ids,
        &edit.categories,
        false,
    )
}

#[test]
fn local_mesh_summaries_match_every_field_with_bound_geometry_and_unrelated_domains() {
    let mut sdk = fixture(2);
    let retained = sdk.read_snapshot();
    for path in 0..9 {
        let mut edit = sdk.begin_owned_edit("Fields", None).unwrap();
        apply(&mut edit, path);
        let full = full_summary(&sdk, &edit);
        let publication = MeshPublication::new(
            sdk.document(),
            edit.candidate_document(),
            &edit.object_ids,
            &edit.erased_keys,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(publication.summarize(&edit.categories)).unwrap(),
            serde_json::to_value(&full).unwrap()
        );
        let receipt = edit.commit_to(&mut sdk).unwrap();
        assert_eq!(
            serde_json::to_value(&receipt.changes).unwrap(),
            serde_json::to_value(&full).unwrap()
        );
        let undo = sdk.undo().unwrap();
        assert_eq!(undo.changes, full.inverted());
        let redo = sdk.redo().unwrap();
        assert_eq!(redo.changes, full);
        sdk.undo().unwrap();
        sdk.drain_events();
        assert!(sdk.document().same_content(retained.document()));
    }
    // Exercise all allowed fields together and preserve IDs for a net-noop Mesh.
    let mut edit = sdk.begin_owned_edit("Mixed Mesh fields", None).unwrap();
    edit.rename_mesh(&id(2), "Multiple fields").unwrap();
    edit.set_mesh_opacity(&id(2), 0.3).unwrap();
    let mut mesh = edit.candidate_document().get_mesh(&id(2)).unwrap().clone();
    mesh.enabled = false;
    mesh.appearance.multiply = [0.5, 0.6, 0.7];
    mesh.appearance.screen = [0.1, 0.2, 0.3];
    edit.replace_mesh(mesh).unwrap();
    edit.rename_mesh(&id(3), "Temporary").unwrap();
    edit.rename_mesh(&id(3), "Mesh").unwrap();
    let full = full_summary(&sdk, &edit);
    let receipt = edit.commit_to(&mut sdk).unwrap();
    assert_eq!(receipt.changes, full);
    assert!(receipt.changes.modified.iter().any(|e| e.id == id(3)));
}

#[test]
fn mismatched_notification_scope_and_retired_keys_cannot_use_local_summary() {
    let mut sdk = fixture(1);
    let mut edit = sdk.begin_owned_edit("Scope", None).unwrap();
    apply(&mut edit, 1);
    let original = edit.object_ids.clone();
    edit.object_ids.clear();
    assert!(MeshPublication::new(
        sdk.document(),
        edit.candidate_document(),
        &edit.object_ids,
        &edit.erased_keys
    )
    .is_none());
    edit.object_ids = original;
    edit.object_ids.push(id(1000));
    assert!(MeshPublication::new(
        sdk.document(),
        edit.candidate_document(),
        &edit.object_ids,
        &edit.erased_keys
    )
    .is_none());
    edit.object_ids.pop();
    let full = full_summary(&sdk, &edit);
    // A conservative summary is still required when the input notification
    // scope cannot be justified, even though Core proves key-set stability.
    edit.object_ids.push(id(1000));
    let scoped_full = full_summary(&sdk, &edit);
    assert_ne!(full.categories, scoped_full.categories);
    let mut recreated = sdk
        .begin_owned_edit("Retired identity guard", None)
        .unwrap();
    apply(&mut recreated, 0);
    recreated.erased_keys.insert(ObjectKey {
        kind: ObjectKind::Mesh,
        id: id(2),
    });
    assert!(MeshPublication::new(
        sdk.document(),
        recreated.candidate_document(),
        &recreated.object_ids,
        &recreated.erased_keys
    )
    .is_none());
    #[cfg(feature = "bench-instrumentation")]
    let capture = kasane_core::benchmark::Capture::start();
    let receipt = edit.commit_to(&mut sdk).unwrap();
    assert_eq!(receipt.changes, scoped_full);
    #[cfg(feature = "bench-instrumentation")]
    {
        let work = capture.finish();
        assert_eq!(work["sdk.publication.fallback.notification_scope"].calls, 1);
        assert_eq!(work["sdk.summarize.fallback"].calls, 1);
    }
}

#[cfg(feature = "bench-instrumentation")]
#[test]
fn mesh_publication_work_is_independent_of_unrelated_keys_samples_and_scene_size() {
    for scale in [1, 10, 100] {
        let mut sdk = fixture(scale);
        for retained in [false, true] {
            let reader = retained.then(|| sdk.read_snapshot());
            for path in 0..9 {
                let base_keys = sdk.object_keys();
                let handle = sdk.handle(ObjectKind::Mesh, &id(2)).unwrap();
                let capture = kasane_core::benchmark::Capture::start();
                let mut edit = sdk.begin_owned_edit("Work", None).unwrap();
                apply(&mut edit, path);
                let receipt = edit.commit_to(&mut sdk).unwrap();
                let work = capture.finish();
                let count = |key| work.get(key).map_or(0, |v| v.calls);
                assert!(receipt.changed);
                assert_eq!(
                    count("sdk.publication.local"),
                    1,
                    "{scale}/{retained}/{path}: {work:?}"
                );
                assert_eq!(count("sdk.publication.touched_records"), 1);
                assert_eq!(count("core.tracking.record_reads"), 4);
                assert_eq!(count("core.validate_structure.local"), 1);
                assert_eq!(count("core.validate_structure.local.mesh_checks"), 1);
                assert_eq!(count("sdk.mesh_summary.notification_reads"), 2);
                assert_eq!(count("sdk.mesh_summary.record_reads"), 2);
                for key in [
                    "sdk.publication.fallback",
                    "sdk.keys_in",
                    "core.authoring_identity_scan",
                    "sdk.summarize.fallback",
                    "core.validate_structure.full",
                    "core.validate_structure.full.mesh_checks",
                    "core.canonicalize_binding",
                    "core.validate_motion",
                ] {
                    assert_eq!(
                        count(key),
                        0,
                        "{scale}/{retained}/{path}: unexpected {key}: {work:?}"
                    );
                }
                assert!(std::sync::Arc::ptr_eq(&base_keys, &sdk.object_keys()));
                sdk.resolve_handle(&handle).unwrap();
                if let Some(reader) = &reader {
                    assert_eq!(reader.document().get_mesh(&id(2)).unwrap().name, "Mesh");
                }
                sdk.undo().unwrap();
                sdk.drain_events();
                println!("mesh_work scale={scale} motion_keys={} binding_samples={} retained={retained} path={path} touched=1 proof_reads=4 local_mesh_checks=1 full_validation=0 canonicalize_binding=0 motion_validation=0 notification_reads=2 summary_reads=2 identity_scans=0 key_enumerations=0", 8 * scale, 4 * scale + 2);
            }
        }
    }
}

#[cfg(feature = "bench-instrumentation")]
#[test]
fn work_counts_grow_with_touched_meshes_and_detect_fallbacks() {
    let mut sdk = fixture(1);
    let mut edit = sdk.begin_owned_edit("Two records", None).unwrap();
    edit.rename_mesh(&id(2), "First").unwrap();
    edit.rename_mesh(&id(3), "Second").unwrap();
    let capture = kasane_core::benchmark::Capture::start();
    edit.commit_to(&mut sdk).unwrap();
    let work = capture.finish();
    assert_eq!(work["sdk.publication.touched_records"].calls, 2);
    assert_eq!(work["sdk.mesh_summary.record_reads"].calls, 4);
    assert_eq!(work["core.tracking.record_reads"].calls, 8);
    assert_eq!(work["core.validate_structure.local.mesh_checks"].calls, 2);
    assert!(!work.contains_key("core.validate_structure.full"));
    for order in [false, true] {
        let mut edit = sdk.begin_owned_edit("Mixed fallback", None).unwrap();
        if order {
            edit.rename_mesh(&id(2), "Before").unwrap();
        }
        edit.create_mesh(Mesh {
            id: id(500 + u32::from(order)),
            ..Default::default()
        })
        .unwrap();
        if !order {
            edit.rename_mesh(&id(2), "After").unwrap();
        }
        let full = full_summary(&sdk, &edit);
        let capture = kasane_core::benchmark::Capture::start();
        let receipt = edit.commit_to(&mut sdk).unwrap();
        let work = capture.finish();
        assert_eq!(receipt.changes, full);
        assert_eq!(work["sdk.publication.fallback.unknown_coverage"].calls, 1);
        assert_eq!(work["sdk.summarize.fallback"].calls, 1);
        assert_eq!(work["sdk.keys_in"].calls, 1);
        assert_eq!(work["core.validate_structure.full"].calls, 1);
        assert!(!work.contains_key("core.validate_structure.local"));
        assert!(!work.contains_key("sdk.mesh_summary.record_reads"));
    }
}

#[cfg(feature = "bench-instrumentation")]
#[test]
fn public_full_validation_work_grows_with_model_size_as_a_positive_control() {
    for scale in [1, 10, 100] {
        let sdk = fixture(scale);
        let capture = kasane_core::benchmark::Capture::start();
        assert!(sdk.document().validate_structure().is_empty());
        let work = capture.finish();
        assert_eq!(work["core.validate_structure.full"].calls, 1);
        assert_eq!(
            work["core.validate_structure.full.mesh_checks"].calls,
            u64::from(scale + 2)
        );
        assert_eq!(
            work["core.canonicalize_binding"].calls,
            u64::from(scale + 1)
        );
        assert_eq!(work["core.validate_motion"].calls, 1);
        assert!(!work.contains_key("core.validate_structure.local"));
        println!("full_validation_work scale={scale} mesh_checks={} canonicalize_binding={} motion_validation=1 local_validation=0", scale + 2, scale + 1);
    }
}
