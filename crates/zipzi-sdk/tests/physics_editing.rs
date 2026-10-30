use std::sync::Arc;
use zipzi_core::document::PhysicsAssetImport;
use zipzi_core::{
    document::{
        PhysicsAsset, PhysicsAssetProperty, PhysicsPatch, PhysicsProperty as P, PhysicsRigPatch,
        PhysicsTarget,
    },
    Canvas, Parameter, Vec2,
};
use zipzi_sdk::AuthoringSession;

const DOC: &str = "00000000-0000-4000-8000-00000000f001";
const ASSET: &str = "00000000-0000-4000-8000-00000000f002";
const X: &str = "00000000-0000-4000-8000-00000000f003";
const Y: &str = "00000000-0000-4000-8000-00000000f004";
fn fid(n: u64) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn asset(groups: usize, nodes: usize) -> PhysicsAsset {
    let mut data = serde_json::json!({"Version":3,"Meta":{"PhysicsSettingCount":groups,"TotalInputCount":groups,"TotalOutputCount":groups,"VertexCount":groups*nodes,"Fps":30,"EffectiveForces":{"Gravity":{"X":0,"Y":-1},"Wind":{"X":0,"Y":0}},"PhysicsDictionary":[]},"PhysicsSettings":[]});
    for i in 0..groups {
        data["Meta"]["PhysicsDictionary"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({"Id":format!("Rig{i}"),"Name":format!("Group {i}")}));
        let vertices:Vec<_>=(0..nodes).map(|n|serde_json::json!({"Position":{"X":0,"Y":n},"Radius":if n==0 {0}else{1},"Mobility":0.95,"Delay":0.8,"Acceleration":1})).collect();
        data["PhysicsSettings"].as_array_mut().unwrap().push(serde_json::json!({"Id":format!("Rig{i}"),"Input":[{"Source":{"Target":"Parameter","Id":"ParamX"},"Weight":100,"Type":"Angle","Reflect":false}],"Output":[{"Destination":{"Target":"Parameter","Id":"ParamY"},"VertexIndex":1,"Scale":1,"Weight":100,"Type":"Angle","Reflect":false}],"Vertices":vertices,"Normalization":{"Position":{"Minimum":-1,"Default":0,"Maximum":1},"Angle":{"Minimum":-1,"Default":0,"Maximum":1}}}));
    }
    let mut asset =
        PhysicsAsset::from_wire(ASSET.into(), serde_json::from_value(data).unwrap(), |id| {
            Some(if id == "ParamX" { X } else { Y }.into())
        })
        .unwrap();
    for (i, g) in asset.data.settings.iter_mut().enumerate() {
        let g = Arc::make_mut(g);
        let old = g.id.clone();
        let base = 1000 + i as u64 * 1000;
        g.id = fid(base);
        for d in &mut asset.data.meta.dictionary {
            if d.id == old {
                d.id = g.id.clone();
            }
        }
        for (j, v) in g.inputs.iter_mut().enumerate() {
            v.id = fid(base + 100 + j as u64);
        }
        for (j, v) in g.outputs.iter_mut().enumerate() {
            v.id = fid(base + 200 + j as u64);
            v.node_id = fid(base + 301);
        }
        for (j, v) in g.vertices.iter_mut().enumerate() {
            v.id = fid(base + 300 + j as u64);
        }
    }
    asset
}
fn session(groups: usize, nodes: usize) -> AuthoringSession {
    let mut session =
        AuthoringSession::new(DOC, Canvas::new(100., 100., Vec2::default(), 1.)).unwrap();
    session
        .edit("Fixture", None, |edit| {
            for (id, runtime) in [(X, "ParamX"), (Y, "ParamY")] {
                edit.create_parameter(Parameter {
                    id: id.into(),
                    runtime_id: runtime.into(),
                    minimum: -1.,
                    maximum: 1.,
                    ..Default::default()
                })?;
            }
            let physics = asset(groups, nodes);
            edit.set_physics(physics)
        })
        .unwrap();
    session
}
fn patch(changes: Vec<P>) -> PhysicsPatch {
    PhysicsPatch {
        asset_id: ASSET.into(),
        asset_changes: vec![],
        rig: Some(PhysicsRigPatch {
            group_id: fid(1000),
            changes,
        }),
    }
}
#[test]
fn physics_patches_share_untouched_rigs_and_keep_snapshots_history_and_json_independent() {
    let mut session = session(3, 3);
    let snapshot = session.read_snapshot();
    let original = snapshot.document().physics().unwrap();
    let mut edit = session.begin_owned_edit("Drag", None).unwrap();
    for value in [0.2, 0.4, 0.7] {
        edit.patch_physics(&patch(vec![P::VertexMobility {
            id: fid(1301),
            value,
        }]))
        .unwrap();
    }
    let candidate = edit.candidate_document().physics().unwrap();
    assert_eq!(original.data.settings[0].vertices[1].mobility, 0.95);
    assert_eq!(candidate.data.settings[0].vertices[1].mobility, 0.7);
    assert!(!Arc::ptr_eq(
        &original.data.settings[0],
        &candidate.data.settings[0]
    ));
    assert!(Arc::ptr_eq(
        &original.data.settings[1],
        &candidate.data.settings[1]
    ));
    assert!(Arc::ptr_eq(
        &original.data.settings[2],
        &candidate.data.settings[2]
    ));
    assert_eq!(session.history_lengths(), (1, 0));
    assert!(edit.commit_to(&mut session).unwrap().changed);
    assert_eq!(session.history_lengths(), (2, 0));
    let json = serde_json::to_value(session.document().physics().unwrap()).unwrap();
    let decoded: PhysicsAsset = serde_json::from_value(json).unwrap();
    assert_eq!(&decoded, session.document().physics().unwrap());
    session.undo().unwrap();
    assert_eq!(session.document().physics(), Some(original));
    session.redo().unwrap();
    assert_eq!(session.document().physics(), Some(&decoded));
}
#[test]
fn rejected_multi_field_patch_is_atomic_and_sdk_failure_cannot_commit() {
    let mut session = session(2, 3);
    let before = session.read_snapshot();
    let mut invalid = patch(vec![
        P::VertexMobility {
            id: fid(1301),
            value: 0.2,
        },
        P::OutputNode {
            id: fid(1200),
            node_id: fid(999),
        },
    ]);
    invalid
        .asset_changes
        .push(PhysicsAssetProperty::Fps(Some(60.)));
    assert!(!session.document().validate_physics_patch(&invalid).is_ok());
    let mut edit = session.begin_owned_edit("Invalid", None).unwrap();
    assert!(edit.patch_physics(&invalid).is_err());
    assert_eq!(
        edit.candidate_document().physics(),
        before.document().physics()
    );
    assert!(edit.commit_to(&mut session).is_err());
    assert_eq!(session.version(), before.version());
    assert_eq!(session.history_lengths(), (1, 0));
}
#[test]
fn binding_patches_prune_unused_ids_and_noop_edits_preserve_redo() {
    let mut session = session(1, 2);
    session
        .edit("Rebind", None, |edit| {
            edit.patch_physics(&patch(vec![P::InputParameter {
                id: fid(1100),
                target: PhysicsTarget::Resolved {
                    parameter_id: Y.into(),
                },
            }]))
        })
        .unwrap();
    let asset = session.document().physics().unwrap();
    assert_eq!(
        asset.data.settings[0].inputs[0].source.parameter_id(),
        Some(Y)
    );
    session.undo().unwrap();
    let before = session.history_lengths();
    let (_, receipt) = session
        .edit("Noop", None, |edit| {
            edit.patch_physics(&patch(vec![P::VertexMobility {
                id: fid(1301),
                value: 0.95,
            }]))
        })
        .unwrap();
    assert!(!receipt.changed);
    assert_eq!(session.history_lengths(), before);
    let stale = session.begin_owned_edit("Stale", None).unwrap();
    session.redo().unwrap();
    assert!(stale.commit_to(&mut session).is_err());
}
#[test]
fn rebinding_after_parameter_renames_preserves_other_rigs_wire_keys() {
    let mut session = session(2, 2);
    session
        .edit("Rename parameters", None, |edit| {
            let mut x = edit.candidate_document().get_parameter(X).unwrap().clone();
            x.runtime_id = "Renamed".into();
            edit.replace_parameter(x)?;
            let mut y = edit.candidate_document().get_parameter(Y).unwrap().clone();
            y.runtime_id = "ParamX".into();
            edit.replace_parameter(y)
        })
        .unwrap();
    session
        .edit("Rebind first rig", None, |edit| {
            edit.patch_physics(&patch(vec![P::InputParameter {
                id: fid(1100),
                target: PhysicsTarget::Resolved {
                    parameter_id: Y.into(),
                },
            }]))
        })
        .unwrap();
    let asset = session.document().physics().unwrap();
    assert_eq!(
        asset.data.settings[0].inputs[0].source.parameter_id(),
        Some(Y)
    );
    assert_eq!(
        asset.data.settings[1].inputs[0].source.parameter_id(),
        Some(X)
    );
    let encoded = zipzi_project::encode_project(session.document()).unwrap();
    let restored = zipzi_project::decode_project(&encoded).unwrap();
    assert_eq!(restored.physics(), Some(asset));
}

