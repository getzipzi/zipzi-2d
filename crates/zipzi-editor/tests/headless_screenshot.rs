//! Native acceptance uses the same library as the application.
#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    zipzi_editor::acceptance::screenshots()
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("SKIP: this acceptance target requires the macOS native capture backend");
}
