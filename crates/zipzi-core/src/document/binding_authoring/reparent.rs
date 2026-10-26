use super::*;
use crate::TransformId;
use crate::deformers::{PsmVec2, rotation_parent_angle, rotation_points};
use std::collections::HashSet;

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub enum ReparentStrategy {
    KeepLocalData,
    PreserveEditState,
    PreserveParameterDomain,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct ReparentEdit {
    /// Stable IDs of objects to reparent; source order is retained in the planned selection.
    pub sources: Vec<String>,
    /// Empty means the model root. Part organization is an independent relation.
    pub parent_id: String,
    /// Explicit reparenting/conversion strategy, including how shape differences are preserved
    /// or reported.
    pub strategy: ReparentStrategy,
    /// Optional placement in the resulting deformation collection.
    #[serde(default)]
    pub placement: Option<ReparentPlacement>,
    /// Absolute position error in model units, before Glue postprocessing.
    pub tolerance: f64,
    /// Relative error times the original affected geometry's model-space scale.
    #[serde(default = "default_relative_tolerance")]
    pub relative_tolerance: f64,
    /// Required for preservation. These are frozen native parameter inputs;
    /// Motion/Physics must first be resolved to parameters by the caller.
    pub context: Option<ReparentContext>,
}
#[derive(
    Debug,
    Clone,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct ReparentPlacement {
    /// None appends; Some names an unselected member of the destination.
    pub before: Option<String>,
}
fn default_relative_tolerance() -> f64 {
    1e-6
}
#[derive(
    Debug,
    Clone,
    Default,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct ReparentContext {
    /// Parameter-ID-to-value map used for the reparenting reference pose; values use each
    /// parameter domain.
    pub parameter_values: crate::PreviewValues,
    /// Empty for PreserveEditState. For domain preservation every unfixed
    /// dependency must have an explicit finite interval.
    pub parameter_domain: Vec<ConversionDomain>,
    /// Finite reference pose time in seconds; zero is the default reference time.
    pub edit_time_seconds: f64,
}
impl ReparentEdit {
    pub fn keep_local(sources: Vec<String>, parent_id: String) -> Self {
        Self {
            sources,
            parent_id,
            strategy: ReparentStrategy::KeepLocalData,
            placement: None,
            tolerance: 1e-5,
            relative_tolerance: 1e-6,
            context: None,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReparentSupport {
    Available,
    RequiresConversionPlan,
    NoChange,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReparentGuarantee {
    pub strategy: ReparentStrategy,
    pub position_scope: String,
    pub tolerance: f64,
    pub relative_tolerance: f64,
    pub reference_scale: f64,
    pub context: Option<ReparentContext>,
    pub measured_error: f64,
    pub error_bound: f64,
    /// Channels outside the position proof must be presented to the caller.
    pub effects: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct ReparentPlan {
    control: ConversionControl,
    report: ConversionReport,
    snapshot: Arc<()>,
    hierarchy: Option<crate::document::HierarchyOrder>,
    meshes: Vec<Mesh>,
    bindings: Vec<MeshBinding>,
    transforms: Vec<Transform>,
    blend_bindings: Vec<crate::BlendShapeBinding>,
    affected: Vec<String>,
    guarantee: ReparentGuarantee,
    cost: ConversionCost,
}
impl ReparentPlan {
    pub fn report(&self) -> &ConversionReport {
        &self.report
    }
    pub fn guarantee(&self) -> &ReparentGuarantee {
        &self.guarantee
    }
    pub fn cost(&self) -> ConversionCost {
        self.cost
    }
    pub fn affected_ids(&self) -> &[String] {
        &self.affected
    }
}

// The same scalar rotation implementation used by evaluation. Ancestor poses
// are resolved using the runtime's angle/scale inheritance, including reflection.
#[derive(Clone, Copy)]
struct RotationMap {
    base: f32,
    angle: f32,
    scale: f32,
    origin: PsmVec2,
    rx: bool,
    ry: bool,
}
impl RotationMap {
    fn point(self, p: Vec2) -> Vec2 {
        let mut out = [0.0; 2];
        rotation_points(
            self.base,
            self.angle,
            self.scale,
            self.origin,
            self.rx,
            self.ry,
            &[p.x, p.y],
            &mut out,
            1,
        );
        Vec2::new(out[0], out[1])
    }
    fn inverse(self, p: Vec2) -> Result<Vec2, Status> {
        let linear = Self {
            origin: PsmVec2::default(),
            ..self
        };
        let x = linear.point(Vec2::new(1.0, 0.0));
        let y = linear.point(Vec2::new(0.0, 1.0));
        let (a, b, c, d) = (
            f64::from(x.x),
            f64::from(y.x),
            f64::from(x.y),
            f64::from(y.y),
        );
        let det = a * d - b * c;
        if !det.is_finite() || det == 0.0 {
            return Err(Status::error(
                "SINGULAR_PARENT",
                "A unique inverse is required",
            ));
        }
        let px = f64::from(p.x) - f64::from(self.origin.x);
        let py = f64::from(p.y) - f64::from(self.origin.y);
        let result = Vec2::new(
            ((d * px - b * py) / det) as f32,
            ((a * py - c * px) / det) as f32,
        );
        require_ok(crate::geometry::validate_positions(&[result]))?;
        Ok(result)
    }
}
impl Document {
    fn reparent_drivers(&self, id: &str) -> Vec<String> {
        let mut out = Vec::new();
        if let Some(b) = self.binding_for_mesh(id) {
            out.extend(
                b.axes
                    .iter()
                    .filter(|a| !a.parameter_id.is_empty())
                    .map(|a| a.parameter_id.clone()),
            );
        }
        if let Some(b) = self.binding_for_scene(id) {
            out.extend(
                b.axes
                    .iter()
                    .filter(|a| !a.parameter_id.is_empty())
                    .map(|a| a.parameter_id.clone()),
            );
        }
        for b in self.iter_active_blend_bindings_for_target(id) {
            out.push(
                self.get_blend_key_table(&b.key_table_id)
                    .unwrap()
                    .parameter_id
                    .clone(),
            );
            out.extend(
                b.constraint_ids
                    .iter()
                    .map(|id| self.get_blend_constraint(id).unwrap().parameter_id.clone()),
            );
        }
        out.sort();
        out.dedup();
        out
    }
    fn validate_reparent_context(
        &self,
        edit: &ReparentEdit,
        changing: &[String],
    ) -> Result<Option<ReparentContext>, Status> {
        if edit.strategy == ReparentStrategy::KeepLocalData {
            return Ok(None);
        }
        let mut c = edit.context.clone().ok_or_else(|| {
            Status::error(
                "REPARENT_CONTEXT_REQUIRED",
                "Capture explicit native parameter inputs and edit time",
            )
        })?;
        if !c.edit_time_seconds.is_finite() || c.edit_time_seconds < 0. {
            return Err(Status::error(
                "INVALID_EDIT_TIME",
                "Expected finite nonnegative time",
            ));
        }
        if edit.strategy == ReparentStrategy::PreserveEditState && !c.parameter_domain.is_empty() {
            return Err(Status::error(
                "UNEXPECTED_PARAMETER_DOMAIN",
                "Edit state has fixed parameter values only",
            ));
        }
        let mut seen = HashSet::new();
        for d in &c.parameter_domain {
            let p = self
                .get_parameter(&d.parameter_id)
                .ok_or_else(|| Status::error("MISSING_PARAMETER", &d.parameter_id))?;
            if !seen.insert(&d.parameter_id)
                || !d.minimum.is_finite()
                || !d.maximum.is_finite()
                || d.minimum > d.maximum
                || d.minimum < p.minimum
                || d.maximum > p.maximum
            {
                return Err(Status::error("INVALID_PARAMETER_DOMAIN", &d.parameter_id));
            }
            let midpoint = (f64::from(d.minimum) * 0.5 + f64::from(d.maximum) * 0.5) as f32;
            c.parameter_values
                .entry(d.parameter_id.clone())
                .or_insert(midpoint);
        }
        for (id, &v) in &c.parameter_values {
            let p = self
                .get_parameter(id)
                .ok_or_else(|| Status::error("MISSING_PARAMETER", id))?;
            if !v.is_finite()
                || v < p.minimum
                || v > p.maximum
                || c.parameter_domain
                    .iter()
                    .any(|d| d.parameter_id == *id && (v < d.minimum || v > d.maximum))
            {
                return Err(Status::error("INVALID_EDIT_PARAMETER", id));
            }
        }
        let mut pending: Vec<_> = changing
            .iter()
            .map(|s| s.as_str())
            .chain(std::iter::once(edit.parent_id.as_str()))
            .collect();
        let mut visited = HashSet::new();
        while let Some(id) = pending.pop() {
            if id.is_empty() || !visited.insert(id) {
                continue;
            }
            for p in self.reparent_drivers(id) {
                if !c.parameter_values.contains_key(&p) {
                    return Err(Status::error("MISSING_EDIT_PARAMETER", p));
                }
            }
            if let Some(m) = self.get_mesh(id) {
                pending.push(&m.deformer_id);
            }
            if let Some(t) = self.get_transform(id) {
                pending.push(t.parent());
            }
        }
        Ok(Some(c))
    }
    fn reparent_rotation_map(
        &self,
        id: &str,
        context: &ReparentContext,
        domain: bool,
        evaluate_driven: bool,
    ) -> Result<RotationMap, Status> {
        if id.is_empty() {
            return Ok(RotationMap {
                base: 0.0,
                angle: 0.0,
                scale: 1.0,
                origin: PsmVec2::default(),
                rx: false,
                ry: false,
            });
        }
        let t = self
            .get_transform(id)
            .ok_or_else(|| Status::error("MISSING_TRANSFORM", id))?;
        let r = t.rotation().ok_or_else(|| {
            Status::error(
                "NONLINEAR_PARENT_UNPROVEN",
                format!("{id}: Warp inverse and error bound have not been proved"),
            )
        })?;
        if domain {
            for parameter in self.reparent_drivers(id) {
                if context
                    .parameter_domain
                    .iter()
                    .any(|d| d.parameter_id == parameter && d.minimum != d.maximum)
                {
                    return Err(Status::error(
                        "PARAMETER_DOMAIN_UNPROVEN",
                        format!(
                            "{id}: ancestor depends on varying parameter {parameter}; a constant affine conversion is unproved"
                        ),
                    ));
                }
            }
        }
        // Hover only checks structure/dependencies. Sampling a driven 16-axis
        // parent belongs to the heavy conversion plan, never capability lookup.
        let pose = if !evaluate_driven && !self.reparent_drivers(id).is_empty() {
            crate::RotationPose::default()
        } else {
            crate::evaluation::authoring_rotation_pose(self, id, &context.parameter_values)
        };
        let mut result = RotationMap {
            base: r.base_angle,
            angle: pose.angle,
            scale: pose.scale,
            origin: PsmVec2::new(pose.origin.x as f32, pose.origin.y as f32),
            rx: pose.reflect_x,
            ry: pose.reflect_y,
        };
        if !t.parent().is_empty() {
            let parent =
                self.reparent_rotation_map(t.parent(), context, domain, evaluate_driven)?;
            let mut zero = PsmVec2::default();
            result.angle += rotation_parent_angle(
                true,
                |p| {
                    let q = RotationMap {
                        origin: PsmVec2::default(),
                        ..parent
                    }
                    .point(Vec2::new(p.x, p.y));
                    PsmVec2::new(q.x, q.y)
                },
                &mut zero,
            );
            let origin = parent.point(Vec2::new(result.origin.x, result.origin.y));
            result.origin = PsmVec2::new(origin.x, origin.y);
            result.scale *= parent.scale;
        }
        // Zero scale is a legal native pose, but cannot provide a reversible
        // change of coordinate systems for a preservation strategy.
        if !result.scale.is_finite() || result.scale == 0.0 {
            return Err(Status::error("SINGULAR_PARENT", id));
        }
        require_ok(crate::geometry::validate_positions(&[
            result.point(Vec2::new(0.0, 0.0)),
            result.point(Vec2::new(1.0, 0.0)),
            result.point(Vec2::new(0.0, 1.0)),
        ]))?;
        Ok(result)
    }
    /// Cheap structural/driver preflight: no geometry, Cartesian forms, candidate
    /// document, or runtime frame is constructed. A successful preservation query
    /// still needs a conversion plan to check capacity and the numeric error bound.
    pub fn reparent_support(&self, edit: &ReparentEdit) -> Result<ReparentSupport, Status> {
        let (sources, changing, _) = self.prepare_reparent(edit, &ConversionControl::default())?;
        let hierarchy = self.reparent_placement(&sources, edit)?;
        Ok(if changing.is_empty() && hierarchy.is_none() {
            ReparentSupport::NoChange
        } else if changing.is_empty() || edit.strategy == ReparentStrategy::KeepLocalData {
            ReparentSupport::Available
        } else {
            ReparentSupport::RequiresConversionPlan
        })
    }
    fn prepare_reparent(
        &self,
        edit: &ReparentEdit,
        control: &ConversionControl,
    ) -> Result<(Vec<String>, Vec<String>, Option<ReparentContext>), Status> {
        if edit.strategy != ReparentStrategy::KeepLocalData
            && (!edit.tolerance.is_finite()
                || edit.tolerance <= 0.0
                || !edit.relative_tolerance.is_finite()
                || edit.relative_tolerance < 0.0)
        {
            return Err(Status::error(
                "INVALID_TOLERANCE",
                "A finite positive model-space tolerance is required",
            ));
        }
        if !edit.parent_id.is_empty() && self.get_transform(&edit.parent_id).is_none() {
            return Err(Status::error("MISSING_TRANSFORM", &edit.parent_id));
        }
        self.authoring_unlocked(&edit.parent_id)?;
        let selected: HashSet<_> = edit.sources.iter().cloned().collect();
        if selected.len() != edit.sources.len() {
            return Err(Status::error("DUPLICATE_SOURCE", "Select each source once"));
        }
        for id in &edit.sources {
            control.checkpoint()?;
            if self.get_mesh(id).is_none() && self.get_transform(id).is_none() {
                return Err(Status::error("INVALID_REPARENT_SOURCE", id));
            }
            self.authoring_unlocked(id)?;
        }
        let parent_of = |id: &str| {
            self.get_mesh(id)
                .map(|m| m.deformer_id.as_str())
                .or_else(|| self.get_transform(id).map(Transform::parent))
                .unwrap_or("")
        };
        let contained = |id: &str| {
            let mut parent = parent_of(id);
            while !parent.is_empty() {
                if selected.contains(parent) {
                    return true;
                }
                parent = parent_of(parent);
            }
            false
        };
        // Canonical document projection order, independent of selection order.
        let mut seen_sources = HashSet::new();
        let sources: Vec<_> = self
            .hierarchy_order()
            .deformation
            .iter()
            .chain(self.transform_order())
            .chain(self.mesh_order())
            .filter(|id| {
                selected.contains(*id) && !contained(id) && seen_sources.insert((*id).clone())
            })
            .cloned()
            .collect();
        for id in &sources {
            let mut parent = edit.parent_id.as_str();
            while !parent.is_empty() {
                if parent == id {
                    return Err(Status::error("RELATION_CYCLE", id));
                }
                parent = parent_of(parent);
            }
        }
        // Lock validation covers all descendants and their owned bindings,
        // including those omitted from the normalized multi-selection.
        for id in self.transform_order().iter().chain(self.mesh_order()) {
            control.checkpoint()?;
            let mut parent = id.as_str();
            let mut affected = false;
            while !parent.is_empty() {
                affected |= sources.iter().any(|s| s == parent);
                parent = parent_of(parent);
            }
            if affected {
                self.authoring_unlocked(id)?;
                if let Some(b) = self.binding_for_mesh(id) {
                    self.authoring_unlocked(&b.id)?;
                }
                if let Some(b) = self.binding_for_scene(id) {
                    self.authoring_unlocked(&b.id)?;
                }
                for b in self.blend_bindings_for_target(id) {
                    self.authoring_unlocked(&b.id)?;
                }
            }
        }
        let changing: Vec<_> = sources
            .iter()
            .filter(|id| parent_of(id) != edit.parent_id)
            .cloned()
            .collect();
        if edit.strategy != ReparentStrategy::KeepLocalData {
            for id in &changing {
                control.checkpoint()?;
                if self.get_transform(id).is_some() {
                    return Err(Status::error(
                        "SUBTREE_CONVERSION_UNPROVEN",
                        format!(
                            "{id}: use KeepLocalData; preservation of this subtree is not yet proved"
                        ),
                    ));
                }
            }
        }
        let context = self.validate_reparent_context(edit, &changing)?;
        if edit.strategy != ReparentStrategy::KeepLocalData && !changing.is_empty() {
            let context = context.as_ref().unwrap();
            let domain = edit.strategy == ReparentStrategy::PreserveParameterDomain;
            for id in &changing {
                self.reparent_rotation_map(
                    &self.get_mesh(id).unwrap().deformer_id,
                    context,
                    domain,
                    false,
                )?;
            }
            self.reparent_rotation_map(&edit.parent_id, context, domain, false)?;
        }
        Ok((sources, changing, context))
    }
    pub fn plan_reparent(
        &self,
        edit: ReparentEdit,
        limits: AuthoringLimits,
    ) -> Result<ReparentPlan, Status> {
        self.plan_reparent_with_control(edit, limits, &ConversionControl::default())
    }
    pub fn plan_reparent_with_control(
        &self,
        edit: ReparentEdit,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<ReparentPlan, Status> {
        control.checkpoint()?;
        let (sources, changing, context) = self.prepare_reparent(&edit, control)?;
        let hierarchy = self.reparent_placement(&sources, &edit)?;
        let point_count = changing
            .iter()
            .filter_map(|id| self.get_mesh(id))
            .try_fold(0usize, |count, m| {
                if edit.strategy == ReparentStrategy::KeepLocalData {
                    return count
                        .checked_add(1)
                        .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", &m.id));
                }
                let forms = self.binding_for_mesh(&m.id).map_or(0, |b| b.keyforms.len());
                let forms =
                    self.blend_bindings_for_target(&m.id)
                        .iter()
                        .try_fold(forms, |n, b| {
                            n.checked_add(b.keyforms.len())
                                .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", &m.id))
                        })?;
                m.base_positions
                    .len()
                    .checked_mul(forms + 1)
                    .and_then(|n| n.checked_add(count))
                    .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", &m.id))
            })?;
        let mut payload = point_count
            .checked_mul(std::mem::size_of::<Vec2>() * 2 + 64)
            .ok_or_else(|| {
                Status::error("AUTHORING_CAPACITY_OVERFLOW", "Mesh conversion payload")
            })?;
        if let Some(order) = &hierarchy {
            payload = order
                .organization
                .iter()
                .chain(&order.deformation)
                .chain(&order.glues)
                .try_fold(payload, |bytes, id| {
                    bytes
                        .checked_add(std::mem::size_of::<String>())
                        .and_then(|n| n.checked_add(id.len()))
                        .ok_or_else(|| {
                            Status::error(
                                "AUTHORING_CAPACITY_OVERFLOW",
                                "Reparent hierarchy payload",
                            )
                        })
                })?;
        }
        let mut combinations = if edit.strategy == ReparentStrategy::KeepLocalData {
            1
        } else {
            changing
                .iter()
                .filter_map(|id| self.binding_for_mesh(id))
                .map(|b| b.keyforms.len())
                .max()
                .unwrap_or(1)
                .max(1)
        };
        // Parent poses also expand ordinary Cartesian selections. Budget them
        // before sampling any driven ancestor, even for an unbound source Mesh.
        if edit.strategy != ReparentStrategy::KeepLocalData && !changing.is_empty() {
            let mut visited = HashSet::new();
            for parent in changing
                .iter()
                .map(|id| self.get_mesh(id).unwrap().deformer_id.as_str())
                .chain(std::iter::once(edit.parent_id.as_str()))
            {
                let mut parent = parent;
                while !parent.is_empty() && visited.insert(parent) {
                    control.checkpoint()?;
                    if let Some(b) = self
                        .binding_for_scene(parent)
                        .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
                    {
                        combinations = combinations.max(b.track.len());
                        payload = b
                            .track
                            .len()
                            .checked_mul(
                                2 * (std::mem::size_of::<usize>() + std::mem::size_of::<f32>()),
                            )
                            .and_then(|n| payload.checked_add(n))
                            .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", parent))?;
                    }
                    parent = self.get_transform(parent).unwrap().parent();
                }
            }
        }
        let cost =
            limits.check_conversion(combinations, payload, self.estimated_content_bytes())?;
        let mut affected = changing.clone();
        if hierarchy.is_some() {
            affected.extend(sources.iter().cloned());
        }
        // A transform move changes the coordinate context of its entire subtree,
        // including bindings that KeepLocalData leaves byte-for-byte intact.
        for id in self.transform_order().iter().chain(self.mesh_order()) {
            control.checkpoint()?;
            let mut parent = id.as_str();
            let mut in_subtree = false;
            while !parent.is_empty() {
                if changing.iter().any(|root| root == parent) {
                    in_subtree = true;
                    break;
                }
                parent = self
                    .get_mesh(parent)
                    .map(|m| m.deformer_id.as_str())
                    .or_else(|| self.get_transform(parent).map(Transform::parent))
                    .unwrap_or("");
            }
            if in_subtree {
                affected.push(id.clone());
                if let Some(b) = self.binding_for_mesh(id) {
                    affected.push(b.id.clone());
                }
                if let Some(b) = self.binding_for_scene(id) {
                    affected.push(b.id.clone());
                }
                affected.extend(
                    self.blend_bindings_for_target(id)
                        .iter()
                        .map(|b| b.id.clone()),
                );
            }
        }
        affected.sort();
        affected.dedup();
        let cost = self.conversion_report_cost(&affected, cost, limits)?;
        control.checkpoint()?;
        let new_map = if edit.strategy != ReparentStrategy::KeepLocalData && !changing.is_empty() {
            Some(self.reparent_rotation_map(
                &edit.parent_id,
                context.as_ref().unwrap(),
                edit.strategy == ReparentStrategy::PreserveParameterDomain,
                true,
            )?)
        } else {
            None
        };
        let mut meshes = Vec::new();
        let mut bindings = Vec::new();
        let mut transforms = Vec::new();
        let mut blend_bindings = Vec::new();
        let mut bounds: Option<[f64; 4]> = None;
        let mut measured_error = 0.0f64;
        let mut magnitude = 1.0f64;
        let mut max_axes = 0;
        for id in &changing {
            control.checkpoint()?;
            if let Some(m) = self.get_mesh(id) {
                let mut mesh = m.clone();
                if let Some(new_map) = new_map {
                    let old_map = self.reparent_rotation_map(
                        &m.deformer_id,
                        context.as_ref().unwrap(),
                        edit.strategy == ReparentStrategy::PreserveParameterDomain,
                        true,
                    )?;
                    let mut convert = |points: &mut [Vec2]| -> Result<(), Status> {
                        for chunk in points.chunks_mut(256) {
                            control.checkpoint()?;
                            for point in chunk {
                                let world = old_map.point(*point);
                                let old_linear = RotationMap {
                                    origin: PsmVec2::default(),
                                    ..old_map
                                }
                                .point(*point);
                                magnitude = magnitude.max(
                                    f64::from(old_linear.x).abs()
                                        + f64::from(old_linear.y).abs()
                                        + f64::from(old_map.origin.x).abs()
                                        + f64::from(old_map.origin.y).abs(),
                                );
                                *point = new_map.inverse(world)?;
                                let after = new_map.point(*point);
                                measured_error = measured_error.max(
                                    (f64::from(after.x) - f64::from(world.x))
                                        .hypot(f64::from(after.y) - f64::from(world.y)),
                                );
                                magnitude = magnitude
                                    .max(f64::from(world.x).abs() + f64::from(world.y).abs());
                                // Also bound intermediate products before translation,
                                // so cancellation at a large origin cannot hide error.
                                let linear = RotationMap {
                                    origin: PsmVec2::default(),
                                    ..new_map
                                }
                                .point(*point);
                                magnitude = magnitude.max(
                                    f64::from(linear.x).abs()
                                        + f64::from(linear.y).abs()
                                        + f64::from(new_map.origin.x).abs()
                                        + f64::from(new_map.origin.y).abs(),
                                );
                            }
                        }
                        Ok(())
                    };
                    convert(&mut mesh.base_positions)?;
                    if let Some(b) = self
                        .binding_for_mesh(id)
                        .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
                    {
                        let mut binding = b.clone();
                        max_axes = max_axes.max(
                            binding
                                .axes
                                .iter()
                                .filter(|a| !a.parameter_id.is_empty() && a.keys.len() > 1)
                                .count(),
                        );
                        for f in &mut binding.keyforms {
                            convert(&mut f.positions)?;
                        }
                        require_ok(self.canonicalize_binding(&mut binding))?;
                        bindings.push(binding);
                    }
                    for b in self.blend_bindings_for_target(id) {
                        let mut next = b.clone();
                        if let crate::DeltaKeyforms::Mesh(forms) = &mut next.keyforms {
                            let old_linear = RotationMap {
                                origin: PsmVec2::default(),
                                ..old_map
                            };
                            let new_linear = RotationMap {
                                origin: PsmVec2::default(),
                                ..new_map
                            };
                            let mut delta_magnitude = 0.0f64;
                            let mut delta_error = 0.0f64;
                            for f in forms {
                                control.checkpoint()?;
                                for chunk in f.positions.chunks_mut(256) {
                                    control.checkpoint()?;
                                    for p in chunk {
                                        let world = old_linear.point(*p);
                                        *p = new_linear.inverse(world)?;
                                        let after = new_linear.point(*p);
                                        delta_error = delta_error.max(
                                            (f64::from(after.x) - f64::from(world.x))
                                                .hypot(f64::from(after.y) - f64::from(world.y)),
                                        );
                                        delta_magnitude = delta_magnitude.max(
                                            f64::from(world.x).abs()
                                                + f64::from(world.y).abs()
                                                + f64::from(p.x).abs()
                                                + f64::from(p.y).abs(),
                                        );
                                    }
                                }
                            }
                            magnitude += delta_magnitude;
                            measured_error += delta_error;
                        }
                        require_ok(self.validate_blend_binding(&next))?;
                        blend_bindings.push(next);
                    }
                    require_ok(crate::geometry::validate_positions(&mesh.base_positions))?;
                }
                mesh.deformer_id.clone_from(&edit.parent_id);
                require_ok(self.validate_mesh_properties(&mesh))?;
                meshes.push(mesh);
            } else {
                let mut t = self.get_transform(id).unwrap().clone();
                t.parent_id = TransformId::optional(edit.parent_id.clone());
                require_ok(self.validate_transform(&t))?;
                transforms.push(t);
            }
        }
        if new_map.is_some() {
            let context = context.as_ref().unwrap();
            for id in &changing {
                control.checkpoint()?;
                let old_map = self.reparent_rotation_map(
                    &self.get_mesh(id).unwrap().deformer_id,
                    context,
                    edit.strategy == ReparentStrategy::PreserveParameterDomain,
                    true,
                )?;
                for p in crate::evaluation::authoring_mesh_positions(
                    self,
                    id,
                    &context.parameter_values,
                )? {
                    let p = old_map.point(p);
                    let x = f64::from(p.x);
                    let y = f64::from(p.y);
                    bounds = Some(bounds.map_or([x, y, x, y], |[x0, y0, x1, y1]| {
                        [x0.min(x), y0.min(y), x1.max(x), y1.max(y)]
                    }));
                }
            }
        }
        let reference_scale =
            bounds.map_or(1., |[x0, y0, x1, y1]| (x1 - x0).hypot(y1 - y0).max(1.));
        let allowed_error = edit.tolerance + edit.relative_tolerance * reference_scale;
        // The converted sample errors extend through convex multilinear weights.
        // Add a conservative float32 accumulation envelope for interpolation and
        // the affine maps. This is not a proof about Motion/Physics/Glue output.
        let error_bound = if new_map.is_some() {
            measured_error
                + magnitude
                    * f64::from(f32::EPSILON)
                    * (16 + 8 * (1usize << max_axes) + 8 * max_axes) as f64
        } else {
            0.0
        };
        if new_map.is_some() && (!error_bound.is_finite() || error_bound > allowed_error) {
            return Err(Status::error(
                "REPARENT_TOLERANCE_EXCEEDED",
                format!("error bound {error_bound} exceeds {allowed_error} model units"),
            ));
        }
        let effects: Vec<String> = if changing.is_empty() {
            vec![]
        } else {
            vec![
            "Opacity inheritance: unknown; not covered by the geometry proof".into(),
            "Multiply color inheritance: unknown; not covered by the geometry proof".into(),
            "Screen color inheritance: unknown; not covered by the geometry proof".into(),
            "Enabled state: unknown; source and ancestor visibility may differ".into(),
            "Parameter-domain visibility: unknown; not covered by the geometry proof".into(),
            "Glue postprocessing: unknown; guarantee ends before Glue".into(),
            "Stateful animation: only the explicitly frozen native parameter inputs are covered".into(),
        ]
        };
        let before = self.conversion_inventory(&affected, control);
        let mut after = self.conversion_inventory(&affected, control);
        for m in &meshes {
            after.scene_base(&m.id, &m.deformer_id);
        }
        for t in &transforms {
            after.scene_base(&t.id, t.parent());
        }
        let mut report = ConversionReport::between(
            before,
            after,
            &affected,
            format!("{:?}: affine geometry before Glue only", edit.strategy),
            effects.clone(),
        );
        let guarantee = ReparentGuarantee {
            strategy: edit.strategy,
            position_scope: if new_map.is_some() {
                "Affine Mesh geometry before Glue; explicit native edit snapshot/domain"
            } else {
                "Local data only"
            }
            .into(),
            tolerance: edit.tolerance,
            relative_tolerance: edit.relative_tolerance,
            reference_scale,
            context,
            measured_error,
            error_bound,
            effects,
        };
        report.reparent_guarantee = Some(guarantee.clone());
        control.check()?;
        Ok(ReparentPlan {
            control: control.clone(),
            report,
            snapshot: self.snapshot_identity.clone(),
            hierarchy,
            meshes,
            bindings,
            transforms,
            blend_bindings,
            affected,
            cost,
            guarantee,
        })
    }
    pub fn apply_reparent_plan(&mut self, plan: &ReparentPlan) -> EditResult {
        if let Err(status) = plan.control.check() {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Finish the active edit",
            ));
        }
        if !Arc::ptr_eq(&self.snapshot_identity, &plan.snapshot) {
            return self.failed(Status::error("STALE_PLAN", "Replan the parent conversion"));
        }
        if plan.affected.is_empty() {
            return self.failed(Status::ok());
        }
        if let Some(order) = &plan.hierarchy {
            self.content.hierarchy_order = order.clone();
        }
        for m in &plan.meshes {
            self.content.meshes.insert(m.id.clone(), m.clone());
        }
        for b in &plan.bindings {
            self.content.bindings.insert(b.id.clone(), b.clone());
        }
        for b in &plan.blend_bindings {
            self.content.blend_bindings.insert(b.id.clone(), b.clone());
        }
        for t in &plan.transforms {
            self.content.transforms.insert(t.id.clone(), t.clone());
        }
        self.changed(
            ChangeKind::Structure,
            self.content.mesh_order.clone(),
            plan.affected.clone(),
        )
    }
}