#[test]
fn new_channels_preserve_bound_and_unresolved_references_after_runtime_rename() {
    let mut session = session(2, 2);
    session
        .edit("Rename", None, |e| {
            let mut x = e.candidate_document().get_parameter(X).unwrap().clone();
            x.runtime_id = "RenamedX".into();
            e.replace_parameter(x)
        })
        .unwrap();
    let mut asset = session.document().physics().unwrap().clone();
    let mut input = asset.data.settings[0].inputs[0].clone();
    input.id = fid(9999);
    input.source = PhysicsTarget::Unresolved {
        runtime_id: "ParamX".into(),
    };
    Arc::make_mut(&mut asset.data.settings[0])
        .inputs
        .push(input);
    asset.reconcile_counts();
    session
        .edit("Add unresolved", None, |e| e.set_physics(asset))
        .unwrap();
    let asset = session.document().physics().unwrap();
    assert_eq!(
        asset.data.settings[1].inputs[0].source.parameter_id(),
        Some(X)
    );
    assert!(matches!(
        asset.data.settings[0].inputs[1].source,
        PhysicsTarget::Unresolved { .. }
    ));
    let encoded = zipzi_project::encode_project(session.document()).unwrap();
    assert_eq!(
        zipzi_project::decode_project(&encoded).unwrap().physics(),
        Some(asset)
    );
}

