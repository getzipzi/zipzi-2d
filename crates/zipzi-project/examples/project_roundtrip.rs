//! Save and reopen a native project through the value-only I/O boundary.
use std::{fs, path::Path};
use zipzi_project::{ProjectIo, ResourceStore};

fn main() {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        eprintln!("Usage: project_roundtrip INPUT_PROJECT OUTPUT_PROJECT");
        std::process::exit(2);
    }
    let io = ProjectIo::new(std::env::temp_dir(), ResourceStore::default());
    let loaded = io.load(Path::new(&args[1]), || true).expect("open input");
    assert!(loaded.diagnostics.is_empty(), "{:?}", loaded.diagnostics);
    let artifact = zipzi_moc3::encode_moc3(&loaded.document).expect("encode input");
    let output = Path::new(&args[2]);
    let receipt = io
        .save(&loaded.document, &loaded.resources, output, None, || true)
        .expect("save output");
    let reopened = io.load(output, || true).expect("reopen output");
    assert!(reopened.diagnostics.is_empty());
    assert_eq!(
        artifact.bytes,
        zipzi_moc3::encode_moc3(&reopened.document).unwrap().bytes
    );
    let saved_bytes = fs::read(output).unwrap();
    io.save(
        &reopened.document,
        &reopened.resources,
        output,
        Some(&receipt.fingerprint),
        || true,
    )
    .unwrap();
    assert_eq!(saved_bytes, fs::read(output).unwrap());
    println!("Project reopened with identical native archive and MOC3 bytes");
}
