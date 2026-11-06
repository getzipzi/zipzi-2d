#![allow(dead_code)]

use zipzi_core::{
    state::{AssetContent, AssetRecord, DocumentBuilder, DocumentState, FrozenDocument},
    Status,
};

/// A model-only texture reference for CPU evaluation fixtures.
pub fn image(id: String, name: &str, width: u32, height: u32) -> AssetRecord {
    AssetRecord {
        id,
        name: name.into(),
        content: AssetContent::Image {
            resource: Default::default(),
            width,
            height,
        },
    }
}

pub fn prepare(
    state: &DocumentState,
    write: impl FnOnce(&mut DocumentBuilder) -> Result<(), Status>,
) -> Result<FrozenDocument, Status> {
    let mut builder = state.builder();
    write(&mut builder)?;
    builder.freeze()
}
