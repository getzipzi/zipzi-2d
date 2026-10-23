//! Stored binding layouts and implicit key grid enumeration.
use super::ObjectRef;
use kasane_core::{BindingAxis, SharedVec};

#[derive(Clone, Debug)]
pub struct BindingInfo {
    pub reference: ObjectRef,
    pub target: ObjectRef,
    pub axes: SharedVec<BindingAxis>,
    /// Actual stored combinations, never a product of unrelated parameter summaries.
    pub points: SharedVec<Vec<f32>>,
    pub blend: bool,
}
impl BindingInfo {
    pub fn supports_parameter_pad(&self) -> bool {
        self.blend
            || self
                .axes
                .iter()
                .filter(|a| !a.parameter_id.is_empty())
                .count()
                <= 2
    }
    pub fn layout(&self) -> String {
        let active: Vec<_> = self
            .axes
            .iter()
            .filter(|a| !a.parameter_id.is_empty())
            .collect();
        let pending = self.axes.len() - active.len();
        format!(
            "{}D · {} = {}{}",
            active.len(),
            active
                .iter()
                .map(|a| a.samples.len().to_string())
                .collect::<Vec<_>>()
                .join(" × "),
            self.points.len(),
            if pending == 0 {
                String::new()
            } else {
                format!(" · {pending} pending")
            }
        )
    }
}
/// Complete SDK grids vary the first axis fastest. Only used for implicit Glue/Blend slots.
pub(super) fn grid_points(axes: &[BindingAxis]) -> Vec<Vec<f32>> {
    let axes: Vec<_> = axes.iter().filter(|a| !a.parameter_id.is_empty()).collect();
    if axes.is_empty() {
        return Vec::new();
    }
    let mut points = vec![Vec::new()];
    for axis in axes {
        points = axis
            .keys
            .iter()
            .flat_map(|&key| {
                points.iter().map(move |p| {
                    let mut p = p.clone();
                    p.push(key);
                    p
                })
            })
            .collect();
    }
    points
}
