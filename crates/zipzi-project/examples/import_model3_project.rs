//! Import a real model3/MOC3 model into a self-contained editable project.
//! Usage: cargo run --release -p zipzi-project --example import_model3_project -- MODEL3 OUTPUT.zipzi

use std::fs;
use std::path::Path;
use std::time::Instant;
use zipzi_project::{ProjectImporter, ProjectIo, ResourceStore};

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        eprintln!("Usage: import_model3_project MODEL3 OUTPUT.zipzi");
        std::process::exit(2);
    }
    let source = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    assert!(source.is_absolute() && output.is_absolute());
    assert!(!output.exists(), "Output already exists");

    let resources = ResourceStore::default();
    let importer = ProjectImporter {
        resources: resources.clone(),
    };
    let io = ProjectIo::new(std::env::temp_dir(), resources);
    let start = Instant::now();
    let (imported, model, report) = importer.import_model3(source);
    assert!(imported.status.is_ok(), "{imported:?}");
    let import_diagnostics = imported.diagnostics.clone();
    assert!(imported.resources_complete(), "{imported:?}");
    let model = model.unwrap();
    let import_ms = start.elapsed().as_secs_f64() * 1000.0;

    let report = report.expect("successful import has a report");
    let meshes = model.document.mesh_order().len();
    let bindings = model.document.binding_order().len();
    let assets = model.document.asset_order().len();
    let start = Instant::now();
    let saved = io
        .save(&model.document, &model.resources, output, None, || true)
        .expect("save imported model");
    assert!(saved.published);
    let initial_save_ms = start.elapsed().as_secs_f64() * 1000.0;
    let start = Instant::now();
    let reopened = io.load(output, || true).expect("reopen imported model");
    assert!(reopened.diagnostics.is_empty());
    let first_open_ms = start.elapsed().as_secs_f64() * 1000.0;
    let before = fs::read(output).unwrap();
    io.save(
        &reopened.document,
        &reopened.resources,
        output,
        Some(&saved.fingerprint),
        || true,
    )
    .expect("save reopened model");
    let roundtrip_equal = before == fs::read(output).unwrap();
    let asset_bytes = saved.asset_bytes;
    println!(
        "{}",
        serde_json::to_string_pretty(&serde_json::json!({
            "source_model3": source.display().to_string(),
            "moc_version": report.moc_version,
            "meshes": meshes,
            "bindings": bindings,
            "assets": assets,
            "archive_bytes": fs::metadata(output).unwrap().len(),
            "asset_bytes": asset_bytes,
            "import_ms": import_ms,
            "initial_save_ms": initial_save_ms,
            "first_open_ms": first_open_ms,
            "roundtrip_equal": roundtrip_equal,
            "import_warnings": report.warnings.len(),
            "import_diagnostics": import_diagnostics.iter().map(|d| format!("{}: {}", d.code, d.message)).collect::<Vec<_>>(),
            "unimported_attachments": report.unimported_attachments.len()
        }))
        .unwrap()
    );
    if !roundtrip_equal {
        std::process::exit(1);
    }
}
