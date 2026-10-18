use std::collections::HashMap;

use crate::document::Document;
use crate::geometry::validate_positions;
use crate::keyforms::find_key_segment;
use crate::types::{
    Appearance, BlendShapeBinding, BlendShapeConstraint, RotationPose, Status, Vec2,
};

#[derive(Debug, Clone)]
pub(super) struct RuntimeRotationPose {
    pub(super) origin: Vec2,
    pub(super) angle: f32,
    pub(super) scale: f32,
    pub(super) reflect_x: bool,
    pub(super) reflect_y: bool,
}
impl From<RotationPose> for RuntimeRotationPose {
    fn from(p: RotationPose) -> Self {
        Self {
            origin: Vec2::new(p.origin.x as f32, p.origin.y as f32),
            angle: p.angle,
            scale: p.scale,
            reflect_x: p.reflect_x,
            reflect_y: p.reflect_y,
        }
    }
}
impl Default for RuntimeRotationPose {
    fn default() -> Self {
        RotationPose::default().into()
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct Selection {
    pub(super) indices: Vec<usize>,
    pub(super) weights: Vec<f32>,
    pub(super) enabled: bool,
}

#[cfg(test)]
pub(super) fn select<'a>(
    doc: &Document,
    values: &HashMap<String, f32>,
    binding: &[crate::types::BindingAxis],
    out: &'a mut Selection,
) -> &'a Selection {
    out.indices.clear();
    out.weights.clear();
    out.indices.push(0);
    out.weights.push(1.0);
    out.enabled = true;
    let mut stride = 1;
    for axis in binding {
        let p = doc.get_parameter(&axis.parameter_id).unwrap();
        let epsilon = 0.1f32.powi(p.decimal_places);
        let segment = find_key_segment(values[&p.id], &axis.keys, epsilon, epsilon * 1.5);
        out.enabled &= !segment.is_outside;
        let count = out.indices.len();
        for i in 0..count {
            out.indices[i] += segment.index as usize * stride;
            if segment.weight != 0.0 {
                out.indices.push(out.indices[i] + stride);
                out.weights.push(out.weights[i] * segment.weight);
                out.weights[i] *= 1.0 - segment.weight;
            }
        }
        stride *= axis.keys.len();
    }
    out
}

/// Sample an interned grid once per frame using numeric parameter slots.
/// Keep the Cartesian expansion order identical to the scalar reference.
pub(super) fn select_prepared(
    parameters: &[super::types::EvaluatedParameter],
    axes: &[super::prepared::SelectionAxis],
    out: &mut Selection,
) {
    out.indices.clear();
    out.weights.clear();
    out.indices.push(0);
    out.weights.push(1.0);
    out.enabled = true;
    let mut stride = 1;
    for axis in axes {
        let segment = find_key_segment(
            parameters[axis.parameter].value,
            &axis.keys,
            axis.epsilon,
            axis.epsilon * 1.5,
        );
        out.enabled &= !segment.is_outside;
        let count = out.indices.len();
        for i in 0..count {
            out.indices[i] += segment.index as usize * stride;
            if segment.weight != 0.0 {
                out.indices.push(out.indices[i] + stride);
                out.weights.push(out.weights[i] * segment.weight);
                out.weights[i] *= 1.0 - segment.weight;
            }
        }
        stride *= axis.keys.len();
    }
}

pub(super) fn blend_appearance<F>(s: &Selection, mut get: F) -> Appearance
where
    F: FnMut(usize) -> Appearance,
{
    let mut a = Appearance {
        opacity: 0.0,
        multiply: [0.0, 0.0, 0.0],
        screen: [0.0, 0.0, 0.0],
    };
    for k in 0..s.indices.len() {
        let f = get(s.indices[k]);
        let w = s.weights[k];
        a.opacity += f.opacity * w;
        for c in 0..3 {
            a.multiply[c] += f.multiply[c] * w;
            a.screen[c] += f.screen[c] * w;
        }
    }
    a
}

