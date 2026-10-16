//! Licensed source assets stay untouched. Stress variants exist only in a temporary directory.
use super::harness::{Harness, command, update};
use anyhow::ensure;
use editor_core::{CommandId, Document, Editor};
use serde_json::json;
use std::path::PathBuf;

mod common;
mod edges;
mod navigation_and_dock;
pub(super) mod number_probe;
mod parity_and_persistence;
mod performance;
mod variants;

pub(crate) use common::*;

pub(super) fn run() -> anyhow::Result<()> {
    ensure!(
        std::env::var_os("KASANE_DEV_SESSION_DIR").is_none(),
        "unset KASANE_DEV_SESSION_DIR"
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sources = super::assets::available(
        std::env::var_os("KASANE_TIMELINE_MODEL").map(|path| vec![PathBuf::from(path)]),
        &[root.join("models/local/mao/runtime/mao_pro.model3.json")],
        std::env::var_os("KASANE_TIMELINE_REQUIRE_ASSET").is_some(),
    )?;
    let Some(source) = sources.first() else {
        println!("SKIP Timeline real-asset acceptance: no local model found");
        return Ok(());
    };
    let samples = std::env::var("KASANE_TIMELINE_SAMPLES")
        .ok()
        .map(|s| s.parse::<usize>())
        .transpose()?
        .unwrap_or(60);
    ensure!((2..=3600).contains(&samples), "invalid sample count");
    let output = std::env::var_os("KASANE_TIMELINE_OUTPUT")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/timeline-acceptance"));
    std::fs::create_dir_all(&output)?;
    if std::env::var_os("KASANE_TIMELINE_NUMBER_PROBE").is_some() {
        crate::benchmark::init()?;
    }
    let source_probe = std::env::var("KASANE_TIMELINE_NUMBER_PROBE").is_ok_and(|v| v == "source");
    let (_assets, path, source_info) = if source_probe {
        (None, source.canonicalize()?, json!({"model":source}))
    } else {
        let (assets, path, info) = variants::variants(&source.canonicalize()?)?;
        (Some(assets), path, info)
    };
    eprintln!("Timeline acceptance: importing real curves and dense variants");
    let (document, notices) = Document::import_model3(&path)?;
    let mut h = Harness::new()?;
    update(&mut h, |view, window, cx| {
        view.editor = Editor::from_document(document);
        view.animation_clock_running = true;
        view.refresh_document(window, cx);
    })?;
    command(&mut h, CommandId::AnimationWorkspace)?;
    command(&mut h, CommandId::FitView)?;
    drain(&mut h)?;
    if std::env::var_os("KASANE_TIMELINE_NUMBER_PROBE").is_some() {
        let report = number_probe::run(&mut h, samples)?;
        std::fs::write(
            output.join("number-probe.json"),
            serde_json::to_vec_pretty(&report)?,
        )?;
        println!("Timeline number probe: {}", output.display());
        crate::benchmark::finish();
        return Ok(());
    }
    eprintln!("Timeline acceptance: imported edge cases");
    let edges = edges::edges(&mut h)?;
    eprintln!("Timeline acceptance: real motion input, cancellation and playback");
    let real_performance = performance::performance(&mut h, &output, samples, "Natural")?;
    eprintln!("Timeline acceptance: dense input, cancellation and playback");
    let performance = performance::performance(&mut h, &output, samples, "Dense")?;
    eprintln!("Timeline acceptance: paused animation canvas navigation");
    let navigation = navigation_and_dock::canvas_navigation(&mut h, samples)?;
    eprintln!("Timeline acceptance: native right dock resize across workspaces");
    let dock_resize = navigation_and_dock::canvas_dock_resize(&mut h, samples, &output)?;
    eprintln!("Timeline acceptance: SDK result parity and project round trip");
    parity_and_persistence::result_parity(&mut h)?;
    parity_and_persistence::persistence(&mut h)?;
    let report = json!({"profile":if cfg!(debug_assertions){"debug"}else{"release"},"logical_window":[1100,740],"measurement":"injected GPUI input through current SDK/Metal canvas acceptance and GPUI draw; excludes OS input delivery, screenshot readback and compositor presentation","source":source_info,"import_notices":notices,"edge_cases":edges,"real_motion_performance":real_performance,"performance":performance,"canvas_navigation":navigation,"dock_resize":dock_resize,"save_reopen_unchanged":true,"result_sdk_snapshot_and_geometry_parity":true});
    std::fs::write(
        output.join("results.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!(
        "Timeline real-asset acceptance passed: {}",
        output.display()
    );
    Ok(())
}
