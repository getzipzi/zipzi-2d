use kasane_core::{
    document::{PhysicsAsset, PhysicsPatch, PhysicsProperty as P},
    Canvas, Parameter, Vec2,
};
use kasane_sdk::AuthoringSession;
use std::sync::Arc;

const DOC: &str = "00000000-0000-4000-8000-00000000f001";
const ASSET: &str = "00000000-0000-4000-8000-00000000f002";
const X: &str = "00000000-0000-4000-8000-00000000f003";
const Y: &str = "00000000-0000-4000-8000-00000000f004";
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
    serde_json::from_value(serde_json::json!({"id":ASSET,"data":data,"parameter_bindings":{"ParamX":X,"ParamY":Y},"opaque_source_ids":null})).unwrap()
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
            edit.set_physics(asset(groups, nodes))
        })
        .unwrap();
    session
}
fn patch(changes: Vec<P>) -> PhysicsPatch {
    PhysicsPatch {
        asset_id: ASSET.into(),
        group_id: "Rig0".into(),
        changes,
    }
}
#[test]
fn physics_patches_share_untouched_rigs_and_keep_snapshots_history_and_json_independent() {
    let mut session = session(3, 3);
    let snapshot = session.read_snapshot();
    let original = snapshot.document().physics().unwrap();
    let mut edit = session.begin_owned_edit("Drag", None).unwrap();
    for value in [0.2, 0.4, 0.7] {
        edit.patch_physics(&patch(vec![P::VertexMobility { index: 1, value }]))
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
    let invalid = patch(vec![
        P::Fps(Some(60.)),
        P::VertexMobility {
            index: 1,
            value: 0.2,
        },
        P::OutputVertex {
            index: 0,
            value: 999,
        },
    ]);
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
                index: 0,
                parameter_id: Y.into(),
            }]))
        })
        .unwrap();
    let asset = session.document().physics().unwrap();
    assert_eq!(asset.data.settings[0].inputs[0].source.id, "ParamY");
    assert_eq!(asset.parameter_bindings.len(), 1);
    assert_eq!(asset.parameter_bindings["ParamY"], Y);
    session.undo().unwrap();
    let before = session.history_lengths();
    let (_, receipt) = session
        .edit("Noop", None, |edit| {
            edit.patch_physics(&patch(vec![P::VertexMobility {
                index: 1,
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
                index: 0,
                parameter_id: Y.into(),
            }]))
        })
        .unwrap();
    let asset = session.document().physics().unwrap();
    assert_eq!(asset.data.settings[0].inputs[0].source.id, "ParamY");
    assert_eq!(asset.data.settings[1].inputs[0].source.id, "ParamX");
    assert_eq!(asset.parameter_bindings["ParamX"], X);
    assert_eq!(asset.parameter_bindings["ParamY"], Y);
    let encoded = kasane_project::encode_project(session.document()).unwrap();
    let restored = kasane_project::decode_project(&encoded).unwrap();
    assert_eq!(restored.physics(), Some(asset));
}

#[test]
fn new_channel_binding_keys_preserve_bound_and_unresolved_references() {
    const Z: &str = "00000000-0000-4000-8000-00000000f005";
    let mut session = session(2, 2);
    session
        .edit("Reuse runtime ID", None, |edit| {
            let mut x = edit.candidate_document().get_parameter(X).unwrap().clone();
            x.runtime_id = "RenamedX".into();
            edit.replace_parameter(x)?;
            edit.create_parameter(Parameter {
                id: Z.into(),
                runtime_id: "ParamX".into(),
                minimum: -1.,
                maximum: 1.,
                ..Default::default()
            })
        })
        .unwrap();
    let mut asset = session.document().physics().unwrap().clone();
    assert_eq!(asset.parameter_binding_key(X, "RenamedX"), "ParamX");
    assert_eq!(asset.parameter_binding_key(Z, "ParamX"), "ParamX~1");
    let mut unresolved = asset.data.settings[1].inputs[0].clone();
    unresolved.source.id = "ParamX~1".into();
    Arc::make_mut(&mut asset.data.settings[1])
        .inputs
        .push(unresolved);
    asset.data.meta.input_count += 1;
    let key = asset.parameter_binding_key(Z, "ParamX");
    assert_eq!(key, "ParamX~2");
    asset.parameter_bindings.insert(key.clone(), Z.into());
    let mut input = asset.data.settings[0].inputs[0].clone();
    input.source.id = key;
    Arc::make_mut(&mut asset.data.settings[0])
        .inputs
        .push(input);
    asset.data.meta.input_count += 1;
    session
        .edit("Add channel", None, |edit| edit.set_physics(asset))
        .unwrap();
    let asset = session.document().physics().unwrap();
    assert_eq!(asset.parameter_bindings["ParamX"], X);
    assert_eq!(asset.parameter_bindings["ParamX~2"], Z);
    assert!(!asset.parameter_bindings.contains_key("ParamX~1"));
    assert_eq!(asset.data.settings[1].inputs[0].source.id, "ParamX");
    let encoded = kasane_project::encode_project(session.document()).unwrap();
    assert_eq!(
        kasane_project::decode_project(&encoded).unwrap().physics(),
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
                    index: 1,
                    value: if i % 2 == 0 { 0.2 } else { 0.8 },
                }]))
                .unwrap();
            black_box(snapshot.physics());
        }
        let patch_time = started.elapsed();
        println!("physics fields: groups={groups} nodes={nodes} iterations={iterations} setter_us/event={:.2} patch_us/event={:.2} speedup={:.2}x",setter.as_secs_f64()*1e6/iterations as f64,patch_time.as_secs_f64()*1e6/iterations as f64,setter.as_secs_f64()/patch_time.as_secs_f64());
    }
}
