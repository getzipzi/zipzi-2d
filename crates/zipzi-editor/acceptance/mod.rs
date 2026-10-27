//! Native acceptance runs against the production library, including asynchronous persistence.
mod animation;
mod assets;
mod harness;
mod screenshots;
mod timeline;

use crate::test_fixture as fixture;

pub fn screenshots() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| screenshots::run())
}

pub fn animation(models: Vec<std::path::PathBuf>) -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| {
        crate::benchmark::init()?;
        let result = animation::run(models);
        crate::benchmark::finish();
        result
    })
}

pub fn timeline() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| timeline::run())
}

mod runtime_interaction;
pub fn runtime_interaction() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| runtime_interaction::run())
}

mod runtime_ipc;
pub fn runtime_ipc() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| runtime_ipc::run())
}

mod runtime_handoff;
pub fn runtime_handoff() -> anyhow::Result<()> {
    objc2::rc::autoreleasepool(|_| runtime_handoff::run())
}