#[test]
#[ignore = "release performance measurement; no timing assertion"]
fn physics_field_patch_benchmark() {
    use std::{hint::black_box, time::Instant};
    for (groups, nodes, iterations) in [(16, 4, 2000), (192, 24, 600)] {
        let session = session(groups, nodes);
        let original = session.document().physics().unwrap();
        let mut replacement = session.begin_owned_edit("Setter", None).unwrap();
        let mut copy = original.clone();
        let started = Instant::now();
        for i in 0..iterations {
            Arc::make_mut(&mut copy.data.settings[0]).vertices[1].mobility =
                if i % 2 == 0 { 0.2 } else { 0.8 };
            replacement.set_physics(black_box(copy.clone())).unwrap();
        }
        let setter = started.elapsed();
        let mut field = session.begin_owned_edit("Patch", None).unwrap();
        let started = Instant::now();
        for i in 0..iterations {
            // Retained readers force copy-on-write on each event, as preview workers do.
            let snapshot = field.candidate_document().fork_candidate();
            field
                .patch_physics(&patch(vec![P::VertexMobility {
                    id: fid(1301),
                    value: if i % 2 == 0 { 0.2 } else { 0.8 },
                }]))
                .unwrap();
            black_box(snapshot.physics());
        }
        let patch_time = started.elapsed();
        println!("physics fields: groups={groups} nodes={nodes} iterations={iterations} setter_us/event={:.2} patch_us/event={:.2} speedup={:.2}x",setter.as_secs_f64()*1e6/iterations as f64,patch_time.as_secs_f64()*1e6/iterations as f64,setter.as_secs_f64()/patch_time.as_secs_f64());
    }
}

