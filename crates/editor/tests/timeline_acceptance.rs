//! Real imported curves, native input, and asynchronous SDK/Metal acceptance.
#![allow(dead_code, unused_imports)]
include!("../src/main.rs");
#[path = "../headless_tests/screenshots.rs"]
mod screenshots;
#[path = "../headless_tests/timeline_acceptance.rs"]
mod timeline_acceptance;
fn main() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| timeline_acceptance::run())
}
