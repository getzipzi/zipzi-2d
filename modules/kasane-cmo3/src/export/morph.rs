//! Cubism stores absolute morph forms relative to the ordinary grid at defaults;
//! Kasane stores additive deltas. Keep that conversion independent of packaging.
use super::*;

impl Exporter<'_> {
    pub(super) fn apply_morphs(&mut self) {
        let mut sets: HashMap<String, (Vec<N>, Vec<N>)> = HashMap::new();
        let mut drivers = BTreeSet::new();
        for id in self.doc.blend_binding_order() {
            let binding = self.doc.get_blend_binding(id).unwrap();
            let table = self.doc.get_blend_key_table(&binding.key_table_id).unwrap();
            if table.keys[table.base_key_idx] != 0. {
                continue;
            }
            let target = if binding.target_kind == BlendShapeTargetKind::Offscreen {
                self.doc
                    .get_offscreen(&binding.target_id)
                    .unwrap()
                    .part_id
                    .as_str()
            } else {
                &binding.target_id
            };
            if !drivers.insert((target.to_string(), table.parameter_id.clone())) {
                continue;
            }
            let owner = self.source(target);
            let source = self.graph.get(&owner).clone();
            let pool = source.field_value("keyforms").unwrap();
            let axes = match binding.target_kind {
                BlendShapeTargetKind::Mesh => {
                    self.doc.binding_for_mesh(target).map(|b| b.axes.as_slice())
                }
                BlendShapeTargetKind::Glue => self
                    .doc
                    .get_glue(target)
                    .unwrap()
                    .binding
                    .as_ref()
                    .map(|b| b.axes.as_slice()),
                _ => self
                    .doc
                    .binding_for_scene(target)
                    .map(|b| b.axes.as_slice()),
            }
            .unwrap_or(&[]);
            let count = axes.iter().fold(1, |n, a| n * a.keys.len());
            let forms: Vec<_> = pool.children[..count]
                .iter()
                .map(|r| self.graph.get(r).clone())
                .collect();
            let weights = grid_default_weights(self.doc, axes);
            let base = interpolate(&forms, &weights);
            let mut new_forms = Vec::new();
            let (records, constraints) = sets.entry(target.to_string()).or_default();
            for (slot, &key) in table.keys.iter().enumerate() {
                if slot == table.base_key_idx {
                    continue;
                }
                let mut form = base.clone();
                let g = self.new_guid("CFormGuid");
                *form.field_mut("guid").unwrap() = g.clone().named("guid");
                form.attrs.remove("xs.id");
                form.attrs.remove("xs.idx");
                form.attrs.remove("xs.n");
                match &binding.keyforms {
                    DeltaKeyforms::Mesh(values) => {
                        let delta = &values[slot];
                        add_positions(
                            &mut form,
                            &delta
                                .positions
                                .iter()
                                .map(|&p| {
                                    self.source_delta(
                                        &self.doc.get_mesh(target).unwrap().deformer_id,
                                        p,
                                    )
                                })
                                .collect::<Vec<_>>(),
                        );
                        add_appearance(&mut form, delta.opacity, delta.multiply, delta.screen);
                        if let Some(order) = delta.draw_order {
                            add_scalar(&mut form, "drawOrder", order);
                        }
                    }
                    DeltaKeyforms::Warp(values) => {
                        let delta = &values[slot];
                        add_positions(
                            &mut form,
                            &delta
                                .points
                                .iter()
                                .map(|&p| {
                                    self.source_delta(
                                        self.doc.get_transform(target).unwrap().parent(),
                                        p,
                                    )
                                })
                                .collect::<Vec<_>>(),
                        );
                        add_appearance(&mut form, delta.opacity, delta.multiply, delta.screen);
                    }
                    DeltaKeyforms::Rotation(values) => {
                        let delta = &values[slot];
                        if let Some(origin) = delta.origin {
                            add_attr(&mut form, "originX", origin.x);
                            add_attr(
                                &mut form,
                                "originY",
                                self.source_delta(
                                    self.doc.get_transform(target).unwrap().parent(),
                                    origin,
                                )
                                .y,
                            );
                        }
                        if let Some(angle) = delta.angle {
                            add_attr(&mut form, "angle", self.rotation_sign() * angle);
                        }
                        if let Some(scale) = delta.scale {
                            add_attr(&mut form, "scale", scale);
                        }
                        add_appearance(&mut form, delta.opacity, delta.multiply, delta.screen);
                    }
                    DeltaKeyforms::Part(values) => {
                        add_scalar(&mut form, "drawOrder", values[slot].draw_order)
                    }
                    DeltaKeyforms::Glue(values) => {
                        add_scalar(&mut form, "intensity", values[slot].intensity)
                    }
                    DeltaKeyforms::Offscreen(values) => {
                        let delta = &values[slot];
                        add_appearance(
                            &mut form,
                            Some(delta.opacity),
                            delta.multiply,
                            delta.screen,
                        );
                    }
                }
                new_forms.push(self.graph.add(form));
                records.push(
                    N::new("KeyFormMorphTarget")
                        .field("parameterGuid", guid("CParameterGuid", &table.parameter_id))
                        .field("keyValue", f(key))
                        .field("keyformGuid", g)
                        .field("owner", owner.clone())
                        .field(
                            "editBaseParameterMap",
                            N::new("MorphTargetEditBaseParameterMap")
                                .field("baseParameterMap", N::null()),
                        ),
                );
            }
            for id in &binding.constraint_ids {
                let c = self.doc.get_blend_constraint(id).unwrap();
                for (&key, &weight) in c.keys.iter().zip(&c.weights) {
                    constraints.push(
                        N::new("MorphTargetBlendWeightConstraint")
                            .field(
                                "morphTargetParameterGuid",
                                guid("CParameterGuid", &table.parameter_id),
                            )
                            .field(
                                "constraintParameterGuid",
                                guid("CParameterGuid", &c.parameter_id),
                            )
                            .field("constraintParameterValue", f(key))
                            .field("blendWeight", f(weight)),
                    );
                }
            }
            let mut source = self.graph.get(&owner).clone();
            let pool = source.field_mut("keyforms").unwrap();
            pool.children.extend(new_forms);
            pool.attrs
                .insert("count".into(), pool.children.len().to_string());
            self.graph.set(&owner, source);
        }
        for (id, (records, constraints)) in sets {
            let owner = self.source(&id);
            let mut source = self.graph.get(&owner).clone();
            *source.field_mut("keyformMorphTargetSet").unwrap() = N::new("KeyFormMorphTargetSet")
                .field("_morphTargets", N::list("carray_list", records))
                .field(
                    "blendWeightConstraintSet",
                    N::new("MorphTargetBlendWeightConstraintSet")
                        .field("_constraints", N::list("carray_list", constraints)),
                )
                .named("keyformMorphTargetSet");
            self.graph.set(&owner, source);
        }
    }
}

