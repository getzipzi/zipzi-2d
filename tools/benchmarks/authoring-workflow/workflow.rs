//! Paired authoring workflow. The adapter is selected by checkout.
use serde_json::json;
use std::{
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::Instant,
};
use zipzi_core::{Parameter, Vec2};

#[path = "support/authoring_workflow_adapter.rs"]
mod adapter;
#[path = "support/allocation.rs"]
mod allocation;
use adapter::Session;
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn measured<T>(stage: &str, step: usize, run: impl FnOnce() -> Result<T>) -> Result<T> {
    let before = allocation::reset_peak();
    let start = Instant::now();
    let result = run();
    let elapsed = start.elapsed();
    let after = allocation::LIVE.load(Ordering::Relaxed);
    let peak = allocation::PEAK.load(Ordering::Relaxed);
    println!(
        "{}",
        json!({"stage":stage,"step":step,"elapsed_ns":elapsed.as_nanos(),
        "rust_live_before":before,"rust_live_after":after,"rust_peak":peak,
        "ok":result.is_ok(),"error":result.as_ref().err().map(|e| e.to_string())})
    );
    result
}

fn project_signature(session: &Session) -> Result<serde_json::Value> {
    let mut texture_bytes = 0;
    let mut texture_hashes = Vec::new();
    for asset in session.assets() {
        let pixels = session.pixels(&asset)?;
        texture_bytes += pixels.len();
        // Stable portable fingerprint of the decoded pixels, independent of asset IDs.
        let hash = pixels.iter().fold(14695981039346656037_u64, |hash, &byte| {
            (hash ^ u64::from(byte)).wrapping_mul(1099511628211)
        });
        texture_hashes.push(format!("{hash:016x}"));
    }
    texture_hashes.sort();
    Ok(json!({"meshes":session.meshes().len(),
        "vertices":session.meshes().iter().map(|id|session.mesh(id).vertex_ids.len()).sum::<usize>(),
        "parameters":session.parameter_count(), "motions":session.motion_count(),
        "decoded_texture_bytes":texture_bytes,"decoded_texture_hashes":texture_hashes}))
}

fn parameter(index: u128) -> Parameter {
    Parameter {
        id: uuid::Uuid::from_u128(0x378bcc50_94a5_464f_a210_000000000000 + index).to_string(),
        runtime_id: format!("PhaseSixParameter{index}"),
        name: format!("Phase 6 Parameter {index}"),
        minimum: -1.,
        maximum: 1.,
        default_value: 0.,
        ..Default::default()
    }
}

