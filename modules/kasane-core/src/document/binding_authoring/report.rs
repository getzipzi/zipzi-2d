use super::*;
use crate::{BlendShapeBinding, DeltaKeyforms};
use std::collections::BTreeMap;

/// Addresses are scoped to the plan's document/structural baseline. Keyforms
/// use canonical sample sets; Offscreen slots retain dedicated topology indices.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ConversionElement {
    Object,
    Base,
    ParameterRange,
    Axis(String),
    Sample(String),
    Keyform(Vec<String>),
    OffscreenSlot(usize),
    OffscreenMapping(Vec<String>),
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ConversionAddress {
    pub owner: String,
    pub element: ConversionElement,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConversionLocation {
    pub ordinal: usize,
    pub parameter_id: String,
    pub value: Option<f32>,
    pub weight: Option<f32>,
    pub mapped_slot: Option<i32>,
    pub parent_id: Option<String>,
    pub is_base: bool,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversionRemapping {
    pub address: ConversionAddress,
    pub before: ConversionLocation,
    pub after: ConversionLocation,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversionDomain {
    pub parameter_id: String,
    pub minimum: f32,
    pub maximum: f32,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversionReconstruction {
    pub destination: ConversionAddress,
    /// Explicit source addresses and multilinear weights for generated shapes.
    pub sources: Vec<(ConversionAddress, f32)>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConversionOwnerDomain {
    pub owner: String,
    pub interval: ConversionDomain,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParameterRangeConversion {
    pub parameter_id: String,
    pub before: [f32; 2],
    pub after: [f32; 2],
    pub policy: ParameterRangePolicy,
}
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConversionReport {
    /// Identity retention, not a claim that field values are unchanged.
    pub preserved: Vec<ConversionAddress>,
    /// Addresses installed/revalidated by this conversion write set.
    pub rewritten: Vec<ConversionAddress>,
    pub generated: Vec<ConversionAddress>,
    pub deleted: Vec<ConversionAddress>,
    pub remapped: Vec<ConversionRemapping>,
    /// All payload owners modified, including compensation and shared users.
    pub affected: Vec<String>,
    /// Domain before conversion. ParameterPlacementChanged also reports the
    /// new sample values in remapped; it never implies behavior equivalence.
    pub domain: Vec<ConversionOwnerDomain>,
    /// Effective result domains, including newly activated dimensions.
    pub result_domain: Vec<ConversionOwnerDomain>,
    pub reconstructed: Vec<ConversionReconstruction>,
    pub grid_guarantee: Option<GridGuarantee>,
    pub reparent_guarantee: Option<ReparentGuarantee>,
    pub parameter_range: Option<ParameterRangeConversion>,
    pub guarantee: String,
    pub losses: Vec<String>,
}
#[derive(Default)]
pub(super) struct Inventory(
    BTreeMap<ConversionAddress, ConversionLocation>,
    ConversionControl,
);
impl Inventory {
    fn item(&mut self, owner: &str, element: ConversionElement, location: ConversionLocation) {
        if self.1.checkpoint().is_err() {
            return;
        }
        self.0.insert(
            ConversionAddress {
                owner: owner.into(),
                element,
            },
            location,
        );
    }
    pub(super) fn clear(&mut self, owner: &str) {
        // Owners form contiguous ranges. Do not scan every preceding owner's
        // addresses each time a shared conversion inventories another binding.
        let start = ConversionAddress {
            owner: owner.into(),
            element: ConversionElement::Object,
        };
        let keys: Vec<_> = self
            .0
            .range(start..)
            .take_while(|(a, _)| a.owner == owner)
            .map(|(a, _)| a.clone())
            .collect();
        for key in keys {
            if self.1.checkpoint().is_err() {
                return;
            }
            self.0.remove(&key);
        }
    }
    pub(super) fn base(&mut self, owner: &str) {
        self.item(
            owner,
            ConversionElement::Object,
            ConversionLocation::default(),
        );
        self.item(
            owner,
            ConversionElement::Base,
            ConversionLocation::default(),
        );
    }
    pub(super) fn scene_base(&mut self, owner: &str, parent: &str) {
        self.item(
            owner,
            ConversionElement::Object,
            ConversionLocation::default(),
        );
        self.item(
            owner,
            ConversionElement::Base,
            ConversionLocation {
                parent_id: Some(parent.into()),
                ..Default::default()
            },
        );
    }
    pub(super) fn axes(&mut self, owner: &str, axes: &[BindingAxis]) {
        for (i, a) in axes.iter().enumerate() {
            if self.1.is_cancelled() {
                return;
            }
            self.item(
                owner,
                ConversionElement::Axis(a.id.clone()),
                ConversionLocation {
                    ordinal: i,
                    parameter_id: a.parameter_id.clone(),
                    ..Default::default()
                },
            );
            for (j, s) in a.samples.iter().enumerate() {
                if self.1.is_cancelled() {
                    return;
                }
                self.item(
                    owner,
                    ConversionElement::Sample(s.id.clone()),
                    ConversionLocation {
                        ordinal: j,
                        parameter_id: a.parameter_id.clone(),
                        value: Some(s.value),
                        ..Default::default()
                    },
                );
            }
        }
    }
    pub(super) fn forms<'a>(&mut self, owner: &str, forms: impl Iterator<Item = &'a [String]>) {
        for (i, samples) in forms.enumerate() {
            if self.1.is_cancelled() {
                return;
            }
            let mut samples = samples.to_vec();
            samples.sort();
            self.item(
                owner,
                ConversionElement::Keyform(samples),
                ConversionLocation {
                    ordinal: i,
                    ..Default::default()
                },
            );
        }
    }
    pub(super) fn mesh(&mut self, b: &MeshBinding) {
        self.clear(&b.id);
        self.item(
            &b.id,
            ConversionElement::Object,
            ConversionLocation::default(),
        );
        self.axes(&b.id, &b.axes);
        self.forms(&b.id, b.keyforms.iter().map(|f| f.samples.as_slice()));
    }
    pub(super) fn scene(&mut self, b: &SceneBinding) {
        self.clear(&b.id);
        self.item(
            &b.id,
            ConversionElement::Object,
            ConversionLocation::default(),
        );
        self.axes(&b.id, &b.axes);
        match &b.track {
            crate::SceneTrack::Warp { keyforms, .. } => {
                self.forms(&b.id, keyforms.iter().map(|f| f.samples.as_slice()))
            }
            crate::SceneTrack::Rotation { keyforms, .. } => {
                self.forms(&b.id, keyforms.iter().map(|f| f.samples.as_slice()))
            }
            crate::SceneTrack::Part { keyforms, .. } => {
                self.forms(&b.id, keyforms.iter().map(|f| f.samples.as_slice()))
            }
        }
    }
    pub(super) fn glue(&mut self, g: &Glue) {
        self.clear(&g.id);
        self.base(&g.id);
        if let Some(b) = &g.binding {
            self.axes(&g.id, &b.axes);
            self.forms(&g.id, b.keyforms.iter().map(|f| f.samples.as_slice()));
        }
    }
    pub(super) fn table(&mut self, t: &crate::BlendShapeKeyTable) {
        self.clear(&t.id);
        self.item(
            &t.id,
            ConversionElement::Object,
            ConversionLocation::default(),
        );
        for (i, s) in t.samples.iter().enumerate() {
            if self.1.is_cancelled() {
                return;
            }
            self.item(
                &t.id,
                ConversionElement::Sample(s.id.clone()),
                ConversionLocation {
                    ordinal: i,
                    parameter_id: t.parameter_id.clone(),
                    value: Some(s.value),
                    is_base: s.id == t.base_sample_id,
                    ..Default::default()
                },
            );
        }
    }
    pub(super) fn constraint(&mut self, c: &crate::BlendShapeConstraint) {
        self.clear(&c.id);
        self.item(
            &c.id,
            ConversionElement::Object,
            ConversionLocation::default(),
        );
        for (i, s) in c.samples.iter().enumerate() {
            if self.1.is_cancelled() {
                return;
            }
            self.item(
                &c.id,
                ConversionElement::Sample(s.id.clone()),
                ConversionLocation {
                    ordinal: i,
                    parameter_id: c.parameter_id.clone(),
                    value: Some(s.value),
                    weight: Some(s.weight),
                    ..Default::default()
                },
            );
        }
    }
    pub(super) fn blend(
        &mut self,
        b: &BlendShapeBinding,
        table: Option<&crate::BlendShapeKeyTable>,
    ) {
        self.clear(&b.id);
        self.item(
            &b.id,
            ConversionElement::Object,
            ConversionLocation::default(),
        );
        macro_rules! forms {
            ($v:expr) => {
                for (i, f) in $v.iter().enumerate() {
                    if self.1.is_cancelled() {
                        return;
                    }
                    self.item(
                        &b.id,
                        ConversionElement::Keyform(vec![f.sample_id.clone()]),
                        ConversionLocation {
                            ordinal: i,
                            parameter_id: table
                                .map_or_else(String::new, |t| t.parameter_id.clone()),
                            value: table.and_then(|t| t.samples.get(i)).map(|s| s.value),
                            is_base: table.is_some_and(|t| t.base_sample_id == f.sample_id),
                            ..Default::default()
                        },
                    );
                }
            };
        }
        match &b.keyforms {
            DeltaKeyforms::Mesh(v) => forms!(v),
            DeltaKeyforms::Warp(v) => forms!(v),
            DeltaKeyforms::Rotation(v) => forms!(v),
            DeltaKeyforms::Part(v) => forms!(v),
            DeltaKeyforms::Glue(v) => forms!(v),
            DeltaKeyforms::Offscreen(v) => forms!(v),
        }
    }
    pub(super) fn offscreen(&mut self, o: &Offscreen, b: Option<&SceneBinding>) {
        self.clear(&o.id);
        self.base(&o.id);
        for i in 0..o.keyforms.len() {
            self.item(
                &o.id,
                ConversionElement::OffscreenSlot(i),
                ConversionLocation {
                    ordinal: i,
                    ..Default::default()
                },
            );
        }
        let samples: Vec<_> = b
            .filter(|b| !lattice::active(&b.axes).is_empty())
            .map_or_else(
                || vec![vec![]],
                |b| {
                    (0..b.track.len())
                        .map(|i| match &b.track {
                            crate::SceneTrack::Part { keyforms, .. } => keyforms[i].samples.clone(),
                            _ => vec![],
                        })
                        .collect()
                },
            );
        for (i, mut s) in samples.into_iter().enumerate() {
            s.sort();
            self.item(
                &o.id,
                ConversionElement::OffscreenMapping(s),
                ConversionLocation {
                    ordinal: i,
                    mapped_slot: Some(
                        o.keyform_index(
                            i,
                            b.filter(|b| !lattice::active(&b.axes).is_empty())
                                .map(|b| b.track.len()),
                        )
                        .ok()
                        .flatten()
                        .map_or(-1, |i| i as i32),
                    ),
                    ..Default::default()
                },
            );
        }
    }
    pub(super) fn remap_parameter(&mut self, parameter: &str, map: impl Fn(f32) -> f32) {
        for loc in self.0.values_mut().filter(|l| l.parameter_id == parameter) {
            if let Some(v) = loc.value {
                loc.value = Some(map(v));
            }
        }
    }
}
impl Document {
    pub(super) fn conversion_inventory(
        &self,
        ids: &[String],
        control: &ConversionControl,
    ) -> Inventory {
        let mut out = Inventory(BTreeMap::new(), control.clone());
        for id in ids {
            if let Some(b) = self.get_binding(id) {
                out.mesh(b);
            } else if let Some(b) = self.get_scene_binding(id) {
                out.scene(b);
            } else if let Some(g) = self.get_glue(id) {
                out.glue(g);
            } else if let Some(t) = self.get_blend_key_table(id) {
                out.table(t);
            } else if let Some(c) = self.get_blend_constraint(id) {
                out.constraint(c);
            } else if let Some(b) = self.get_blend_binding(id) {
                out.blend(b, self.get_blend_key_table(&b.key_table_id));
            } else if let Some(o) = self.get_offscreen(id) {
                out.offscreen(o, self.binding_for_scene(&o.part_id));
            } else if self.get_parameter(id).is_some() {
                out.item(
                    id,
                    ConversionElement::ParameterRange,
                    ConversionLocation::default(),
                );
            } else if let Some(m) = self.get_mesh(id) {
                out.scene_base(id, &m.deformer_id);
            } else if let Some(t) = self.get_transform(id) {
                out.scene_base(id, t.parent());
            } else if self.contains_id(id) {
                out.base(id);
            }
        }
        out
    }
}
impl ConversionReport {
    pub(super) fn between(
        before: Inventory,
        after: Inventory,
        affected: &[String],
        guarantee: String,
        losses: Vec<String>,
    ) -> Self {
        let mut out = Self {
            affected: affected.to_vec(),
            guarantee,
            losses,
            ..Default::default()
        };
        let domains = |inventory: &Inventory| {
            let mut domain: BTreeMap<(String, String), (f32, f32)> = BTreeMap::new();
            for (address, loc) in &inventory.0 {
                if inventory.1.checkpoint().is_err() {
                    break;
                }
                if let Some(v) = loc.value.filter(|_| !loc.parameter_id.is_empty()) {
                    domain
                        .entry((address.owner.clone(), loc.parameter_id.clone()))
                        .and_modify(|(lo, hi)| {
                            *lo = lo.min(v);
                            *hi = hi.max(v);
                        })
                        .or_insert((v, v));
                }
            }
            domain
                .into_iter()
                .map(
                    |((owner, parameter_id), (minimum, maximum))| ConversionOwnerDomain {
                        owner,
                        interval: ConversionDomain {
                            parameter_id,
                            minimum,
                            maximum,
                        },
                    },
                )
                .collect()
        };
        out.domain = domains(&before);
        out.result_domain = domains(&after);
        for (address, b) in &before.0 {
            if before.1.checkpoint().is_err() {
                return out;
            }
            match after.0.get(address) {
                None => out.deleted.push(address.clone()),
                Some(a) => {
                    out.preserved.push(address.clone());
                    if a != b {
                        out.remapped.push(ConversionRemapping {
                            address: address.clone(),
                            before: b.clone(),
                            after: a.clone(),
                        });
                    }
                }
            }
        }
        out.rewritten = after.0.keys().cloned().collect();
        out.generated = after
            .0
            .keys()
            .filter(|a| !before.0.contains_key(*a))
            .cloned()
            .collect();
        out
    }
}

impl Document {
    pub(super) fn grid_form_address(
        &self,
        owner: &str,
        slot: usize,
        target: &str,
    ) -> ConversionAddress {
        let samples = if let Some(b) = self.get_binding(owner).filter(|b| !b.keyforms.is_empty()) {
            b.keyforms[slot].samples.clone()
        } else if let Some(b) = self
            .get_scene_binding(owner)
            .filter(|b| !b.track.is_empty())
        {
            match &b.track {
                crate::SceneTrack::Warp { keyforms, .. } => keyforms[slot].samples.clone(),
                crate::SceneTrack::Rotation { keyforms, .. } => keyforms[slot].samples.clone(),
                crate::SceneTrack::Part { keyforms, .. } => keyforms[slot].samples.clone(),
            }
        } else if let Some(b) = self
            .get_glue(owner)
            .and_then(|g| g.binding.as_ref())
            .filter(|b| !b.keyforms.is_empty())
        {
            b.keyforms[slot].samples.clone()
        } else {
            return ConversionAddress {
                owner: target.into(),
                element: ConversionElement::Base,
            };
        };
        let mut samples = samples;
        samples.sort();
        ConversionAddress {
            owner: owner.into(),
            element: ConversionElement::Keyform(samples),
        }
    }
}

impl Document {
    /// Account for old-address inventories before building them. New/generated
    /// address payloads are already included by the individual converters.
    pub(super) fn conversion_report_cost(
        &self,
        ids: &[String],
        cost: ConversionCost,
        limits: AuthoringLimits,
    ) -> Result<ConversionCost, Status> {
        let overflow =
            || Status::error("AUTHORING_CAPACITY_OVERFLOW", "Conversion report addresses");
        let mut payload = cost.payload_bytes;
        for id in ids {
            let axes = self
                .get_binding(id)
                .map(|b| b.axes.as_slice())
                .or_else(|| self.get_scene_binding(id).map(|b| b.axes.as_slice()))
                .or_else(|| {
                    self.get_glue(id)
                        .and_then(|g| g.binding.as_ref())
                        .map(|b| b.axes.as_slice())
                });
            let mut count = 2usize;
            let mut dimensions = 0usize;
            if let Some(axes) = axes {
                dimensions = axes.len();
                for a in axes {
                    count = count
                        .checked_add(a.samples.len())
                        .and_then(|n| n.checked_add(1))
                        .ok_or_else(overflow)?;
                }
            }
            let forms = self
                .get_binding(id)
                .map(|b| b.keyforms.len())
                .or_else(|| self.get_scene_binding(id).map(|b| b.track.len()))
                .or_else(|| {
                    self.get_glue(id)
                        .and_then(|g| g.binding.as_ref())
                        .map(|b| b.keyforms.len())
                })
                .or_else(|| self.get_blend_binding(id).map(|b| b.keyforms.len()))
                .or_else(|| self.get_blend_key_table(id).map(|t| t.samples.len()))
                .or_else(|| self.get_blend_constraint(id).map(|c| c.samples.len()))
                .unwrap_or(0);
            count = count.checked_add(forms).ok_or_else(overflow)?;
            if let Some(o) = self.get_offscreen(id) {
                let binding = self
                    .binding_for_scene(&o.part_id)
                    .filter(|b| !lattice::active(&b.axes).is_empty());
                dimensions = dimensions.max(binding.map_or(0, |b| b.axes.len()));
                let mappings = binding.map_or(1, |b| b.track.len());
                count = count
                    .checked_add(o.keyforms.len())
                    .and_then(|n| n.checked_add(mappings))
                    .ok_or_else(overflow)?;
            }
            let bytes = dimensions
                .checked_mul(80)
                .and_then(|n| n.checked_add(256))
                .and_then(|n| n.checked_mul(8))
                .and_then(|n| n.checked_mul(count))
                .ok_or_else(overflow)?;
            payload = payload.checked_add(bytes).ok_or_else(overflow)?;
        }
        limits.check_conversion(
            cost.sample_combinations,
            payload,
            self.estimated_content_bytes(),
        )
    }
}

impl Document {
    pub(super) fn grid_cost_with_report(
        &self,
        ids: &[String],
        dimensions: &[usize],
        bytes_per_form: usize,
        limits: AuthoringLimits,
    ) -> Result<ConversionCost, Status> {
        let cost = limits.check_grid(dimensions, bytes_per_form, self.estimated_content_bytes())?;
        self.conversion_report_cost(ids, cost, limits)
    }
    pub(super) fn reserve_report_limits(
        &self,
        ids: &[String],
        limits: AuthoringLimits,
    ) -> Result<AuthoringLimits, Status> {
        let reserve = self
            .conversion_report_cost(ids, ConversionCost::default(), limits)?
            .payload_bytes;
        let error = || {
            Status::error(
                "AUTHORING_MEMORY_LIMIT",
                "Conversion report exceeds the session budget",
            )
        };
        Ok(AuthoringLimits {
            max_conversion_bytes: limits
                .max_conversion_bytes
                .checked_sub(reserve.checked_mul(2).ok_or_else(error)?)
                .ok_or_else(error)?,
            max_history_bytes: limits
                .max_history_bytes
                .checked_sub(reserve)
                .ok_or_else(error)?,
            ..limits
        })
    }
}