fn grid_default_weights(doc: &Document, axes: &[BindingAxis]) -> Vec<f32> {
    let mut weights = vec![1.];
    for axis in axes {
        let parameter = doc.get_parameter(&axis.parameter_id).unwrap();
        let epsilon = 0.1f32.powi(parameter.decimal_places);
        let segment = kasane_core::keyforms::find_key_segment(
            parameter.default_value,
            &axis.keys,
            epsilon,
            epsilon * 1.5,
        );
        let mut next = vec![0.; weights.len() * axis.keys.len()];
        for (slot, &weight) in weights.iter().enumerate() {
            next[slot + weights.len() * segment.index as usize] = weight * (1. - segment.weight);
            if segment.weight > 0. {
                next[slot + weights.len() * (segment.index as usize + 1)] = weight * segment.weight;
            }
        }
        weights = next;
    }
    weights
}
fn interpolate(forms: &[N], weights: &[f32]) -> N {
    let first = weights.iter().position(|&w| w > 0.).unwrap_or(0);
    let mut out = forms[first].clone();
    if matches!(out.tag.as_str(), "f" | "i") {
        let value: f32 = forms
            .iter()
            .zip(weights)
            .filter(|(_, w)| **w != 0.)
            .map(|(n, w)| n.text.parse::<f32>().unwrap() * w)
            .sum();
        out.text = if out.tag == "i" {
            value.round().to_string()
        } else {
            value.to_string()
        };
    } else if out.tag == "float-array" {
        let values: Vec<Vec<f32>> = forms
            .iter()
            .map(|n| {
                n.text
                    .split_whitespace()
                    .map(|v| v.parse().unwrap())
                    .collect()
            })
            .collect();
        out.text = (0..values[first].len())
            .map(|index| {
                forms
                    .iter()
                    .enumerate()
                    .map(|(i, _)| values[i][index] * weights[i])
                    .sum::<f32>()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join(" ");
    }
    for attr in [
        "originX", "originY", "angle", "scale", "red", "green", "blue", "alpha",
    ] {
        if out.attrs.contains_key(attr) {
            let value: f32 = forms
                .iter()
                .zip(weights)
                .map(|(n, w)| n.attrs[attr].parse::<f32>().unwrap() * w)
                .sum();
            out.attrs.insert(attr.into(), value.to_string());
        }
    }
    for index in 0..out.children.len() {
        let nodes: Vec<_> = forms.iter().map(|n| n.children[index].clone()).collect();
        out.children[index] = interpolate(&nodes, weights);
    }
    out
}
fn add_scalar(form: &mut N, name: &str, delta: f32) {
    let n = form.field_mut(name).unwrap();
    let value = n.text.parse::<f32>().unwrap() + delta;
    n.text = if n.tag == "i" {
        value.round().to_string()
    } else {
        value.to_string()
    };
}
fn add_attr(form: &mut N, name: &str, delta: f32) {
    let value = form.attrs[name].parse::<f32>().unwrap() + delta;
    form.attrs.insert(name.into(), value.to_string());
}
fn add_positions(form: &mut N, delta: &[Vec2]) {
    if delta.is_empty() {
        return;
    }
    let n = form.field_mut("positions").unwrap();
    let mut values: Vec<f32> = n
        .text
        .split_whitespace()
        .map(|v| v.parse().unwrap())
        .collect();
    for (position, d) in values.as_chunks_mut::<2>().0.iter_mut().zip(delta) {
        position[0] += d.x;
        position[1] += d.y;
    }
    n.text = values
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");
}
fn add_appearance(
    form: &mut N,
    opacity: Option<f32>,
    multiply: Option<[f32; 3]>,
    screen: Option<[f32; 3]>,
) {
    if let Some(v) = opacity {
        add_scalar(form, "opacity", v);
    }
    for (field, delta) in [("multiplyColor", multiply), ("screenColor", screen)] {
        if let Some(delta) = delta {
            let n = form.field_mut(field).unwrap();
            for (attr, value) in ["red", "green", "blue"].into_iter().zip(delta) {
                add_attr(n, attr, value);
            }
        }
    }
}
