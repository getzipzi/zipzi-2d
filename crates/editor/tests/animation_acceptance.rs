//! Real editor/Metal acceptance and opt-in, wall-clock 60 Hz playback profiling.
#![allow(dead_code, unused_imports)]
include!("../src/main.rs");
#[path = "../headless_tests/animation_acceptance.rs"]
mod animation_acceptance;
#[path = "../headless_tests/screenshots.rs"]
mod screenshots;
fn main() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| {
        benchmark::init()?;
        let result = animation_acceptance::run();
        benchmark::finish();
        result
    })
}
