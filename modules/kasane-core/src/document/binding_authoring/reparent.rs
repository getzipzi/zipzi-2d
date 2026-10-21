use super::*;
use crate::deformers::{rotation_parent_angle, rotation_points, PsmVec2};
use crate::TransformId;
use std::collections::HashSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReparentStrategy {
    KeepLocalData,
    PreserveEditState,
    PreserveParameterDomain,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReparentEdit {
    pub sources: Vec<String>,
    /// Empty means the model root. Part organization is an independent relation.
    pub parent_id: String,
    pub strategy: ReparentStrategy,
    /// Absolute position error in model units, before Glue postprocessing.
    pub tolerance: f64,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ReparentGuarantee {
    pub strategy: ReparentStrategy,
    pub position_scope: String,
    pub tolerance: f64,
    pub measured_error: f64,
    pub error_bound: f64,
    /// Channels outside the position proof must be presented to the caller.
    pub effects: Vec<String>,
}
#[derive(Debug, Clone)]
pub struct ReparentPlan {
    snapshot: Arc<()>,
    meshes: Vec<Mesh>,
    bindings: Vec<MeshBinding>,
    transforms: Vec<Transform>,
    affected: Vec<String>,
    guarantee: ReparentGuarantee,
    cost: ConversionCost,
}
impl ReparentPlan {
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
    fn static_rotation_map(&self, id: &str) -> Result<RotationMap, Status> {
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
        if self
            .binding_for_scene(id)
            .is_some_and(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
            || self
                .iter_active_blend_bindings_for_target(id)
                .next()
                .is_some()
        {
            return Err(Status::error(
                "PARAMETER_DOMAIN_UNPROVEN",
                format!("{id}: the coordinate conversion may depend on another driver"),
            ));
        }
        let mut result = RotationMap {
            base: r.base_angle,
            angle: r.pose.angle,
            scale: r.pose.scale,
            origin: PsmVec2::new(r.pose.origin.x as f32, r.pose.origin.y as f32),
            rx: r.pose.reflect_x,
            ry: r.pose.reflect_y,
        };
        if !t.parent().is_empty() {
            let parent = self.static_rotation_map(t.parent())?;
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
    pub fn plan_reparent(
        &self,
        edit: ReparentEdit,
        limits: AuthoringLimits,
    ) -> Result<ReparentPlan, Status> {
        if edit.strategy != ReparentStrategy::KeepLocalData
            && (!edit.tolerance.is_finite() || edit.tolerance <= 0.0)
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
            .collect();
        if edit.strategy != ReparentStrategy::KeepLocalData {
            for id in &changing {
                if self.get_transform(id).is_some() {
                    return Err(Status::error("SUBTREE_CONVERSION_UNPROVEN", format!("{id}: use KeepLocalData; preservation of this subtree is not yet proved")));
                }
                if !self.blend_bindings_for_target(id).is_empty() {
                    return Err(Status::error("BLEND_CONVERSION_UNPROVEN", *id));
                }
            }
        }
        let point_count = changing
            .iter()
            .filter_map(|id| self.get_mesh(id))
            .try_fold(0usize, |count, m| {
                let forms = self.binding_for_mesh(&m.id).map_or(0, |b| b.keyforms.len());
                m.base_positions
                    .len()
                    .checked_mul(forms + 1)
                    .and_then(|n| n.checked_add(count))
                    .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", &m.id))
            })?;
        let payload = point_count
            .checked_mul(std::mem::size_of::<Vec2>() * 2 + 64)
            .ok_or_else(|| {
                Status::error("AUTHORING_CAPACITY_OVERFLOW", "Mesh conversion payload")
            })?;
        let combinations = changing
            .iter()
            .filter_map(|id| self.binding_for_mesh(id))
            .map(|b| b.keyforms.len())
            .max()
            .unwrap_or(1)
            .max(1);
        let cost =
            limits.check_conversion(combinations, payload, self.estimated_content_bytes())?;
        let mut meshes = Vec::new();
        let mut bindings = Vec::new();
        let mut transforms = Vec::new();
        let mut measured_error = 0.0f64;
        let mut magnitude = 1.0f64;
        let mut max_axes = 0;
        let new_map = if edit.strategy != ReparentStrategy::KeepLocalData && !changing.is_empty() {
            Some(self.static_rotation_map(&edit.parent_id)?)
        } else {
            None
        };
        for id in &changing {
            if let Some(m) = self.get_mesh(id) {
                let mut mesh = m.clone();
                if let Some(new_map) = new_map {
                    let old_map = self.static_rotation_map(&m.deformer_id)?;
                    let mut convert = |points: &mut [Vec2]| -> Result<(), Status> {
                        for point in points {
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
                            magnitude =
                                magnitude.max(f64::from(world.x).abs() + f64::from(world.y).abs());
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
                                .filter(|a| !a.parameter_id.is_empty())
                                .count(),
                        );
                        for f in &mut binding.keyforms {
                            convert(&mut f.positions)?;
                        }
                        require_ok(self.canonicalize_binding(&mut binding))?;
                        bindings.push(binding);
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
        if new_map.is_some() && (!error_bound.is_finite() || error_bound > edit.tolerance) {
            return Err(Status::error(
                "REPARENT_TOLERANCE_EXCEEDED",
                format!("error bound {error_bound} exceeds {}", edit.tolerance),
            ));
        }
        let mut affected: Vec<_> = changing.iter().map(|id| (*id).clone()).collect();
        affected.extend(bindings.iter().map(|b| b.id.clone()));
        let effects = if changing.is_empty() {
            vec![]
        } else {
            vec![
            "Parent opacity and color inheritance may change; no appearance preservation is claimed".into(),
            "Parent enabled state and parameter-domain visibility may change".into(),
            "Glue postprocessing and stateful animation are outside the position guarantee".into(),
        ]
        };
        Ok(ReparentPlan {
            snapshot: self.snapshot_identity.clone(),
            meshes,
            bindings,
            transforms,
            affected,
            cost,
            guarantee: ReparentGuarantee {
                strategy: edit.strategy,
                position_scope: if new_map.is_some() {
                    "Affine Mesh geometry before Glue; ordinary binding effective domain"
                } else {
                    "Local data only"
                }
                .into(),
                tolerance: edit.tolerance,
                measured_error,
                error_bound,
                effects,
            },
        })
    }
    pub fn apply_reparent_plan(&mut self, plan: &ReparentPlan) -> EditResult {
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
        for m in &plan.meshes {
            self.content.meshes.insert(m.id.clone(), m.clone());
        }
        for b in &plan.bindings {
            self.content.bindings.insert(b.id.clone(), b.clone());
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
