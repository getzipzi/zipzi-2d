//! Rehydrate numeric caches omitted by native binding serialization. This does
//! not create IDs, reorder samples, validate edits or select conversion policies.
use super::*;
use zipzi_core::{BindingAxis, SceneTrack};

fn axes(axes: &mut [BindingAxis]) {
    for axis in axes {
        axis.keys = axis.samples.iter().map(|sample| sample.value).collect();
    }
}

fn coordinates(axes: &[BindingAxis], samples: &[String]) -> Result<Vec<f32>, ObservationError> {
    let active: Vec<_> = axes
        .iter()
        .filter(|axis| !axis.parameter_id.is_empty())
        .collect();
    if active.len() != samples.len() {
        return Err(failure(
            "BUNDLE_FORMAT",
            "Binding coordinate dimension mismatch",
        ));
    }
    active
        .iter()
        .zip(samples)
        .map(|(axis, id)| {
            axis.samples
                .iter()
                .find(|sample| sample.id == *id)
                .map(|sample| sample.value)
                .ok_or_else(|| {
                    failure(
                        "BUNDLE_FORMAT",
                        "Binding coordinate references a missing sample",
                    )
                })
        })
        .collect()
}

impl CapturedAuthoring {
    pub(super) fn restore_binding_coordinates(&mut self) -> Result<(), ObservationError> {
        for binding in &mut self.bindings {
            axes(&mut binding.axes);
            for form in &mut binding.keyforms {
                form.keys = coordinates(&binding.axes, &form.samples)?;
            }
        }
        for binding in &mut self.scene_bindings {
            axes(&mut binding.axes);
            match &mut binding.track {
                SceneTrack::Warp { keyforms, .. } => {
                    for form in keyforms {
                        form.keys = coordinates(&binding.axes, &form.samples)?;
                    }
                }
                SceneTrack::Rotation { keyforms, .. } => {
                    for form in keyforms {
                        form.keys = coordinates(&binding.axes, &form.samples)?;
                    }
                }
                SceneTrack::Part { keyforms, .. } => {
                    for form in keyforms {
                        form.keys = coordinates(&binding.axes, &form.samples)?;
                    }
                }
            }
        }
        for glue in &mut self.glues {
            if let Some(binding) = &mut glue.binding {
                axes(&mut binding.axes);
            }
        }
        for table in &mut self.blend_key_tables {
            table.keys = table.samples.iter().map(|sample| sample.value).collect();
            table.base_key_idx = if table.samples.is_empty() {
                0
            } else {
                table
                    .samples
                    .iter()
                    .position(|sample| sample.id == table.base_sample_id)
                    .ok_or_else(|| {
                        failure(
                            "BUNDLE_FORMAT",
                            "Blend table references a missing base sample",
                        )
                    })?
            };
        }
        for constraint in &mut self.blend_constraints {
            constraint.keys = constraint
                .samples
                .iter()
                .map(|sample| sample.value)
                .collect();
            constraint.weights = constraint
                .samples
                .iter()
                .map(|sample| sample.weight)
                .collect();
        }
        Ok(())
    }
}
