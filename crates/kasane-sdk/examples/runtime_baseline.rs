//! R0 CPU/allocation baseline. JSONL on stdout; source assets are never written.
//! cargo run --locked --release -p kasane-sdk --example runtime_baseline -- OUTPUT [MODEL3]
use kasane_core::document::MotionEdit;
use kasane_sdk::AuthoringSession;
use serde_json::json;
use std::{hint::black_box, path::PathBuf, sync::atomic::Ordering, time::Instant};
#[path = "support/allocation.rs"]
mod allocation;
#[path = "support/runtime_fixture.rs"]
mod fixture;

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn measure<T>(case: &str, index: usize, f: impl FnOnce() -> Result<T>) -> Result<T> {
    let base = allocation::reset_peak();
    let started = Instant::now();
    let value = f()?;
    let elapsed = started.elapsed().as_nanos();
    let retained = allocation::LIVE
        .load(Ordering::Relaxed)
        .saturating_sub(base);
    let peak = allocation::PEAK
        .load(Ordering::Relaxed)
        .saturating_sub(base);
    println!(
        "{}",
        json!({"type":"sample", "case":case, "index":index,
        "warmup": index < 4, "wall_ns":elapsed, "retained_bytes":retained, "peak_bytes":peak})
    );
    Ok(value)
}

fn probe(sdk: &mut AuthoringSession, output: PathBuf, session_live_bytes: usize) -> Result<()> {
    let mesh = sdk.mesh_ids()[0].clone();
    let original = sdk.mesh(&mesh).unwrap().name;
    let mut texture_bytes = 0;
    for id in sdk.asset_ids() {
        texture_bytes += sdk.read_texture(id)?.rgba.len();
    }
    let initial_history = sdk.history_lengths();
    println!(
        "{}",
        json!({"type":"metadata", "debug_assertions":cfg!(debug_assertions),
        "meshes":sdk.mesh_ids().len(), "parameters":sdk.parameter_ids().len(),
        "vertices": sdk.mesh_ids().iter().map(|id| sdk.mesh(id).unwrap().vertex_ids.len()).sum::<usize>(),
        "motions":sdk.motion_ids().len(), "decoded_texture_bytes":texture_bytes,
        "content_estimated_bytes":sdk.estimated_content_bytes(), "history_estimated_bytes":sdk.history_state().estimated_bytes,
        "session_size_of":std::mem::size_of::<AuthoringSession>(), "session_live_bytes":session_live_bytes,
        "samples":32, "warmup":4})
    );
    for index in 0..36 {
        let version = sdk.version();
        let history = sdk.history_lengths();
        let snapshot = measure("snapshot_capture", index, || Ok(sdk.read_snapshot()))?;
        assert_eq!(snapshot.version(), version);
        drop(snapshot);
        let edit = measure("candidate_begin", index, || {
            Ok(sdk.begin_owned_edit("probe", Some(version))?)
        })?;
        assert_eq!(edit.base_version(), version);
        drop(edit);
        let plan = measure("motion_plan", index, || {
            Ok(sdk.plan_motion_edit(MotionEdit::CreateGroup {
                id: fixture::id(20),
                name: "Probe group".into(),
            })?)
        })?;
        black_box(plan);
        assert_eq!(sdk.version(), version);
        assert_eq!(sdk.history_lengths(), history);
        let mut edit = sdk.begin_owned_edit("R0 rename", Some(version))?;
        edit.rename_mesh(&mesh, &format!("R0 name {index}"))?;
        let receipt = measure("metadata_commit", index, || Ok(edit.commit_to(sdk)?))?;
        assert!(receipt.changed);
        assert_eq!(receipt.before, version);
        assert_eq!(sdk.history_lengths(), (history.0 + 1, 0));
        measure("undo", index, || Ok(sdk.undo()?))?;
        assert_eq!(sdk.mesh(&mesh).unwrap().name, original);
        assert_eq!(sdk.history_lengths().0, initial_history.0);
        assert_eq!(sdk.drain_events().len(), 2);
        let preview_version = sdk.version();
        let mut preview = sdk.motion_preview();
        if let Some(clip) = sdk.motion_ids().first() {
            preview.schedule_motion(clip, 0.)?;
        }
        measure("motion_seek_1s", index, || Ok(preview.seek(1.)?))?;
        assert_eq!(sdk.version(), preview_version);
        assert!(sdk.drain_events().is_empty());
    }
    // Save once into a fresh destination, then measure warm saves independently.
    // Save can relocate descriptors; capture version/history AFTER the first save.
    let receipt = measure("save_first", 4, || Ok(sdk.save_project(&output, None)?))?;
    let version = sdk.version();
    let history = sdk.history_lengths();
    for index in 0..12 {
        let saved = measure("save_warm", index, || {
            Ok(sdk.save_project(&output, Some(version))?)
        })?;
        assert_eq!(saved.manifest, receipt.manifest);
        assert_eq!(sdk.version(), version);
        assert_eq!(sdk.history_lengths(), history);
    }
    let mut reopened = AuthoringSession::new(&fixture::id(99), kasane_core::ModelSetup::default())?;
    reopened.open_project(&receipt.manifest, None)?;
    assert_eq!(reopened.mesh_ids(), sdk.mesh_ids());
    assert_eq!(reopened.motion_ids(), sdk.motion_ids());
    for id in sdk.motion_ids() {
        assert_eq!(reopened.motion(id), sdk.motion(id));
    }
    assert_eq!(
        reopened.read_texture(&sdk.asset_ids()[0])?.rgba,
        sdk.read_texture(&sdk.asset_ids()[0])?.rgba
    );
    println!(
        "{}",
        json!({"type":"complete", "validated":true, "saved_manifest":receipt.manifest})
    );
    Ok(())
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(
        args.next()
            .ok_or("supply a fresh absolute output directory")?,
    );
    if !output.is_absolute() || output.exists() {
        return Err("output must be absolute and must not exist".into());
    }
    let session_base = allocation::LIVE.load(Ordering::Relaxed);
    let mut sdk = if let Some(path) = args.next() {
        let mut sdk = AuthoringSession::new(&fixture::id(1), kasane_core::ModelSetup::default())?;
        sdk.import_model3(&PathBuf::from(path), None)?;
        sdk.drain_events();
        sdk
    } else {
        let sdk = fixture::build(128, 12, 1024)?;
        let low = sdk.evaluate(&[(fixture::id(3), -1.)].into_iter().collect())?;
        let high = sdk.evaluate(&[(fixture::id(3), 1.)].into_iter().collect())?;
        assert_eq!(
            high.drawables[0].positions[0].x - low.drawables[0].positions[0].x,
            2.
        );
        assert_eq!(sdk.motion_ids().len(), 1);
        sdk
    };
    let session_live_bytes = allocation::LIVE
        .load(Ordering::Relaxed)
        .saturating_sub(session_base);
    probe(&mut sdk, output, session_live_bytes)
}
