//! Fixed-scene cold renderer probe. Loading is outside every recorded interval.
#[cfg(target_os = "macos")]
#[path = "macos/first_frame_benchmark.rs"]
mod native;

fn main() -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    return native::main();
    #[cfg(not(target_os = "macos"))]
    anyhow::bail!("This probe requires macOS and Metal")
}
