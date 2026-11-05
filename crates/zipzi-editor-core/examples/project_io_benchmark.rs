//! Fresh-process CPU import/open probe; see tools/benchmarks/README.md.
//! No window, GPU upload, or presentation is included in these timings.
use anyhow::{Result, bail, ensure};
use serde_json::json;
use std::{io::Write, path::Path, time::Instant};
use zipzi_editor_core::{Editor, LoadRequest, ViewportId, ViewportKind, ViewportMetrics};
use zipzi_project::DocumentSession;

fn measure<T>(phase: &str, operation: impl FnOnce() -> Result<T>) -> Result<T> {
    let start = Instant::now();
    let value = operation()?;
    let elapsed_ms = start.elapsed().as_secs_f64() * 1000.;
    println!("{}", json!({"phase": phase, "elapsed_ms": elapsed_ms}));
    std::io::stdout().flush()?;
    Ok(value)
}

fn project(mode: &str, input: &Path, output: Option<&Path>) -> Result<()> {
    let mut session = DocumentSession::new();
    let result = measure(mode, || {
        Ok(if mode == "project-import" {
            session.import_model3_authoring(input).0
        } else {
            session.open_authoring(input)
        })
    })?;
    ensure!(result.status.is_ok(), "{result:?}");
    // Imports can report unresolved display-info references independently of
    // image validity. Keep those notices and check image resources explicitly.
    ensure!(session.diagnose().is_empty(), "Invalid image resources");
    let document = session.document();
    println!(
        "{}",
        json!({
            "meshes": document.mesh_order().len(),
            "bindings": document.binding_order().len(),
            "assets": document.asset_order().len(),
            "diagnostics": result.diagnostics.iter().map(|d| json!({
                "asset_id": d.asset_id, "code": d.code, "message": d.message,
            })).collect::<Vec<_>>(),
        })
    );
    if let Some(output) = output {
        ensure!(!output.exists(), "Output already exists");
        let saved = measure("project-save", || Ok(session.save(output)))?;
        ensure!(
            saved.status.is_ok() && saved.resources_complete(),
            "{saved:?}"
        );
        // Correctness checks are outside the recorded save/import phases.
        let mut reopened = DocumentSession::new();
        let opened = reopened.open_authoring(output);
        ensure!(
            opened.status.is_ok() && opened.resources_complete(),
            "{opened:?}"
        );
        ensure!(
            session.document().same_content(reopened.document()),
            "Round trip differs"
        );
        println!("{}", json!({"roundtrip_equal": true}));
    }
    Ok(())
}

fn editor(mode: &str, input: &Path, output: Option<&Path>) -> Result<()> {
    let mut editor = Editor::empty();
    editor.register_primary_viewport(ViewportId::new(), ViewportKind::Modeling)?;
    let request = match mode {
        "editor-import" => LoadRequest::Model3(input.to_owned()),
        "editor-psd" => LoadRequest::Psd(input.to_owned()),
        _ => LoadRequest::Open(input.to_owned()),
    };
    let task = measure("editor-admit", || {
        editor
            .prepare_load(request)?
            .ok_or_else(|| anyhow::anyhow!("Already open"))
    })?;
    let completion = measure("editor-worker", || Ok(task.run()))?;
    let notices = measure("editor-publish", || editor.finish_load(completion))?;
    editor.set_canvas_metrics(ViewportMetrics {
        width: 1280.,
        height: 720.,
        scale_factor: 1.,
    });
    let preview = measure("editor-first-cpu-preview", || editor.preview())?;
    ensure!(
        !preview.scene.runtime_frame.drawables.is_empty(),
        "Empty preview"
    );
    println!(
        "{}",
        json!({
            "meshes": preview.scene.runtime_frame.drawables.len(),
            "textures": preview.scene.textures.len(),
            "rgba_bytes": preview.scene.textures.iter().map(|t| t.rgba.len()).sum::<usize>(),
            "notices": notices,
        })
    );
    if let Some(output) = output {
        ensure!(!output.exists(), "Output already exists");
        let id = editor.active_id().unwrap();
        let task = measure("editor-save-admit", || {
            editor.prepare_save(id, output, false)
        })?;
        let mut completion = measure("editor-save-worker", || Ok(task.run()))?;
        measure("editor-save-publish", || {
            editor.finish_save(&mut completion)
        })?;
    }
    Ok(())
}

fn main() -> Result<()> {
    let args: Vec<_> = std::env::args_os().collect();
    ensure!(
        (3..=4).contains(&args.len()),
        "Usage: project_io_benchmark MODE INPUT [NEW_OUTPUT]"
    );
    let mode = args[1].to_str().unwrap_or_default();
    let input = Path::new(&args[2]);
    let output = args.get(3).map(Path::new);
    ensure!(
        input.is_absolute() && output.is_none_or(Path::is_absolute),
        "Use absolute paths"
    );
    match mode {
        "project-import" | "project-open" => project(mode, input, output),
        "editor-import" | "editor-psd" | "editor-open" => editor(mode, input, output),
        _ => bail!("Unknown mode: {mode}"),
    }
}