pub(super) fn inherit_appearance(child: &mut Appearance, parent: &Appearance) {
    child.opacity *= parent.opacity;
    for c in 0..3 {
        child.multiply[c] *= parent.multiply[c];
        child.screen[c] = child.screen[c] + parent.screen[c] - child.screen[c] * parent.screen[c];
    }
}

fn evaluate_constraint(c: &BlendShapeConstraint, val: f32) -> f32 {
    let n = c.keys.len();
    if n == 0 {
        return 1.0;
    }
    if n == 1 || val <= c.keys[0] {
        return c.weights[0];
    }
    if val >= c.keys[n - 1] {
        return c.weights[n - 1];
    }
    let mut idx = 0;
    while idx + 1 < n && val >= c.keys[idx + 1] {
        idx += 1;
    }
    let span = c.keys[idx + 1] - c.keys[idx];
    if span <= 0.0 {
        return c.weights[idx];
    }
    let t = (val - c.keys[idx]) / span;
    c.weights[idx] * (1.0 - t) + c.weights[idx + 1] * t
}

/// A one-dimensional BlendShape table selects at most two non-base keys.
#[derive(Debug, Clone, Default)]
pub(super) struct BlendSelection {
    entries: [(usize, f32); 2],
    len: usize,
}

impl BlendSelection {
    pub(super) fn as_slice(&self) -> &[(usize, f32)] {
        &self.entries[..self.len]
    }

    pub(super) fn sample(
        &mut self,
        input: &super::prepared::BlendSelectionInput,
        parameters: &[super::types::EvaluatedParameter],
    ) {
        self.len = 0;
        if input.keys.len() < 2 {
            return;
        }
        let value = parameters[input.parameter].value;
        let mut index = 0;
        let mut weight = 0.0;
        if value > input.keys[0] {
            while index + 1 < input.keys.len() && value >= input.keys[index + 1] {
                index += 1;
            }
            if index < input.keys.len() - 1 {
                let span = input.keys[index + 1] - input.keys[index];
                if span > 0.0 {
                    weight = (value - input.keys[index]) / span;
                }
            }
        }
        let mut constraint_weight = 1.0f32;
        for (slot, constraint) in &input.constraints {
            constraint_weight =
                constraint_weight.min(evaluate_constraint(constraint, parameters[*slot].value));
        }
        if constraint_weight == 0.0 {
            return;
        }
        let mut push = |index, weight: f32| {
            let weight = weight * constraint_weight;
            if weight != 0.0 {
                self.entries[self.len] = (index, weight);
                self.len += 1;
            }
        };
        if weight != 0.0 && index == input.base_key {
            push(index + 1, weight);
        } else if weight == 0.0 {
            if index != input.base_key {
                push(index, 1.0);
            }
        } else {
            push(index, 1.0 - weight);
            if index + 1 != input.base_key {
                push(index + 1, weight);
            }
        }
    }
}

pub fn evaluate_blend_binding(
    doc: &Document,
    values: &HashMap<String, f32>,
    b: &BlendShapeBinding,
) -> Vec<(usize, f32)> {
    let kt = match doc.get_blend_key_table(&b.key_table_id) {
        Some(kt) => kt,
        None => return Vec::new(),
    };
    let v = values.get(&kt.parameter_id).copied().unwrap_or(0.0);
    let key_count = kt.keys.len();
    if key_count < 2 {
        return Vec::new();
    }

    let mut index = 0usize;
    let mut weight = 0.0f32;
    if v > kt.keys[0] {
        while index + 1 < key_count && v >= kt.keys[index + 1] {
            index += 1;
        }
        if index < key_count - 1 {
            let span = kt.keys[index + 1] - kt.keys[index];
            if span > 0.0 {
                weight = (v - kt.keys[index]) / span;
            }
        }
    }

    let base_key_idx = kt.base_key_idx;
    let mut keyforms = Vec::with_capacity(2);
    if weight != 0.0 && index == base_key_idx {
        keyforms.push((index + 1, weight));
    } else if weight == 0.0 {
        if index != base_key_idx {
            keyforms.push((index, 1.0));
        }
    } else {
        if index + 1 == base_key_idx {
            keyforms.push((index, 1.0 - weight));
        } else {
            keyforms.push((index, 1.0 - weight));
            keyforms.push((index + 1, weight));
        }
    }

    let mut constraint_weight = 1.0f32;
    for c_id in &b.constraint_ids {
        if let Some(c) = doc.get_blend_constraint(c_id) {
            let cv = values.get(&c.parameter_id).copied().unwrap_or(0.0);
            let cw = evaluate_constraint(c, cv);
            constraint_weight = constraint_weight.min(cw);
        }
    }

    if constraint_weight == 0.0 {
        return Vec::new();
    }

    keyforms
        .into_iter()
        .map(|(idx, w)| (idx, w * constraint_weight))
        .filter(|(_, w)| *w != 0.0)
        .collect()
}

