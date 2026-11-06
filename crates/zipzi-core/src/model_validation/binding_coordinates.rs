use crate::document::valid_uuid;
use crate::{BindingAxis, BindingSample, Status};

pub(crate) fn normalize_axes(axes: &mut [BindingAxis]) -> Result<(), Status> {
    if axes.len() > 16 {
        return Err(Status::error("INVALID_BINDING", "At most 16 axes"));
    }
    let mut ids = std::collections::HashSet::new();
    for a in axes {
        if a.id.is_empty() {
            a.id = uuid::Uuid::new_v4().to_string();
        }
        if !valid_uuid(&a.id) || !ids.insert(a.id.clone()) {
            return Err(Status::error("INVALID_AXIS_ID", &a.id));
        }
        if a.samples.is_empty() && !a.keys.is_empty() {
            a.samples = a
                .keys
                .iter()
                .map(|&value| BindingSample {
                    id: uuid::Uuid::new_v4().to_string(),
                    value,
                })
                .collect();
        }
        let values: Vec<_> = a.samples.iter().map(|s| s.value).collect();
        if !a.keys.is_empty() && a.keys != values {
            return Err(Status::error("STALE_SAMPLE_VALUES", &a.id));
        }
        for (i, s) in a.samples.iter().enumerate() {
            if i > 0 && s.value <= a.samples[i - 1].value {
                return Err(Status::error("INVALID_KEYS", &a.id));
            }
            if !valid_uuid(&s.id) || !ids.insert(s.id.clone()) {
                return Err(Status::error("INVALID_SAMPLE_ID", &s.id));
            }
            if !s.value.is_finite() {
                return Err(Status::error("INVALID_KEYS", &a.id));
            }
        }
        a.keys = values;
    }
    Ok(())
}
pub(crate) fn coordinates(
    axes: &[BindingAxis],
    keys: &mut Vec<f32>,
    samples: &mut Vec<String>,
) -> Result<(), Status> {
    let active: Vec<_> = axes.iter().filter(|a| !a.parameter_id.is_empty()).collect();
    if samples.is_empty() && !keys.is_empty() {
        if keys.len() != active.len() {
            return Err(Status::error(
                "INVALID_KEY_COMBINATION",
                "Coordinate dimension mismatch",
            ));
        }
        *samples = active
            .iter()
            .zip(keys.iter())
            .map(|(a, k)| {
                a.samples
                    .iter()
                    .find(|s| s.value == *k)
                    .map(|s| s.id.clone())
                    .ok_or_else(|| Status::error("INVALID_KEY_COMBINATION", &a.id))
            })
            .collect::<Result<_, _>>()?;
    }
    if samples.len() != active.len() {
        return Err(Status::error(
            "INVALID_KEY_COMBINATION",
            "Sample dimension mismatch",
        ));
    }
    *samples = active
        .iter()
        .map(|axis| {
            samples
                .iter()
                .find(|id| axis.samples.iter().any(|s| &s.id == *id))
                .cloned()
                .ok_or_else(|| Status::error("MISSING_SAMPLE", &axis.id))
        })
        .collect::<Result<_, _>>()?;
    let derived = active
        .iter()
        .zip(samples.iter())
        .map(|(a, id)| {
            a.samples
                .iter()
                .find(|s| &s.id == id)
                .map(|s| s.value)
                .ok_or_else(|| Status::error("MISSING_SAMPLE", id))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if !keys.is_empty() && *keys != derived {
        return Err(Status::error(
            "STALE_SAMPLE_VALUES",
            "Keyform coordinates changed",
        ));
    }
    *keys = derived;
    Ok(())
}
