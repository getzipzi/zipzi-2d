/// Shared scalar parameter-range validation for native edits and host drafts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParameterRangeError {
    MinimumInvalid,
    MaximumInvalid,
    DefaultInvalid,
    RangeInvalid,
    RangeOverflow,
    DefaultOutOfRange,
}
impl std::fmt::Display for ParameterRangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::MinimumInvalid => "Enter a finite minimum",
            Self::MaximumInvalid => "Enter a finite maximum",
            Self::DefaultInvalid => "Enter a finite default",
            Self::RangeInvalid => "Minimum must be less than maximum",
            Self::RangeOverflow => "Parameter range length must be finite",
            Self::DefaultOutOfRange => "Default must be within the parameter range",
        })
    }
}
impl std::error::Error for ParameterRangeError {}
pub fn validate_parameter_range(
    minimum: f32,
    maximum: f32,
    default: f32,
) -> Result<(), ParameterRangeError> {
    use ParameterRangeError as E;
    let error = if !minimum.is_finite() {
        Some(E::MinimumInvalid)
    } else if !maximum.is_finite() {
        Some(E::MaximumInvalid)
    } else if !default.is_finite() {
        Some(E::DefaultInvalid)
    } else if minimum >= maximum {
        Some(E::RangeInvalid)
    } else if !(maximum - minimum).is_finite() {
        Some(E::RangeOverflow)
    } else if !(minimum..=maximum).contains(&default) {
        Some(E::DefaultOutOfRange)
    } else {
        None
    };
    error.map_or(Ok(()), Err)
}
