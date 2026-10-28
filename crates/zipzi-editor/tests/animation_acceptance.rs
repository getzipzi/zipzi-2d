//! Native acceptance uses the same library as the application.
#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    zipzi_editor::acceptance::animation(
        std::env::args()
            .skip(1)
            .filter(|arg| !arg.starts_with('-'))
            .map(Into::into)
            .collect(),
    )
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("SKIP: this acceptance target requires the macOS native capture backend");
}
