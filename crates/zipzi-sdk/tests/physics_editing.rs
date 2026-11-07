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
    let mut session = AuthoringSession::new(
        zipzi_sdk::Snapshot::new(
            zipzi_core::state::DocumentState::new(
                DOC,
                (Canvas::new(100., 100., Vec2::default(), 1.)).into(),
            )
            .unwrap(),
            zipzi_sdk::ResourceView::default(),
        )
        .unwrap(),
    );
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
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
                edit.replace_physics(Some(physics))
            })
            .and_then(|value| {
                let prepared = draft.prepare("Fixture")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
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
    let original = snapshot.snapshot.document().physics().as_ref().unwrap();
    let mut edit = session.begin_draft();
    for value in [0.2, 0.4, 0.7] {
        edit.batch(edit.source(), |c| {
            c.patch_physics(&patch(vec![P::VertexMobility {
                id: fid(1301),
                value,
            }]))
        })
        .unwrap();
    }
    let candidate_snapshot = edit.read_snapshot().unwrap();
    let candidate = candidate_snapshot
        .snapshot
        .document()
        .physics()
        .as_ref()
        .unwrap();
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
    assert_eq!(
        {
            let history = session.history_state();
            (history.undo_steps, history.redo_steps)
        },
        (1, 0)
    );
    assert!(!session
        .publish(edit.prepare("Drag").unwrap())
        .unwrap()
        .writes
        .is_empty());
    assert_eq!(
        {
            let history = session.history_state();
            (history.undo_steps, history.redo_steps)
        },
        (2, 0)
    );
    let json =
        serde_json::to_value(session.snapshot().document().physics().as_ref().unwrap()).unwrap();
    let decoded: PhysicsAsset = serde_json::from_value(json).unwrap();
    assert_eq!(
        &decoded,
        session
            .snapshot()
            .document()
            .physics()
            .as_ref()
            .unwrap()
            .as_ref()
    );
    session.undo(session.version()).unwrap();
    assert_eq!(
        session.snapshot().document().physics(),
        &Some(original.clone())
    );
    session.redo(session.version()).unwrap();
    assert_eq!(
        session
            .snapshot()
            .document()
            .physics()
            .as_ref()
            .unwrap()
            .as_ref(),
        &decoded
    );
}
#[test]
fn rejected_multi_field_patch_is_atomic_and_sdk_failure_cannot_commit() {
    let session = session(2, 3);
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
    assert!(!session
        .snapshot()
        .document()
        .validate_physics_patch(&invalid)
        .is_ok());
    let mut edit = session.begin_draft();
    assert!(edit
        .batch(edit.source(), |c| c.patch_physics(&invalid))
        .is_err());
    assert!(edit.read_snapshot().is_err());
    assert!(edit.prepare("Invalid").is_err());
    assert_eq!(session.read_snapshot().source, before.source);
    assert_eq!(
        {
            let history = session.history_state();
            (history.undo_steps, history.redo_steps)
        },
        (1, 0)
    );
}
#[test]
fn binding_patches_prune_unused_ids_and_noop_edits_preserve_redo() {
    let mut session = session(1, 2);
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.patch_physics(&patch(vec![P::InputParameter {
                    id: fid(1100),
                    target: PhysicsTarget::Resolved {
                        parameter_id: Y.into(),
                    },
                }]))
            })
            .and_then(|value| {
                let prepared = draft.prepare("Rebind")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let asset = session.snapshot().document().physics().as_ref().unwrap();
    assert_eq!(
        asset.data.settings[0].inputs[0].source.parameter_id(),
        Some(Y)
    );
    session.undo(session.version()).unwrap();
    let before = {
        let history = session.history_state();
        (history.undo_steps, history.redo_steps)
    };
    let (_, receipt) = {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.patch_physics(&patch(vec![P::VertexMobility {
                    id: fid(1301),
                    value: 0.95,
                }]))
            })
            .and_then(|value| {
                let prepared = draft.prepare("Noop")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(receipt.writes.is_empty());
    assert_eq!(
        {
            let history = session.history_state();
            (history.undo_steps, history.redo_steps)
        },
        before
    );
    let stale = session.begin_draft();
    session.redo(session.version()).unwrap();
    assert!(session.publish(stale.prepare("stale").unwrap()).is_err());
}
#[test]
fn rebinding_after_parameter_renames_preserves_other_rigs_wire_keys() {
    let mut session = session(2, 2);
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                let mut x = edit.read_document().unwrap().parameter(X).unwrap().clone();
                x.runtime_id = "Renamed".into();
                edit.replace_parameter(x)?;
                let mut y = edit.read_document().unwrap().parameter(Y).unwrap().clone();
                y.runtime_id = "ParamX".into();
                edit.replace_parameter(y)
            })
            .and_then(|value| {
                let prepared = draft.prepare("Rename parameters")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| {
                edit.patch_physics(&patch(vec![P::InputParameter {
                    id: fid(1100),
                    target: PhysicsTarget::Resolved {
                        parameter_id: Y.into(),
                    },
                }]))
            })
            .and_then(|value| {
                let prepared = draft.prepare("Rebind first rig")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let asset = session.snapshot().document().physics().as_ref().unwrap();
    assert_eq!(
        asset.data.settings[0].inputs[0].source.parameter_id(),
        Some(Y)
    );
    assert_eq!(
        asset.data.settings[1].inputs[0].source.parameter_id(),
        Some(X)
    );
    let restored = super::physics_authoring::roundtrip(&session);
    assert_eq!(restored.physics().as_ref(), Some(asset));
}

#[test]
fn new_channels_preserve_bound_and_unresolved_references_after_runtime_rename() {
    let mut session = session(2, 2);
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |e| {
                let mut x = e.read_document().unwrap().parameter(X).unwrap().clone();
                x.runtime_id = "RenamedX".into();
                e.replace_parameter(x)
            })
            .and_then(|value| {
                let prepared = draft.prepare("Rename")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let mut asset = session
        .snapshot()
        .document()
        .physics()
        .as_ref()
        .unwrap()
        .as_ref()
        .clone();
    let mut input = asset.data.settings[0].inputs[0].clone();
    input.id = fid(9999);
    input.source = PhysicsTarget::Unresolved {
        runtime_id: "ParamX".into(),
    };
    Arc::make_mut(&mut asset.data.settings[0])
        .inputs
        .push(input);
    asset.reconcile_counts();
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |e| e.replace_physics(Some(asset)))
            .and_then(|value| {
                let prepared = draft.prepare("Add unresolved")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    let asset = session.snapshot().document().physics().as_ref().unwrap();
    assert_eq!(
        asset.data.settings[1].inputs[0].source.parameter_id(),
        Some(X)
    );
    assert!(matches!(
        asset.data.settings[0].inputs[1].source,
        PhysicsTarget::Unresolved { .. }
    ));
    let restored = super::physics_authoring::roundtrip(&session);
    assert_eq!(restored.physics().as_ref(), Some(asset));
}

#[test]
#[ignore = "release performance measurement; no timing assertion"]
fn physics_field_patch_benchmark() {
    use std::{hint::black_box, time::Instant};
    for (groups, nodes, iterations) in [(16, 4, 2000), (192, 24, 600)] {
        let session = session(groups, nodes);
        let original = session.snapshot().document().physics().as_ref().unwrap();
        let mut replacement = session.begin_draft();
        let mut copy = original.as_ref().clone();
        let started = Instant::now();
        for i in 0..iterations {
            Arc::make_mut(&mut copy.data.settings[0]).vertices[1].mobility =
                if i % 2 == 0 { 0.2 } else { 0.8 };
            replacement
                .batch(replacement.source(), |c| {
                    c.replace_physics(Some(black_box(copy.clone())))
                })
                .unwrap();
        }
        let setter = started.elapsed();
        let mut field = session.begin_draft();
        let started = Instant::now();
        for i in 0..iterations {
            // Retained readers force copy-on-write on each event, as preview workers do.
            let snapshot = field.read_snapshot().unwrap();
            field
                .batch(field.source(), |c| {
                    c.patch_physics(&patch(vec![P::VertexMobility {
                        id: fid(1301),
                        value: if i % 2 == 0 { 0.2 } else { 0.8 },
                    }]))
                })
                .unwrap();
            black_box(snapshot.snapshot.document().physics());
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
    {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| edit.patch_physics(&patch))
            .and_then(|value| {
                let prepared = draft.prepare("FPS")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert_eq!(
        session
            .snapshot()
            .document()
            .physics()
            .as_ref()
            .unwrap()
            .data
            .meta
            .fps,
        Some(60.)
    );
    session.undo(session.version()).unwrap();
    let history = {
        let history = session.history_state();
        (history.undo_steps, history.redo_steps)
    };
    patch.asset_changes = vec![PhysicsAssetProperty::Fps(Some(30.))];
    let (_, receipt) = {
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| edit.patch_physics(&patch))
            .and_then(|value| {
                let prepared = draft.prepare("Noop")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .unwrap();
    assert!(receipt.writes.is_empty());
    assert_eq!(
        {
            let history = session.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    patch.asset_changes = vec![PhysicsAssetProperty::Fps(Some(f32::NAN))];
    let before = session.read_snapshot();
    assert!({
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |edit| edit.patch_physics(&patch))
            .and_then(|value| {
                let prepared = draft.prepare("Invalid")?;
                session.publish(prepared).map(|receipt| (value, receipt))
            })
    }
    .is_err());
    assert_eq!(
        session.snapshot().document().physics(),
        before.snapshot.document().physics()
    );
    assert_eq!(
        {
            let history = session.history_state();
            (history.undo_steps, history.redo_steps)
        },
        history
    );
    for fps in [Some(0.), None] {
        patch.asset_changes = vec![PhysicsAssetProperty::Fps(fps)];
        {
            let mut draft = session.begin_draft();
            draft
                .batch(draft.source(), |edit| edit.patch_physics(&patch))
                .and_then(|value| {
                    let prepared = draft.prepare("Variable FPS")?;
                    session.publish(prepared).map(|receipt| (value, receipt))
                })
        }
        .unwrap();
        assert_eq!(
            session
                .snapshot()
                .document()
                .physics()
                .as_ref()
                .unwrap()
                .data
                .meta
                .fps,
            fps
        );
    }
}

#[test]
fn unified_field_patch_paths_match_local_patches_and_share_untouched_rigs() {
    use zipzi_core::document::PhysicsEdit;
    let mut session = session(3, 3);
    let snapshot = session.read_snapshot();
    let original = snapshot.snapshot.document().physics().as_ref().unwrap();
    let patch = patch(vec![P::VertexMobility {
        id: fid(1301),
        value: 0.25,
    }]);
    let mut expected_changes = None;
    for route in 0..3 {
        let receipt = match route {
            0 => {
                {
                    let mut draft = session.begin_draft();
                    draft
                        .batch(draft.source(), |e| e.patch_physics(&patch))
                        .and_then(|value| {
                            let prepared = draft.prepare("Local patch")?;
                            session.publish(prepared).map(|receipt| (value, receipt))
                        })
                }
                .unwrap()
                .1
            }
            1 => {
                {
                    let mut draft = session.begin_draft();
                    draft
                        .batch(draft.source(), |e| {
                            e.edit_physics(PhysicsEdit::Patch {
                                patch: patch.clone(),
                            })
                        })
                        .and_then(|value| {
                            let prepared = draft.prepare("Unified patch")?;
                            session.publish(prepared).map(|receipt| (value, receipt))
                        })
                }
                .unwrap()
                .1
            }
            _ => {
                let plan = super::physics_authoring::prepare(
                    &session,
                    PhysicsEdit::Patch {
                        patch: patch.clone(),
                    },
                )
                .unwrap();
                session.publish(plan).unwrap()
            }
        };
        if let Some(expected) = &expected_changes {
            assert_eq!(&receipt.writes, expected);
        } else {
            expected_changes = Some(receipt.writes);
        }
        let current = session.snapshot().document().physics().as_ref().unwrap();
        assert_eq!(current.data.settings[0].vertices[1].mobility, 0.25);
        for index in 1..3 {
            assert!(Arc::ptr_eq(
                &original.data.settings[index],
                &current.data.settings[index]
            ));
        }
        session.undo(session.version()).unwrap();
        assert_eq!(
            session.snapshot().document().physics(),
            &Some(original.clone())
        );
    }
}
