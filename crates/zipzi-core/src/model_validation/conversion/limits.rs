use super::*;
/// Bounds the conversion result and temporary working data independently of history.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionLimits {
    pub max_sample_combinations: usize,
    pub max_workspace_bytes: usize,
}
impl Default for ConversionLimits {
    fn default() -> Self {
        Self {
            max_sample_combinations: 65_536,
            max_workspace_bytes: 64 * 1024 * 1024,
        }
    }
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionWork {
    pub sample_combinations: usize,
    pub payload_bytes: usize,
    pub workspace_bytes: usize,
}
impl ConversionLimits {
    pub fn check_grid(
        self,
        dimensions: &[usize],
        bytes_per_form: usize,
    ) -> Result<ConversionWork, Status> {
        let combinations = dimensions.iter().try_fold(1usize, |n, &d| {
            if d == 0 {
                Err(Status::error("INVALID_KEYS", "Empty grid dimension"))
            } else {
                n.checked_mul(d).ok_or_else(overflow)
            }
        })?;
        let payload = combinations
            .checked_mul(bytes_per_form)
            .ok_or_else(overflow)?;
        self.check_conversion(combinations, payload)
    }
    pub fn check_conversion(
        self,
        combinations: usize,
        payload: usize,
    ) -> Result<ConversionWork, Status> {
        // Output plus interpolation/reconstruction working storage. Unchanged model
        // records and history are shared and are not part of this operation's budget.
        let workspace = payload.checked_mul(2).ok_or_else(overflow)?;
        if combinations > self.max_sample_combinations {
            return Err(Status::error(
                "AUTHORING_GRID_LIMIT",
                "Sample combinations exceed conversion limit",
            ));
        }
        if workspace > self.max_workspace_bytes {
            return Err(Status::error(
                "AUTHORING_MEMORY_LIMIT",
                "Conversion workspace exceeds limit",
            ));
        }
        Ok(ConversionWork {
            sample_combinations: combinations,
            payload_bytes: payload,
            workspace_bytes: workspace,
        })
    }
}
fn overflow() -> Status {
    Status::error("AUTHORING_CAPACITY_OVERFLOW", "Conversion size overflows")
}
