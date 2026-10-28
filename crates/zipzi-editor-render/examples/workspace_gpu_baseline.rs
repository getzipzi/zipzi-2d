#[cfg(target_os = "macos")]
#[path = "macos/workspace_gpu_baseline.rs"]
mod native;
#[cfg(target_os = "macos")]
fn main() -> anyhow::Result<()> {
    native::main()
}
#[cfg(not(target_os = "macos"))]
fn main() -> anyhow::Result<()> {
    anyhow::bail!("This Metal benchmark requires macOS; use the wgpu_readback GPU test on Windows")
}