#[test]
fn asset_fps_patch_supports_empty_assets_and_preserves_history_on_noop_or_error() {
    let mut session = session(0, 0);
    let mut patch = PhysicsPatch {
        asset_id: ASSET.into(),
        asset_changes: vec![PhysicsAssetProperty::Fps(Some(60.))],
        rig: None,
    };
    session
        .edit("FPS", None, |edit| edit.patch_physics(&patch))
        .unwrap();
    assert_eq!(
        session.document().physics().unwrap().data.meta.fps,
        Some(60.)
    );
    session.undo().unwrap();
    let history = session.history_lengths();
    patch.asset_changes = vec![PhysicsAssetProperty::Fps(Some(30.))];
    let (_, receipt) = session
        .edit("Noop", None, |edit| edit.patch_physics(&patch))
        .unwrap();
    assert!(!receipt.changed);
    assert_eq!(session.history_lengths(), history);
    patch.asset_changes = vec![PhysicsAssetProperty::Fps(Some(f32::NAN))];
    let before = session.read_snapshot();
    assert!(session
        .edit("Invalid", None, |edit| edit.patch_physics(&patch))
        .is_err());
    assert_eq!(session.document().physics(), before.document().physics());
    assert_eq!(session.history_lengths(), history);
    for fps in [Some(0.), None] {
        patch.asset_changes = vec![PhysicsAssetProperty::Fps(fps)];
        session
            .edit("Variable FPS", None, |edit| edit.patch_physics(&patch))
            .unwrap();
        assert_eq!(session.document().physics().unwrap().data.meta.fps, fps);
    }
}

#[test]
fn unified_field_patch_paths_match_local_patches_and_share_untouched_rigs() {
    use zipzi_core::document::PhysicsEdit;
    let mut session = session(3, 3);
    let snapshot = session.read_snapshot();
    let original = snapshot.document().physics().unwrap();
    let patch = patch(vec![P::VertexMobility {
        id: fid(1301),
        value: 0.25,
    }]);
    let mut expected_changes = None;
    for route in 0..3 {
        let receipt = match route {
            0 => {
                session
                    .edit("Local patch", None, |e| e.patch_physics(&patch))
                    .unwrap()
                    .1
            }
            1 => {
                session
                    .edit("Unified patch", None, |e| {
                        e.edit_physics(PhysicsEdit::Patch {
                            patch: patch.clone(),
                        })
                    })
                    .unwrap()
                    .1
            }
            _ => {
                let plan = session
                    .plan_physics_edit(PhysicsEdit::Patch {
                        patch: patch.clone(),
                    })
                    .unwrap();
                session.execute_physics_plan(&plan).unwrap()
            }
        };
        if let Some(expected) = &expected_changes {
            assert_eq!(&receipt.changes, expected);
        } else {
            expected_changes = Some(receipt.changes);
        }
        let current = session.document().physics().unwrap();
        assert_eq!(current.data.settings[0].vertices[1].mobility, 0.25);
        for index in 1..3 {
            assert!(Arc::ptr_eq(
                &original.data.settings[index],
                &current.data.settings[index]
            ));
        }
        session.undo().unwrap();
        assert_eq!(session.document().physics(), Some(original));
    }
}