pub(super) fn default_selection() -> &'static Selection {
    static SELECTION: std::sync::OnceLock<Selection> = std::sync::OnceLock::new();
    SELECTION.get_or_init(|| Selection {
        indices: vec![0],
        weights: vec![1.0],
        enabled: true,
    })
}

pub(super) fn blend_positions<'a, F>(
    doc: &Document,
    parent: &str,
    s: &Selection,
    mut get: F,
    out: &mut Vec<Vec2>,
) -> Result<(), Status>
where
    F: FnMut(usize) -> &'a [Vec2],
{
    if !parent.is_empty() && doc.get_transform(parent).is_none() {
        return Err(Status::error("MISSING_TRANSFORM", parent));
    }
    let size = s.indices.first().map(|&i| get(i).len()).unwrap_or(0);
    if size > (i32::MAX as usize) / 2 {
        return Err(Status::error("CAPACITY", "positions"));
    }
    out.clear();
    out.resize(size, Vec2::default());
    for (&index, &weight) in s.indices.iter().zip(&s.weights) {
        let raw = get(index);
        if raw.len() != size {
            return Err(Status::error("INVALID_LENGTH", "keyform positions"));
        }
        let mut finite = true;
        for (target, p) in out.iter_mut().zip(raw) {
            finite &= p.x.is_finite() & p.y.is_finite();
            if weight != 0. {
                target.x += p.x * weight;
                target.y += p.y * weight;
            }
        }
        if !finite {
            return Err(Status::error(
                "NON_FINITE",
                "Position conversion overflows float32",
            ));
        }
    }
    let status = validate_positions(out);
    if !status.is_ok() {
        return Err(status);
    }
    Ok(())
}

#[cfg(test)]
mod selection_tests {
    use super::*;
    use crate::keyforms::{key_combinations, KeyAxis};
    use crate::{BindingAxis, Parameter};

