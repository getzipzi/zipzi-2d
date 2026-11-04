//! Measure native authoring open/save, with validation and durable publication.
//! Usage: cargo run --release -p zipzi-project --example project_io_stress -- INPUT.zipzi OUTPUT_DIR [REPEATS]

use std::{fs, hint::black_box, path::Path, time::Instant};
use zipzi_project::{decode_project, encode_project, DocumentSession};

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
    let mut session = DocumentSession::new();
    let mut open = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        open.push(measure(|| {
            let result = session.open_authoring(input);
            assert!(
                result.status.is_ok() && result.resources_complete(),
                "{result:?}"
            );
        }));
    }
    times.insert("open_ms".into(), serde_json::json!(open));
    let original = encode_project(session.document()).unwrap();
    // Codec-only timings deliberately remain separate from archive admission,
    // copying, PNG validation, hashing and publication costs above/below.
    let mut decode = Vec::with_capacity(repeats);
    let mut encode = Vec::with_capacity(repeats);
    for _ in 0..repeats {
        decode.push(measure(|| {
            black_box(decode_project(black_box(&original)).unwrap());
        }));
        encode.push(measure(|| {
            black_box(encode_project(black_box(session.document())).unwrap());
        }));
    }
    times.insert("decode_ms".into(), serde_json::json!(decode));
    times.insert("encode_ms".into(), serde_json::json!(encode));

    // Establish a private copy, then time overwrites and fresh Save As paths.
    // Never write the supplied input project.
    let saved_path = output.join("saved.zipzi");
    assert!(session.save(&saved_path).status.is_ok());
    let mut save = Vec::with_capacity(repeats);
    let mut save_as = Vec::with_capacity(repeats);
    for index in 0..repeats {
        save.push(measure(|| {
            let result = session.save(&saved_path);
            assert!(
                result.status.is_ok() && result.resources_complete(),
                "{result:?}"
            );
        }));
        let mut reopened = DocumentSession::new();
        assert!(reopened.open_authoring(&saved_path).status.is_ok());
        let destination = output.join(format!("save-as-{index}.zipzi"));
        save_as.push(measure(|| {
            let result = reopened.save(&destination);
            assert!(
                result.status.is_ok() && result.resources_complete(),
                "{result:?}"
            );
        }));
        let mut verified = DocumentSession::new();
        assert!(verified.open_authoring(&destination).resources_complete());
        assert!(session.document().same_content(verified.document()));
        for id in session.document().asset_order() {
            let Some(content) = &session.document().get_asset(id).unwrap().content else {
                continue;
            };
            assert_eq!(
                fs::read(session.asset_root().join(&content.source)).unwrap(),
                fs::read(verified.asset_root().join(&content.source)).unwrap()
            );
        }
    }
    times.insert("save_ms".into(), serde_json::json!(save));
    times.insert("save_as_ms".into(), serde_json::json!(save_as));
    let doc = session.document();
    let report = serde_json::json!({
        "input_archive_bytes": fs::metadata(input).unwrap().len(),
        "saved_archive_bytes": fs::metadata(&saved_path).unwrap().len(),
        "document_bytes": original.len(),
        "meshes": doc.mesh_order().len(), "bindings": doc.binding_order().len(),
        "assets": doc.asset_order().len(), "motions": doc.motion_order().len(),
        "repeats": repeats, "times": times, "roundtrip_equal": true,
        "resource_bytes_equal": true
    });
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}
