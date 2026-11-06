#[cfg(feature = "binary-prototype")]
mod binary_prototype;
mod decode;
mod encode;
mod types;
mod validation;

#[cfg(feature = "binary-prototype")]
pub use binary_prototype::{decode_project_cbor, encode_project_cbor};
pub(crate) use decode::decode_archive_project;
pub use decode::decode_project;
pub(crate) use encode::encode_native_project;
pub use encode::encode_project;
pub use validation::validate_json_syntax;

mod state_decode;
mod view;
pub(crate) use encode::encode_state;
pub(crate) use state_decode::{decode_state, DecodedState};
