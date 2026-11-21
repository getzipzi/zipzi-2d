const CURRENT_DOCUMENT_VERSION: u64 = 24;

mod decode;
mod encode;
mod types;
mod v22;
mod validation;

pub use validation::validate_json_syntax;

mod state_decode;
mod view;
pub(crate) use encode::encode_state;
pub(crate) use state_decode::{decode_state, DecodedState};