    fn reference_blend_positions<'a, F>(
        doc: &Document,
        parent: &str,
        s: &Selection,
        mut get: F,
        out: &mut Vec<Vec2>,
    ) -> Result<(), Status>
    where
        F: FnMut(usize) -> &'a [Vec2],
    {
        if !parent.is_empty() && doc.get_transform(parent).is_none() {
            return Err(Status::error("MISSING_TRANSFORM", parent));
        }
        let size = s.indices.first().map(|&i| get(i).len()).unwrap_or(0);
        if size > (i32::MAX as usize) / 2 {
            return Err(Status::error("CAPACITY", "positions"));
        }
        out.clear();
        out.resize(size, Vec2::default());
        for (&index, &weight) in s.indices.iter().zip(&s.weights) {
            let raw = get(index);
            if raw.len() != size {
                return Err(Status::error("INVALID_LENGTH", "keyform positions"));
            }
            for (target, p) in out.iter_mut().zip(raw) {
                let q = *p;
                if !q.x.is_finite() || !q.y.is_finite() {
                    return Err(Status::error(
                        "NON_FINITE",
                        "Position conversion overflows float32",
                    ));
                }
                if weight != 0.0 {
                    target.x += q.x * weight;
                    target.y += q.y * weight;
                }
            }
        }
        let status = validate_positions(out);
        if !status.is_ok() {
            return Err(status);
        }
        Ok(())
    }

    #[test]
    fn position_blending_matches_scalar_bits_and_errors() {
        let id = |n| format!("{n:08x}-1111-4111-8111-111111111111");
        let mut seed = 123456789u32;
        let mut rows = vec![Vec::new(); 3];
        for row in &mut rows {
            for _ in 0..257 {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let x = (seed as i32) as f32 * 0.00001;
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                row.push(Vec2::new(x, (seed as i32) as f32 * 0.00001));
            }
        }
        for ppu in [1.0, 100.0, f32::MIN_POSITIVE] {
            let mut doc = Document::new();
            assert!(doc
                .initialize(
                    id(1),
                    crate::Canvas::new(640., 480., Vec2::new(320., 240.), ppu)
                )
                .is_ok());
            assert!(doc
                .create_transform(crate::Transform {
                    id: id(2),
                    ..Default::default()
                })
                .status
                .is_ok());
            for parent in [String::new(), id(2)] {
                for weights in [
                    vec![0.125, 0.375, 0.5],
                    vec![1., 0., -0.],
                    vec![0., 0., 0.],
                    vec![1., 1., 1.],
                ] {
                    let selection = Selection {
                        indices: vec![0, 1, 2],
                        weights,
                        enabled: true,
                    };
                    for exceptional in [
                        None,
                        Some(f32::MAX),
                        Some(f32::INFINITY),
                        Some(f32::NAN),
                        Some(-0.0),
                    ] {
                        let mut data = rows.clone();
                        if let Some(value) = exceptional {
                            data[1][128].x = value;
                        }
                        let mut expected = Vec::new();
                        let mut actual = Vec::new();
                        let reference = reference_blend_positions(
                            &doc,
                            &parent,
                            &selection,
                            |i| &data[i],
                            &mut expected,
                        );
                        let result =
                            blend_positions(&doc, &parent, &selection, |i| &data[i], &mut actual);
                        assert_eq!(result, reference);
                        if result.is_ok() {
                            let bits = |positions: &[Vec2]| {
                                positions
                                    .iter()
                                    .map(|p| [p.x.to_bits(), p.y.to_bits()])
                                    .collect::<Vec<_>>()
                            };
                            assert_eq!(
                                bits(&actual),
                                bits(&expected),
                                "parent={parent}, ppu={ppu}, weights={:?}",
                                selection.weights
                            );
                        }
                    }
                    let mut truncated = rows.clone();
                    truncated[1].pop();
                    let mut expected = Vec::new();
                    let mut actual = Vec::new();
                    assert_eq!(
                        blend_positions(&doc, &parent, &selection, |i| &truncated[i], &mut actual),
                        reference_blend_positions(
                            &doc,
                            &parent,
                            &selection,
                            |i| &truncated[i],
                            &mut expected
                        )
                    );
                }
            }
        }
    }

    #[test]
    fn compiled_blend_selection_matches_reference_at_keys_and_constraints() {
        use crate::{BlendShapeKeyTable, BlendShapeTargetKind, DeltaKeyforms, ParameterKind};
        let id = |n| format!("{n:08x}-1111-4111-8111-111111111111");
        let mut doc = Document::new();
        assert!(doc
            .initialize(id(1), crate::Canvas::new(100., 100., Vec2::default(), 1.))
            .is_ok());
        for (n, kind) in [(2, ParameterKind::BlendShape), (3, ParameterKind::Normal)] {
            assert!(doc
                .create_parameter(Parameter {
                    id: id(n),
                    kind,
                    minimum: -10.,
                    maximum: 10.,
                    ..Default::default()
                })
                .status
                .is_ok());
        }
        let constraint = BlendShapeConstraint {
            id: id(5),
            parameter_id: id(3),
            keys: vec![-1., 0., 1.],
            weights: vec![0., 0.75, 1.],
        };
        assert!(doc
            .create_blend_constraint(constraint.clone())
            .status
            .is_ok());
        let mut table = BlendShapeKeyTable {
            id: id(4),
            parameter_id: id(2),
            keys: vec![0.],
            base_key_idx: 0,
        };
        assert!(doc.create_blend_key_table(table.clone()).status.is_ok());
        let binding = BlendShapeBinding {
            id: id(6),
            target_id: id(7),
            target_kind: BlendShapeTargetKind::Mesh,
            key_table_id: id(4),
            constraint_ids: vec![id(5)],
            keyforms: DeltaKeyforms::Mesh(vec![]),
        };
        let mut selection = BlendSelection::default();
        for count in 1..=5 {
            table.keys = (0..count).map(|i| i as f32 - 2.).collect();
            for base in 0..count {
                table.base_key_idx = base;
                assert!(doc.replace_blend_key_table(table.clone()).status.is_ok());
                let input = super::super::prepared::BlendSelectionInput {
                    parameter: 0,
                    keys: table.keys.clone(),
                    base_key: base,
                    constraints: vec![(1, constraint.clone())],
                };
                for value in [-3., -2., -1.99999, -1.5, -1., -0.1, 0., 0.75, 1., 2., 3.] {
                    for constraint_value in [-2., -1., -0.5, 0., 0.5, 1., 2.] {
                        selection.sample(
                            &input,
                            &[
                                super::super::types::EvaluatedParameter {
                                    value,
                                    ..Default::default()
                                },
                                super::super::types::EvaluatedParameter {
                                    value: constraint_value,
                                    ..Default::default()
                                },
                            ],
                        );
                        let values = [(id(2), value), (id(3), constraint_value)].into();
                        assert_eq!(selection.as_slice(), evaluate_blend_binding(&doc, &values, &binding),
                            "count={count}, base={base}, value={value}, constraint={constraint_value}");
                    }
                }
            }
        }
    }

    #[test]
    fn sparse_selection_matches_cartesian_reference() {
        let mut doc = Document::new();
        doc.initialize(
            "00000000-0000-4000-8000-000000000001",
            crate::types::Canvas::new(100., 100., Vec2::default(), 1.),
        );
        let id = "00000000-0000-4000-8000-000000000002";
        assert!(doc
            .create_parameter(Parameter {
                id: id.into(),
                ..Default::default()
            })
            .status
            .is_ok());
        let binding = vec![
            BindingAxis {
                parameter_id: id.into(),
                keys: vec![-1., 0., 1.]
            };
            5
        ];
        let mut scratch = Selection::default();
        for value in [-2., -1., -0.8, -0.5, 0., 0.3, 1., 2.] {
            let values = [(id.to_owned(), value)].into();
            let result = select(&doc, &values, &binding, &mut scratch);
            let segment = find_key_segment(value, &binding[0].keys, 0.000001, 0.0000015);
            let axes = vec![
                KeyAxis {
                    index: segment.index,
                    key_count: 3,
                    weight: segment.weight
                };
                5
            ];
            let mut indices = vec![0; 32];
            let mut weights = vec![0.; 32];
            let count = key_combinations(&axes, &mut indices, &mut weights);
            assert_eq!(
                result.indices,
                indices[..count]
                    .iter()
                    .map(|&i| i as usize)
                    .collect::<Vec<_>>()
            );
            assert_eq!(result.weights, weights[..count]);
            assert_eq!(result.enabled, !segment.is_outside);
            let axes: Vec<_> = binding
                .iter()
                .map(|axis| super::super::prepared::SelectionAxis {
                    parameter: 0,
                    keys: axis.keys.clone(),
                    epsilon: 0.1f32.powi(doc.get_parameter(id).unwrap().decimal_places),
                })
                .collect();
            let mut compiled = Selection::default();
            select_prepared(
                &[super::super::types::EvaluatedParameter {
                    value,
                    ..Default::default()
                }],
                &axes,
                &mut compiled,
            );
            assert_eq!(compiled.indices, result.indices);
            assert_eq!(compiled.weights, result.weights);
            assert_eq!(compiled.enabled, result.enabled);
        }
        let binding = vec![
            BindingAxis {
                parameter_id: id.into(),
                keys: vec![0.]
            };
            80
        ];
        let result = select(&doc, &[(id.into(), 0.)].into(), &binding, &mut scratch);
        assert_eq!(result.indices, [0]);
        assert_eq!(result.weights, [1.]);
    }
}