fn main() -> Result<()> {
    assert!(!cfg!(debug_assertions), "Release workflow required");
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    assert_eq!(args.len(), 3, "MODEL3 OUTPUT_DIR history|workflow");
    let model = PathBuf::from(&args[0]);
    let output = PathBuf::from(&args[1]);
    let history_probe = args[2] == "history";
    assert!(model.is_absolute() && output.is_absolute() && output.is_dir());
    let mut session = measured("load", 0, || Session::import(&model, &output))?;
    let signature = measured("read_loaded_resources", 0, || project_signature(&session))?;
    let mesh = session
        .meshes()
        .into_iter()
        .find(|id| session.mesh(id).runtime_id == "ArtMesh266")
        .ok_or("Mao ArtMesh266 missing")?;
    let mesh_record = session.mesh(&mesh);
    let original_name = mesh_record.name.clone();
    let vertex = mesh_record.vertex_ids[0];
    let original = mesh_record.base_positions[0];
    drop(mesh_record);
    println!(
        "{}",
        json!({"signature":signature,"mesh":"ArtMesh266","vertex":vertex,
        "initial_position":[original.x,original.y], "history":session.history()})
    );

    if history_probe {
        let mut accepted = 0;
        for index in 0..80 {
            if measured("history_rename", index, || {
                session.rename(&mesh, &format!("History {index}"))
            })
            .is_err()
            {
                break;
            }
            accepted += 1;
        }
        let retained = session.history();
        let live_at_retention = allocation::LIVE.load(Ordering::Relaxed);
        let mut undone = 0;
        for step in 0..retained["undo"].as_u64().unwrap() as usize {
            measured("history_undo", step, || session.undo())?;
            undone += 1;
        }
        assert_eq!(session.history()["undo"], 0);
        for step in 0..undone {
            measured("history_redo", step, || session.redo())?;
        }
        if accepted > 0 {
            assert_eq!(
                session.mesh(&mesh).name,
                format!("History {}", accepted - 1)
            );
        }
        println!(
            "{}",
            json!({"complete":true,"accepted":accepted,"usable_undo":undone,
            "usable_redo":undone,"rust_live_at_retention":live_at_retention,"history":retained})
        );
        return Ok(());
    }

    let initial = output.join("initial.zipzi");
    let copy = output.join("copy.zipzi");
    measured("initial_save", 0, || session.save_as(&initial))?;
    assert!(!session.dirty());
    let moved = Vec2::new(original.x + 0.125, original.y - 0.0625);
    measured("rename", 0, || {
        session.rename(&mesh, "Paired workflow rename")
    })?;
    assert_eq!(session.mesh(&mesh).name, "Paired workflow rename");
    measured("vertex", 0, || session.move_vertex(&mesh, vertex, moved))?;
    assert_eq!(session.mesh(&mesh).base_positions[0], moved);
    let count = session.parameter_count();
    let parameters = (1..=32).map(parameter).collect();
    measured("batch_32_parameters", 0, || {
        session.create_parameters(parameters)
    })?;
    assert_eq!(session.parameter_count(), count + 32);
    assert!(session.dirty());
    assert_eq!(session.history()["undo"], 3);
    for step in 0..3 {
        measured("undo", step, || session.undo())?;
        match step {
            0 => assert_eq!(session.parameter_count(), count),
            1 => assert_eq!(session.mesh(&mesh).base_positions[0], original),
            2 => assert_eq!(session.mesh(&mesh).name, original_name),
            _ => unreachable!(),
        }
    }
    assert!(!session.dirty());
    for step in 0..3 {
        measured("redo", step, || session.redo())?;
        match step {
            0 => assert_eq!(session.mesh(&mesh).name, "Paired workflow rename"),
            1 => assert_eq!(session.mesh(&mesh).base_positions[0], moved),
            2 => assert_eq!(session.parameter_count(), count + 32),
            _ => unreachable!(),
        }
    }
    assert!(session.dirty());
    measured("save", 0, || session.save(&initial))?;
    assert!(!session.dirty());
    measured("save_as", 0, || session.save_as(&copy))?;
    assert!(!session.dirty());
    let saved_history = session.history();
    assert_eq!(saved_history["undo"], 3);
    measured("undo_after_save_as", 0, || session.undo())?;
    assert!(session.dirty());
    measured("redo_after_save_as", 0, || session.redo())?;
    assert!(!session.dirty());
    let final_signature = project_signature(&session)?;
    let live_with_history = allocation::LIVE.load(Ordering::Relaxed);
    drop(session);
    let mut reopened = measured("reopen_saved_copy", 0, || Session::open(&copy, &output))?;
    assert_eq!(reopened.mesh(&mesh).name, "Paired workflow rename");
    assert_eq!(reopened.mesh(&mesh).base_positions[0], moved);
    assert_eq!(reopened.parameter_count(), count + 32);
    let reopened_signature = measured("read_reopened_resources", 0, || {
        project_signature(&reopened)
    })?;
    assert_eq!(reopened_signature, final_signature);
    assert_eq!(reopened.history()["undo"], 0);
    let live_reopened = allocation::LIVE.load(Ordering::Relaxed);
    measured("edit_reopened", 0, || {
        reopened.move_vertex(&mesh, vertex, original)
    })?;
    assert_eq!(reopened.mesh(&mesh).base_positions[0], original);
    println!(
        "{}",
        json!({"complete":true, "usable_undo":3, "usable_redo":3,
        "rust_live_with_history":live_with_history,"rust_live_reopened":live_reopened,
        "history":saved_history,"archive_bytes":copy.metadata()?.len(),
        "signature":final_signature})
    );
    Ok(())
}
