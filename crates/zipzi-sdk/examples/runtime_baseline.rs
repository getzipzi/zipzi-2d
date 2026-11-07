//! R0 CPU/allocation baseline. JSONL on stdout; source assets are never written.
//! cargo run --locked --release -p zipzi-sdk --example runtime_baseline -- OUTPUT [MODEL3]
use serde_json::json;
use std::{hint::black_box, path::PathBuf, sync::atomic::Ordering, time::Instant};
use zipzi_core::document::MotionEdit;
use zipzi_sdk::{AuthoringSession, NameTarget, ProjectIo, ResourceStore, Snapshot};
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
    let document = sdk.snapshot().document();
    let mesh = document.mesh_order()[0].clone();
    let original = document.mesh(&mesh).unwrap().name.clone();
    let mut texture_bytes = 0;
    for id in document.asset_order() {
        texture_bytes += sdk
            .snapshot()
            .resources()
            .read_texture(document.asset(id).unwrap())?
            .rgba
            .len();
    }
    let initial_history = sdk.history_state();
    println!(
        "{}",
        json!({"type":"metadata", "debug_assertions":cfg!(debug_assertions),
        "meshes":sdk.snapshot().document().mesh_order().len(), "parameters":sdk.snapshot().document().parameter_order().len(),
        "vertices": sdk.snapshot().document().mesh_order().iter().map(|id| sdk.snapshot().document().mesh(id).unwrap().vertex_ids.len()).sum::<usize>(),
        "motions":sdk.snapshot().document().motion_order().len(), "decoded_texture_bytes":texture_bytes,
        "history_estimated_bytes":sdk.history_state().estimated_bytes,
        "session_size_of":std::mem::size_of::<AuthoringSession>(), "session_live_bytes":session_live_bytes,
        "samples":32, "warmup":4})
    );
    for index in 0..36 {
        let version = sdk.version();
        let history = sdk.history_state();
        let snapshot = measure("snapshot_capture", index, || Ok(sdk.read_snapshot()))?;
        assert_eq!(snapshot.source, zipzi_sdk::QuerySource::Published(version));
        drop(snapshot);
        let edit = measure("candidate_begin", index, || Ok(sdk.begin_draft()))?;
        assert_eq!(edit.source().base, version);
        drop(edit);
        let plan = measure("motion_plan", index, || {
            let mut draft = sdk.begin_draft();
            draft.batch(draft.source(), |c| {
                c.edit_motion(MotionEdit::CreateGroup {
                    id: fixture::id(20),
                    name: "Probe group".into(),
                })
            })?;
            Ok(draft.prepare("Create motion group")?)
        })?;
        black_box(plan);
        assert_eq!(sdk.version(), version);
        assert_eq!(sdk.history_state(), history);
        let mut draft = sdk.begin_draft();
        draft.batch(draft.source(), |c| {
            c.rename(NameTarget::Mesh(&mesh), format!("R0 name {index}"))
        })?;
        let receipt = measure("metadata_commit", index, || {
            Ok(sdk.publish(draft.prepare("R0 rename")?)?)
        })?;
        assert!(!receipt.writes.is_empty());
        assert_eq!(receipt.version.revision, version.revision + 1);
        assert_eq!(sdk.history_state().undo_steps, history.undo_steps + 1);
        measure("undo", index, || Ok(sdk.undo(sdk.version())?))?;
        assert_eq!(
            sdk.snapshot().document().mesh(&mesh).unwrap().name,
            original
        );
        assert_eq!(sdk.history_state().undo_steps, initial_history.undo_steps);
        assert_eq!(sdk.take_events().len(), 2);
        let preview_version = sdk.version();
        let mut preview = zipzi_animation::MotionPreview::new(sdk.snapshot().document());
        if let Some(clip) = sdk.snapshot().document().motion_order().first() {
            preview.schedule_motion(clip, 0.)?;
        }
        measure("motion_seek_1s", index, || Ok(preview.seek(1.)?))?;
        assert_eq!(sdk.version(), preview_version);
        assert!(sdk.take_events().is_empty());
    }
    let io = ProjectIo::new(output.parent().unwrap(), ResourceStore::default());
    let receipt = measure("save_first", 4, || {
        Ok(sdk.save_as(&io, &output, None, || true)?)
    })?;
    let version = sdk.version();
    let history = sdk.history_state();
    for index in 0..12 {
        let saved = measure("save_warm", index, || Ok(sdk.save(&io, || true)?))?;
        assert_eq!(saved.path, receipt.path);
        assert_eq!(sdk.version(), version);
        assert_eq!(sdk.history_state(), history);
    }
    let reopened = io.load(&receipt.path, || true)?;
    let document = sdk.snapshot().document();
    assert_eq!(reopened.document.mesh_order(), document.mesh_order());
    assert_eq!(reopened.document.motion_order(), document.motion_order());
    for id in document.motion_order() {
        assert_eq!(reopened.document.motion(id), document.motion(id));
    }
    let asset = &document.asset_order()[0];
    assert_eq!(
        reopened
            .resources
            .read_texture(reopened.document.asset(asset).unwrap())?
            .rgba,
        sdk.snapshot()
            .resources()
            .read_texture(document.asset(asset).unwrap())?
            .rgba,
    );
    println!(
        "{}",
        json!({"type":"complete", "validated":true, "saved_path":receipt.path})
    );
    Ok(())
}

fn main() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let output = PathBuf::from(
        args.next()
            .ok_or("supply a fresh absolute output .zipzi path")?,
    );
    if !output.is_absolute() || output.exists() {
        return Err("output must be absolute and must not exist".into());
    }
    let session_base = allocation::LIVE.load(Ordering::Relaxed);
    let mut sdk = if let Some(path) = args.next() {
        let (result, imported, _) =
            zipzi_project::ProjectImporter::default().import_model3(&PathBuf::from(path));
        let imported = imported.ok_or(result.status)?;
        AuthoringSession::new(Snapshot::new(imported.document, imported.resources)?)
    } else {
        let sdk = fixture::build(128, 12, 1024)?;
        let low = evaluate(&sdk, -1.)?;
        let high = evaluate(&sdk, 1.)?;
        assert_eq!(
            high.drawables[0].positions[0].x - low.drawables[0].positions[0].x,
            2.
        );
        assert_eq!(sdk.snapshot().document().motion_order().len(), 1);
        sdk
    };
    let session_live_bytes = allocation::LIVE
        .load(Ordering::Relaxed)
        .saturating_sub(session_base);
    probe(&mut sdk, output, session_live_bytes)
}

fn evaluate(sdk: &AuthoringSession, value: f32) -> Result<zipzi_core::DrawableFrame> {
    let mut frame = zipzi_core::DrawableFrame::default();
    let status = zipzi_core::evaluation::evaluate_frame(
        sdk.snapshot().document(),
        &[(fixture::id(3), value)].into_iter().collect(),
        &mut frame,
    );
    if status.is_ok() {
        Ok(frame)
    } else {
        Err(status.into())
    }
}
