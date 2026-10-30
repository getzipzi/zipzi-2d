//! Shared authoring contract metadata and value types.
extern crate self as zipzi_contract;

pub mod animation;
pub mod api_shape;
pub mod authoring;
pub mod binary_wire;
pub mod changes;
pub mod document;
pub mod evaluation;
pub mod model;
pub mod operations;
pub mod physics;
pub mod physics_document;
pub mod project;
pub mod protocol;
pub mod runtime;
pub mod scene_reference;
pub mod serde_maps;
mod shared_vec;
pub mod spatial;
pub use shared_vec::SharedVec;

pub mod warp_bezier;
