use super::*;
use crate::deformers::{rotation_parent_angle, rotation_points, PsmVec2};
use crate::TransformId;
use std::collections::HashSet;
mod bake;
mod keep_children;
mod residual;
mod rotation;
mod warp;
use residual::ParentResidual;

pub use zipzi_contract::document::{
    ReparentContext, ReparentEdit, ReparentPlacement, ReparentStrategy,
};

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
    scene_bindings: Vec<crate::SceneBinding>,
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

// Rotations use the same scalar implementation as evaluation. Rectangular Warp
// lattices supply an affine basis; scale still follows Rotation inheritance,
// rather than the Warp lattice dimensions.
#[derive(Clone, Copy)]
struct ParentMap {
    base: f32,
    angle: f32,
    scale: f32,
    origin: PsmVec2,
    rx: bool,
    ry: bool,
    warp_basis: Option<[Vec2; 2]>,
    residual: ParentResidual,
}
impl ParentMap {
    fn direction(self) -> f32 {
        let mut zero = PsmVec2::default();
        rotation_parent_angle(
            self.warp_basis.is_none(),
            |p| {
                let q = Self {
                    origin: PsmVec2::default(),
                    ..self
                }
                .point(Vec2::new(p.x, p.y));
                PsmVec2::new(q.x, q.y)
            },
            &mut zero,
        )
    }
    fn point(self, p: Vec2) -> Vec2 {
        if let Some([x, y]) = self.warp_basis {
            return Vec2::new(
                (f64::from(self.origin.x)
                    + f64::from(x.x) * f64::from(p.x)
                    + f64::from(y.x) * f64::from(p.y)) as f32,
                (f64::from(self.origin.y)
                    + f64::from(x.y) * f64::from(p.x)
                    + f64::from(y.y) * f64::from(p.y)) as f32,
            );
        }
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
    pub(in crate::document) fn reparent_drivers(&self, id: &str) -> Vec<String> {
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
    fn reparent_parent_map(
        &self,
        id: &str,
        context: &ReparentContext,
        domain: bool,
        evaluate_driven: bool,
    ) -> Result<ParentMap, Status> {
        if id.is_empty() {
            return Ok(ParentMap {
                base: 0.0,
                angle: 0.0,
                scale: 1.0,
                origin: PsmVec2::default(),
                rx: false,
                ry: false,
                warp_basis: None,
                residual: ParentResidual::default(),
            });
        }
        let t = self
            .get_transform(id)
            .ok_or_else(|| Status::error("MISSING_TRANSFORM", id))?;
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
        if let Some(warp) = t.warp() {
            // Reuse the construction envelope used by keep-children edits.
            // Equivalent float32 lattice construction orders need not be equal.
            if self.binding_for_scene(id).is_some()
                || !self.blend_bindings_for_target(id).is_empty()
            {
                return Err(Status::error("NONLINEAR_PARENT_UNPROVEN", id));
            }
            let parent = self.reparent_parent_map(t.parent(), context, domain, evaluate_driven)?;
            return ParentMap::warp(warp, parent).map_err(|status| {
                if status.code == "NONLINEAR_WARP_PRESERVATION_UNPROVEN" {
                    Status::error("NONLINEAR_PARENT_UNPROVEN", id)
                } else {
                    status
                }
            });
        }
        let r = t.rotation().unwrap();
        // Hover only checks structure/dependencies. Sampling a driven 16-axis
        // parent belongs to the heavy conversion plan, never capability lookup.
        let pose = if !evaluate_driven && !self.reparent_drivers(id).is_empty() {
            crate::RotationPose::default()
        } else {
            crate::evaluation::authoring_rotation_pose(self, id, &context.parameter_values)
        };
        let mut result = ParentMap {
            base: r.base_angle,
            angle: pose.angle,
            scale: pose.scale,
            origin: PsmVec2::new(pose.origin.x as f32, pose.origin.y as f32),
            rx: pose.reflect_x,
            ry: pose.reflect_y,
            warp_basis: None,
            residual: ParentResidual::default(),
        };
        if !t.parent().is_empty() {
            let parent = self.reparent_parent_map(t.parent(), context, domain, evaluate_driven)?;
            result.angle += parent.direction();
            let origin = parent.point(Vec2::new(result.origin.x, result.origin.y));
            result.origin = PsmVec2::new(origin.x, origin.y);
            result.scale *= parent.scale;
            if !parent.residual.is_zero() {
                // A rounded Warp can alter inherited direction. Without a
                // derivative proof, cover every possible Rotation direction.
                result.residual = ParentResidual::rotation(
                    parent.residual.bound(residual::Bounds::point(Vec2::new(
                        pose.origin.x as f32,
                        pose.origin.y as f32,
                    ))),
                    if parent.residual.direction_unproven {
                        2. * f64::from(result.scale).abs()
                    } else {
                        0.
                    },
                );
            }
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
        let (sources, changing, _) =
            self.prepare_reparent(edit, &ConversionControl::default(), true)?;
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
        check_locks: bool,
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
        if check_locks {
            self.authoring_unlocked(&edit.parent_id)?;
        }
        let selected: HashSet<_> = edit.sources.iter().cloned().collect();
        if selected.len() != edit.sources.len() {
            return Err(Status::error("DUPLICATE_SOURCE", "Select each source once"));
        }
        for id in &edit.sources {
            control.checkpoint()?;
            if self.get_mesh(id).is_none() && self.get_transform(id).is_none() {
                return Err(Status::error("INVALID_REPARENT_SOURCE", id));
            }
            if check_locks {
                self.authoring_unlocked(id)?;
            }
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
            if affected && check_locks {
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
        let context = self.validate_reparent_context(edit, &changing)?;
        if edit.strategy != ReparentStrategy::KeepLocalData && !changing.is_empty() {
            let context = context.as_ref().unwrap();
            let domain = edit.strategy == ReparentStrategy::PreserveParameterDomain;
            for id in &changing {
                let support = self.reparent_parent_map(
                    self.scene_deformer_parent(id).unwrap(),
                    context,
                    domain,
                    false,
                );
                if let Err(status) = support {
                    if !self.can_bake_reparent_source(edit, id, &status) {
                        return Err(status);
                    }
                }
            }
            self.reparent_parent_map(&edit.parent_id, context, domain, false)?;
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
        self.plan_reparent_with_access(edit, limits, control, true)
    }
    // The deletion planner validates the exact final write set, including locks
    // on converted bindings/samples. Read-only destinations and unchanged
    // descendants must not be locked merely because ordinary reparent checks them.
    pub(in crate::document) fn deletion_reparent_supported(
        &self,
        edit: &ReparentEdit,
    ) -> Result<(), Status> {
        self.prepare_reparent(edit, &ConversionControl::default(), false)
            .map(|_| ())
    }
    pub(in crate::document) fn plan_deletion_reparent(
        &self,
        edit: ReparentEdit,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<ReparentPlan, Status> {
        self.plan_reparent_with_access(edit, limits, control, false)
    }
    fn plan_reparent_with_access(
        &self,
        edit: ReparentEdit,
        limits: AuthoringLimits,
        control: &ConversionControl,
        check_locks: bool,
    ) -> Result<ReparentPlan, Status> {
        let (sources, changing, context) = self.prepare_reparent(&edit, control, check_locks)?;
        let hierarchy = self.reparent_placement(&sources, &edit)?;
        // A moved Warp can also compensate the first Rotation on each branch.
        // Budget all descendant Rotation channels before doing that conversion.
        let mut written = changing.clone();
        for id in self.transform_order() {
            if edit.strategy == ReparentStrategy::KeepLocalData
                || self.get_transform(id).unwrap().rotation().is_none()
                || written.contains(id)
            {
                continue;
            }
            let mut parent = self.get_transform(id).unwrap().parent();
            while !parent.is_empty() {
                if changing.iter().any(|root| {
                    root == parent && self.get_transform(root).is_some_and(|t| t.warp().is_some())
                }) {
                    written.push(id.clone());
                    break;
                }
                parent = self.get_transform(parent).unwrap().parent();
            }
        }
        let point_count = written.iter().try_fold(0usize, |count, id| {
            let Some(m) = self.get_mesh(id) else {
                let forms = self.binding_for_scene(id).map_or(0, |b| b.track.len());
                let forms = self
                    .blend_bindings_for_target(id)
                    .iter()
                    .try_fold(forms, |n, b| {
                        n.checked_add(b.keyforms.len())
                            .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", id))
                    })?;
                let points = self.get_transform(id).unwrap().warp().map_or(8, |w| {
                    w.points.len() + w.bezier.as_ref().map_or(0, |b| b.nodes.len() * 5)
                });
                return (forms + 1)
                    .checked_mul(points)
                    .and_then(|n| n.checked_add(count))
                    .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", id));
            };
            if edit.strategy == ReparentStrategy::KeepLocalData {
                return count
                    .checked_add(1)
                    .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", &m.id));
            }
            let forms = self.binding_for_mesh(&m.id).map_or(0, |b| b.keyforms.len());
            let forms = self
                .blend_bindings_for_target(&m.id)
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
        let verify_geometry =
            edit.strategy != ReparentStrategy::KeepLocalData && !changing.is_empty();
        if verify_geometry {
            // Native before/after verification retains two complete geometry frames.
            for id in self.mesh_order() {
                payload = self
                    .get_mesh(id)
                    .unwrap()
                    .base_positions
                    .len()
                    .checked_mul(8 * std::mem::size_of::<Vec2>())
                    .and_then(|n| payload.checked_add(n))
                    .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", id))?;
            }
            for id in self.transform_order() {
                let points = self
                    .get_transform(id)
                    .unwrap()
                    .warp()
                    .map_or(3, |w| w.points.len());
                payload = points
                    .checked_mul(8 * std::mem::size_of::<Vec2>())
                    .and_then(|n| payload.checked_add(n))
                    .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", id))?;
            }
        }
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
            written
                .iter()
                .map(|id| {
                    self.binding_for_mesh(id).map_or_else(
                        || self.binding_for_scene(id).map_or(1, |b| b.track.len()),
                        |b| b.keyforms.len(),
                    )
                })
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
                .map(|id| self.scene_deformer_parent(id).unwrap())
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
            Some(self.reparent_parent_map(
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
        let mut scene_bindings = Vec::new();
        let mut blend_bindings = Vec::new();
        let mut baked_sources = Vec::new();
        let mut measured_error = 0.0f64;
        let mut magnitude = 1.0f64;
        let mut max_axes = 0;
        for id in &changing {
            control.checkpoint()?;
            if let Some(m) = self.get_mesh(id) {
                let mut mesh = m.clone();
                if let Some(new_map) = new_map {
                    let old_map = match self.reparent_parent_map(
                        &m.deformer_id,
                        context.as_ref().unwrap(),
                        edit.strategy == ReparentStrategy::PreserveParameterDomain,
                        true,
                    ) {
                        Ok(map) => map,
                        Err(status) if self.can_bake_reparent_source(&edit, id, &status) => {
                            let baked = self.bake_mesh_parent(
                                id,
                                &edit.parent_id,
                                new_map,
                                context.as_ref().unwrap(),
                                control,
                            )?;
                            meshes.push(baked.mesh);
                            bindings.extend(baked.binding);
                            blend_bindings.extend(baked.blends);
                            baked_sources.push(id.clone());
                            continue;
                        }
                        Err(status) => return Err(status),
                    };
                    let mut convert = |points: &mut [Vec2]| -> Result<(), Status> {
                        for chunk in points.chunks_mut(256) {
                            control.checkpoint()?;
                            for point in chunk {
                                let world = old_map.point(*point);
                                let old_linear = ParentMap {
                                    origin: PsmVec2::default(),
                                    ..old_map
                                }
                                .point(*point);
                                magnitude = magnitude.max(
                                    f64::from(old_linear.x).abs() + f64::from(old_linear.y).abs(),
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
                                let linear = ParentMap {
                                    origin: PsmVec2::default(),
                                    ..new_map
                                }
                                .point(*point);
                                magnitude = magnitude
                                    .max(f64::from(linear.x).abs() + f64::from(linear.y).abs());
                            }
                        }
                        Ok(())
                    };
                    convert(&mut mesh.base_positions)?;
                    if let Some(b) = self.binding_for_mesh(id) {
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
                            let old_linear = ParentMap {
                                origin: PsmVec2::default(),
                                ..old_map
                            };
                            let new_linear = ParentMap {
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
                                            f64::from(world.x).abs() + f64::from(world.y).abs(),
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
                if let Some(new_map) = new_map {
                    let old_map = match self.reparent_parent_map(
                        self.get_transform(id).unwrap().parent(),
                        context.as_ref().unwrap(),
                        edit.strategy == ReparentStrategy::PreserveParameterDomain,
                        true,
                    ) {
                        Ok(map) => map,
                        Err(status) if self.can_bake_reparent_source(&edit, id, &status) => {
                            let baked = self.bake_transform_parent(
                                id,
                                &edit.parent_id,
                                new_map,
                                context.as_ref().unwrap(),
                                control,
                            )?;
                            magnitude = magnitude.max(baked.magnitude);
                            measured_error = measured_error.max(baked.error);
                            transforms.extend(baked.transforms);
                            scene_bindings.extend(baked.bindings);
                            blend_bindings.extend(baked.blends);
                            baked_sources.push(id.clone());
                            continue;
                        }
                        Err(status) => return Err(status),
                    };
                    if self.get_transform(id).unwrap().warp().is_some() {
                        let converted = self.convert_warp_parent(
                            id,
                            &edit.parent_id,
                            old_map,
                            new_map,
                            control,
                        )?;
                        magnitude = magnitude.max(converted.magnitude);
                        measured_error = measured_error.max(converted.error);
                        max_axes = max_axes.max(converted.axes);
                        transforms.extend(converted.transforms);
                        scene_bindings.extend(converted.bindings);
                        blend_bindings.extend(converted.blends);
                    } else {
                        let converted = self.convert_rotation_parent(
                            id,
                            &edit.parent_id,
                            old_map,
                            new_map,
                            control,
                        )?;
                        magnitude = magnitude.max(converted.magnitude);
                        measured_error = measured_error.max(converted.error);
                        max_axes = max_axes.max(converted.axes);
                        transforms.push(converted.transform);
                        scene_bindings.extend(converted.binding);
                        blend_bindings.extend(converted.blends);
                    }
                    continue;
                }
                let mut t = self.get_transform(id).unwrap().clone();
                t.parent_id = TransformId::optional(edit.parent_id.clone());
                require_ok(self.validate_transform(&t))?;
                transforms.push(t);
            }
        }
        // The converted sample errors extend through convex multilinear weights.
        // The forward/inverse maps have already been measured above. Reserve
        // float32 rounding for the converted inputs and multilinear accumulation;
        // charging for the affine operations again rejects exact translations.
        // Native verification below also checks the actual captured geometry.
        let error_bound = if new_map.is_some() {
            measured_error
                + magnitude * f64::from(f32::EPSILON) * (2 + (1usize << max_axes) + max_axes) as f64
        } else {
            0.0
        };
        let mut effects: Vec<String> = if changing.is_empty() {
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
        if !baked_sources.is_empty() {
            effects.push(format!(
                "Baked nonlinear parent effects at the captured parameter values for {}. Bindings remain editable; other parameter poses and Bezier controller behavior may change.",
                baked_sources.join(", ")
            ));
        }
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
            format!(
                "{:?}: geometry before Glue at the declared parameter scope",
                edit.strategy
            ),
            effects.clone(),
        );
        let guarantee = ReparentGuarantee {
            strategy: edit.strategy,
            position_scope: if !baked_sources.is_empty() {
                "Baked subtree geometry at captured native parameter values only; other poses are not preserved"
            } else if new_map.is_some() {
                "Affine Mesh, Warp and Rotation channel conversion; explicit native edit snapshot/domain"
            } else {
                "Local data only"
            }
            .into(),
            tolerance: edit.tolerance,
            relative_tolerance: edit.relative_tolerance,
            // Filled from original native Mesh and deformer geometry below.
            reference_scale: 1.,
            context,
            measured_error,
            error_bound,
            effects,
        };
        report.reparent_guarantee = Some(guarantee.clone());
        control.check()?;
        let mut plan = ReparentPlan {
            control: control.clone(),
            report,
            snapshot: self.snapshot_identity.clone(),
            hierarchy,
            meshes,
            bindings,
            transforms,
            scene_bindings,
            blend_bindings,
            affected,
            cost,
            guarantee,
        };
        if verify_geometry {
            self.verify_reparent_geometry(&mut plan, control)?;
        }
        Ok(plan)
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
        for b in &plan.scene_bindings {
            self.content.scene_bindings.insert(b.id.clone(), b.clone());
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
