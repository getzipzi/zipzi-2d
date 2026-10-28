#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    if std::env::var_os("ZIPZI_RUNTIME_INTERACTION_OUTPUT").is_none() {
        println!(
            "SKIP opt-in Runtime interaction timing: set ZIPZI_RUNTIME_INTERACTION_OUTPUT and ZIPZI_TIMELINE_MODEL"
        );
        return Ok(());
    }
    zipzi_editor::acceptance::runtime_interaction()
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("SKIP: this acceptance target requires the macOS native capture backend");
}
