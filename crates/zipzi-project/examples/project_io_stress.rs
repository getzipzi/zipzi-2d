//! Measure native authoring open/save, with validation and durable publication.
//! Usage: cargo run --release -p zipzi-project --example project_io_stress -- INPUT.zipzi OUTPUT_DIR [REPEATS]

use std::{fs, hint::black_box, path::Path, time::Instant};
use zipzi_project::{ProjectIo, ResourceStore};

fn measure(mut run: impl FnMut()) -> f64 {
    let start = Instant::now();
    run();
    start.elapsed().as_secs_f64() * 1000.0
}

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if !(3..=4).contains(&args.len()) {
        eprintln!("Usage: project_io_stress INPUT.zipzi OUTPUT_DIR [REPEATS]");
        std::process::exit(2);
    }
    let input = Path::new(&args[1]);
    let output = Path::new(&args[2]);
    let repeats: usize = args
        .get(3)
        .map(|a| a.to_string_lossy().parse().unwrap())
        .unwrap_or(5);
    assert!(repeats > 0);
    assert!(
        input.is_absolute() && output.is_absolute(),
        "use absolute input/output paths"
    );
    fs::create_dir(output).expect("choose a fresh output directory");
    let mut times = serde_json::Map::new();
    let io = ProjectIo::new(output, ResourceStore::default());
    let mut loaded = None;
    let mut open = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        open.push(measure(|| {
            let result = io.load(input, || true).unwrap();
            assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
            loaded = Some(black_box(result));
        }));
    }
    times.insert("open_ms".into(), serde_json::json!(open));
    let loaded = loaded.unwrap();
    let saved_path = output.join("saved.zipzi");
    let mut receipt = io
        .save(
            &loaded.document,
            &loaded.resources,
            &saved_path,
            None,
            || true,
        )
        .unwrap();
    let original = fs::read(&saved_path).unwrap();
    let mut save = Vec::with_capacity(repeats);
    let mut save_as = Vec::with_capacity(repeats);
    for index in 0..repeats {
        save.push(measure(|| {
            receipt = io
                .save(
                    &loaded.document,
                    &loaded.resources,
                    &saved_path,
                    Some(&receipt.fingerprint),
                    || true,
                )
                .unwrap();
        }));
        let reopened = io.load(&saved_path, || true).unwrap();
        let destination = output.join(format!("save-as-{index}.zipzi"));
        save_as.push(measure(|| {
            io.save(
                &reopened.document,
                &reopened.resources,
                &destination,
                None,
                || true,
            )
            .unwrap();
        }));
        assert_eq!(fs::read(&destination).unwrap(), original);
        let verified = io.load(&destination, || true).unwrap();
        assert!(verified.diagnostics.is_empty());
        for id in loaded.document.asset_order() {
            let asset = loaded.document.asset(id).unwrap();
            if !matches!(asset.content, zipzi_core::state::AssetContent::Image { .. }) {
                continue;
            }
            assert_eq!(
                loaded.resources.read_asset(asset).unwrap().rgba,
                verified
                    .resources
                    .read_asset(verified.document.asset(id).unwrap())
                    .unwrap()
                    .rgba
            );
        }
    }
    times.insert("save_ms".into(), serde_json::json!(save));
    times.insert("save_as_ms".into(), serde_json::json!(save_as));
    let doc = &loaded.document;
    let report = serde_json::json!({
        "input_archive_bytes": fs::metadata(input).unwrap().len(),
        "saved_archive_bytes": fs::metadata(&saved_path).unwrap().len(),
        "document_bytes": receipt.document_bytes,
        "meshes": doc.mesh_order().len(), "bindings": doc.binding_order().len(),
        "assets": doc.asset_order().len(), "motions": doc.motion_order().len(),
        "repeats": repeats, "times": times, "roundtrip_equal": true,
        "resource_bytes_equal": true
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
