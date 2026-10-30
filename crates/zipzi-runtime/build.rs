//! Deterministic SDK source identity, also available outside a Git checkout.
mod source_identity;
use std::{
    fs,
    path::{Path, PathBuf},
};
fn sources(path: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(path).expect("read SDK source tree") {
        let path = entry.expect("source entry").path();
        if path.is_dir() {
            sources(&path, files);
        } else if path
            .extension()
            .is_some_and(|ext| ext == "rs" || ext == "metal" || ext == "wgsl")
        {
            files.push(path);
        }
    }
}
fn main() {
    let manifest = PathBuf::from(std::env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.parent().unwrap().parent().unwrap();
    let mut files = vec![
        root.join("Cargo.toml"),
        root.join("Cargo.lock"),
        manifest.join("build.rs"),
        manifest.join("source_identity.rs"),
    ];
    for name in [
        "zipzi-runtime",
        "zipzi-observe",
        "zipzi-observe-backend",
        "zipzi-render",
        "zipzi-render-metal",
        "zipzi-render-wgpu",
        "zipzi-sdk",
        "zipzi-api-derive",
        "zipzi-contract",
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
        let shaders = path.join("shaders");
        if shaders.exists() {
            println!("cargo:rerun-if-changed={}", shaders.display());
            sources(&shaders, &mut files);
        }
    }
    for path in &files {
        println!("cargo:rerun-if-changed={}", path.display());
    }
    let digest = source_identity::digest(root, files);
    println!(
        "cargo:rustc-env=ZIPZI_RUNTIME_SDK_BUILD={}+{digest}",
        std::env::var("CARGO_PKG_VERSION").unwrap()
    );
}
