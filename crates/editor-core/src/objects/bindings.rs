//! Stored binding layouts and implicit key grid enumeration.
use super::ObjectRef;
use kasane_core::BindingAxis;

#[derive(Clone, Debug)]
pub struct BindingInfo {
    pub reference: ObjectRef,
    pub target: ObjectRef,
    pub axes: Vec<BindingAxis>,
    /// Actual stored combinations, never a product of unrelated parameter summaries.
    pub points: Vec<Vec<f32>>,
    pub blend: bool,
}
impl BindingInfo {
    pub fn writable(&self) -> bool {
        self.blend || self.axes.len() <= 2
    }
    pub fn layout(&self) -> String {
        format!(
            "{}D · {} = {}",
            self.axes.len(),
            self.axes
                .iter()
                .map(|a| a.keys.len().to_string())
                .collect::<Vec<_>>()
                .join(" × "),
            self.points.len()
        )
    }
}
/// Complete SDK grids vary the first axis fastest. Only used for implicit Glue/Blend slots.
pub(super) fn grid_points(axes: &[BindingAxis]) -> Vec<Vec<f32>> {
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
