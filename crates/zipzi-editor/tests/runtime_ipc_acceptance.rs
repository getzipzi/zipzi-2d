#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    zipzi_editor::acceptance::runtime_ipc()
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("SKIP: this acceptance target requires the macOS native capture backend");
}
