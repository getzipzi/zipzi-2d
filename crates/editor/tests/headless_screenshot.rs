//! AppKit text shaping must run on the main thread, so this target has no libtest harness.
//! Compile the same private UI modules as the application rather than a screenshot mock.
#![allow(dead_code, unused_imports)]

include!("../src/main.rs");

#[path = "../headless_tests/screenshots.rs"]
mod screenshots;

fn main() -> anyhow::Result<()> {
    #[cfg(target_os = "macos")]
    {
        objc2::rc::autoreleasepool(|_| screenshots::run())
    }
    #[cfg(not(target_os = "macos"))]
    anyhow::bail!("Editor screenshots require macOS with a Metal GPU")
}
