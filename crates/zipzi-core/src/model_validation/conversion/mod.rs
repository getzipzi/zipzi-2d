//! Read-only domain conversions; results own only changed payloads.
use super::binding_coordinates as identity;
use super::*;
use crate::*;
use serde::{Deserialize, Serialize};
mod limits;
pub use limits::{ConversionLimits, ConversionWork};
pub(crate) mod report;
pub use report::*;
mod control;
pub use control::ConversionControl;
mod deltas;
mod lattice;
pub(crate) mod reparent;
use crate::model_validation::organization::SceneWorkspace;
pub(crate) mod blend;
pub(crate) mod ordinary;
pub(crate) mod range;
fn require_ok(status: Status) -> Result<(), Status> {
    if status.is_ok() {
        Ok(())
    } else {
        Err(status)
    }
}
fn check_keys(parameter: &Parameter, keys: &[f32]) -> Result<(), Status> {
    if keys.is_empty()
        || keys.iter().enumerate().any(|(i, &k)| {
            !k.is_finite()
                || k < parameter.minimum
                || k > parameter.maximum
                || (i > 0 && k <= keys[i - 1])
        })
    {
        Err(Status::error("INVALID_KEYS", &parameter.id))
    } else {
        Ok(())
    }
}

/// Conversion diagnostics and operation-local work estimate.
#[derive(Debug, Clone)]
pub struct ConversionOutcome {
    pub report: ConversionReport,
    pub work: ConversionWork,
}
