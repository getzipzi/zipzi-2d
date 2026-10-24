//! Deterministic SDK source identity, also available outside a Git checkout.
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn sources(path: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(path).expect("read SDK source tree") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            sources(&path, files);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            files.push(path);
        }
    }
}
fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.parent().unwrap().parent().unwrap();
    let mut files = vec![root.join("Cargo.toml"), root.join("Cargo.lock")];
    for name in [
        "zipzi-sdk",
        "zipzi-core",
        "zipzi-project",
        "zipzi-animation",
        "zipzi-live2d",
        "zipzi-moc3",
        "zipzi-moc3-psd",
        "zipzi-psd",
        "zipzi-cmo3",
    ] {
        let path = root.join("crates").join(name);
        println!("cargo:rerun-if-changed={}", path.join("src").display());
        files.push(path.join("Cargo.toml"));
        sources(&path.join("src"), &mut files);
    }
    files.sort();
    let mut hash = Sha256::new();
    for path in files {
        println!("cargo:rerun-if-changed={}", path.display());
        let relative = path.strip_prefix(root).unwrap().to_string_lossy();
        let data = fs::read(&path).expect("read build identity input");
        hash.update(relative.as_bytes());
        hash.update([0]);
        hash.update((data.len() as u64).to_be_bytes());
        hash.update(data);
    }
    let digest: String = hash
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    println!(
        "cargo:rustc-env=ZIPZI_RUNTIME_SDK_BUILD={}+{digest}",
        std::env::var("CARGO_PKG_VERSION").unwrap()
    );
}
