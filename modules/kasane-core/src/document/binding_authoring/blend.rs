use super::deltas::{self, DeltaVector};
use super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BlendTableOperation {
    Insert {
        sample: BindingSample,
    },
    Move {
        sample_id: String,
        value: f32,
    },
    Delete {
        sample_id: String,
        replacement_base: Option<String>,
    },
    SetBase {
        sample_id: String,
    },
    SetParameter {
        parameter_id: String,
    },
    RemapParameter {
        parameter_id: String,
        policy: ParameterRangePolicy,
    },
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BlendAuthoringEdit {
    CreateTable {
        table: BlendShapeKeyTable,
    },
    CreateConstraint {
        constraint: BlendShapeConstraint,
    },
    CreateBinding {
        id: String,
        target_id: String,
        target_kind: BlendShapeTargetKind,
        key_table_id: String,
    },
    Table {
        id: String,
        operation: BlendTableOperation,
    },
    Constraint {
        id: String,
        parameter_id: String,
        samples: Vec<ConstraintSample>,
    },
    SetBindingTable {
        id: String,
        key_table_id: String,
        reset: bool,
    },
}
#[derive(Debug, Clone, Default)]
struct Writes {
    tables: Vec<BlendShapeKeyTable>,
    constraints: Vec<BlendShapeConstraint>,
    bindings: Vec<BlendShapeBinding>,
    meshes: Vec<Mesh>,
    mesh_bindings: Vec<MeshBinding>,
    transforms: Vec<Transform>,
    scene_bindings: Vec<SceneBinding>,
    parts: Vec<Part>,
    glues: Vec<Glue>,
    offscreens: Vec<Offscreen>,
}
#[derive(Debug, Clone)]
pub struct BlendAuthoringPlan {
    control: ConversionControl,
    report: ConversionReport,
    snapshot: Arc<()>,
    writes: Writes,
    affected: Vec<String>,
    cost: ConversionCost,
    losses: Vec<String>,
}
impl BlendAuthoringPlan {
    pub fn report(&self) -> &ConversionReport {
        &self.report
    }
    pub fn losses(&self) -> &[String] {
        &self.losses
    }
    pub fn affected_ids(&self) -> &[String] {
        &self.affected
    }
    pub fn cost(&self) -> ConversionCost {
        self.cost
    }
}
impl Writes {
    fn install(&self, d: &mut Document) {
        macro_rules! put {
            ($map:ident,$order:ident,$items:ident) => {
                for v in &self.$items {
                    if !d.content.$map.contains_key(&v.id) {
                        d.content.$order.push(v.id.clone());
                    }
                    d.content.$map.insert(v.id.clone(), v.clone());
                }
            };
        }
        put!(blend_key_tables, blend_key_table_order, tables);
        put!(blend_constraints, blend_constraint_order, constraints);
        put!(blend_bindings, blend_binding_order, bindings);
        put!(meshes, mesh_order, meshes);
        put!(bindings, binding_order, mesh_bindings);
        put!(transforms, transform_order, transforms);
        put!(scene_bindings, scene_binding_order, scene_bindings);
        put!(parts, part_order, parts);
        put!(glues, glue_order, glues);
        put!(offscreens, offscreen_order, offscreens);
        d.lookup.take();
        d.binding_lookup.take();
    }
}
impl Document {
    pub fn plan_blend_authoring(
        &self,
        edit: BlendAuthoringEdit,
        limits: AuthoringLimits,
    ) -> Result<BlendAuthoringPlan, Status> {
        self.plan_blend_authoring_with_control(edit, limits, &ConversionControl::default())
    }
    pub fn plan_blend_authoring_with_control(
        &self,
        edit: BlendAuthoringEdit,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<BlendAuthoringPlan, Status> {
        control.checkpoint()?;
        let mut writes = Writes::default();
        let mut affected = Vec::new();
        let mut losses = Vec::new();
        let mut reconstructed = Vec::new();
        let mut cost = limits.check_conversion(1, 1024, self.estimated_content_bytes())?;
        match edit {
            BlendAuthoringEdit::CreateTable { mut table } => {
                if self.contains_id(&table.id) {
                    return Err(Status::error("DUPLICATE_ID", &table.id));
                }
                self.normalize_table(&mut table)?;
                cost = self.grid_cost_with_report(
                    &[table.id.clone()],
                    &[table.samples.len().max(1)],
                    4096,
                    limits,
                )?;
                require_ok(self.validate_blend_key_table(&table))?;
                affected.push(table.id.clone());
                writes.tables.push(table);
            }
            BlendAuthoringEdit::CreateConstraint { mut constraint } => {
                if self.contains_id(&constraint.id) {
                    return Err(Status::error("DUPLICATE_ID", &constraint.id));
                }
                self.normalize_constraint(&mut constraint)?;
                cost = self.grid_cost_with_report(
                    &[constraint.id.clone()],
                    &[constraint.samples.len().max(1)],
                    4096,
                    limits,
                )?;
                require_ok(self.validate_blend_constraint(&constraint))?;
                affected.push(constraint.id.clone());
                writes.constraints.push(constraint);
            }
            BlendAuthoringEdit::CreateBinding {
                id,
                target_id,
                target_kind,
                key_table_id,
            } => {
                if self.contains_id(&id) {
                    return Err(Status::error("DUPLICATE_ID", id));
                }
                self.authoring_unlocked(&target_id)?;
                let n = self.delta_width(target_kind, &target_id)?;
                let table = if key_table_id.is_empty() {
                    None
                } else {
                    Some(
                        self.get_blend_key_table(&key_table_id)
                            .ok_or_else(|| Status::error("MISSING_KEY_TABLE", &key_table_id))?,
                    )
                };
                cost = self.grid_cost_with_report(
                    &[id.clone(), target_id.clone()],
                    &[table.map_or(1, |t| t.samples.len().max(1))],
                    n * 4 + 4096,
                    limits,
                )?;
                let forms = table.map_or_else(Vec::new, |t| {
                    t.samples
                        .iter()
                        .map(|s| (s.id.clone(), DeltaVector::zero(n)))
                        .collect()
                });
                let b = BlendShapeBinding {
                    id,
                    target_id: target_id.clone(),
                    target_kind,
                    key_table_id,
                    constraint_ids: vec![],
                    keyforms: deltas::unpack(target_kind, forms),
                };
                require_ok(self.validate_blend_binding(&b))?;
                affected.extend([b.id.clone(), target_id]);
                writes.bindings.push(b);
            }
            BlendAuthoringEdit::Constraint {
                id,
                parameter_id,
                samples,
            } => {
                let old = self
                    .get_blend_constraint(&id)
                    .ok_or_else(|| Status::error("MISSING_CONSTRAINT", &id))?;
                self.authoring_unlocked(&id)?;
                let mut c = old.clone();
                c.parameter_id = parameter_id;
                c.samples = samples;
                c.keys.clear();
                c.weights.clear();
                self.normalize_constraint(&mut c)?;
                require_ok(self.validate_blend_constraint(&c))?;
                for b in self
                    .content
                    .blend_bindings
                    .values()
                    .filter(|b| b.constraint_ids.contains(&id))
                {
                    self.authoring_unlocked(&b.id)?;
                    affected.extend([b.id.clone(), b.target_id.clone()]);
                }
                let mut report_sources = affected.clone();
                report_sources.push(id.clone());
                report_sources.sort();
                report_sources.dedup();
                cost = self.grid_cost_with_report(
                    &report_sources,
                    &[c.samples.len().max(1)],
                    4096,
                    limits,
                )?;
                affected.push(id);
                writes.constraints.push(c);
            }
            BlendAuthoringEdit::SetBindingTable {
                id,
                key_table_id,
                reset,
            } => {
                let mut b = self
                    .get_blend_binding(&id)
                    .ok_or_else(|| Status::error("MISSING_BINDING", &id))?
                    .clone();
                self.authoring_unlocked(&id)?;
                if b.key_table_id == key_table_id {
                    cost = self.conversion_report_cost(
                        &[b.id.clone(), b.target_id.clone()],
                        cost,
                        limits,
                    )?;
                }
                if b.key_table_id != key_table_id {
                    let target = if key_table_id.is_empty() {
                        None
                    } else {
                        Some(
                            self.get_blend_key_table(&key_table_id)
                                .ok_or_else(|| Status::error("MISSING_KEY_TABLE", &key_table_id))?,
                        )
                    };
                    let n = self.delta_width(b.target_kind, &b.target_id)?;
                    cost = self.grid_cost_with_report(
                        &[b.id.clone(), b.target_id.clone()],
                        &[target.map_or(1, |t| t.samples.len().max(1))],
                        n * 4 + 4096,
                        limits,
                    )?;
                    let old = self.get_blend_key_table(&b.key_table_id);
                    let compatible = old
                        .zip(target)
                        .is_some_and(|(a, b)| a.keys == b.keys && a.base_key_idx == b.base_key_idx);
                    if !reset && !b.keyforms.is_empty() && !compatible {
                        return Err(Status::error(
                            "BLEND_REMAP_REQUIRED",
                            "Accept an explicit reset or use a table conversion",
                        ));
                    }
                    if reset && !b.keyforms.is_empty() {
                        losses.push(format!("{id}: existing deltas are reset"));
                    }
                    b.keyforms = if compatible && !reset {
                        for (before, after) in
                            old.unwrap().samples.iter().zip(&target.unwrap().samples)
                        {
                            reconstructed.push(ConversionReconstruction {
                                destination: ConversionAddress {
                                    owner: b.id.clone(),
                                    element: ConversionElement::Keyform(vec![after.id.clone()]),
                                },
                                sources: vec![(
                                    ConversionAddress {
                                        owner: b.id.clone(),
                                        element: ConversionElement::Keyform(vec![before
                                            .id
                                            .clone()]),
                                    },
                                    1.,
                                )],
                            });
                        }
                        deltas::remap_ids(&b.keyforms, &target.unwrap().samples)
                    } else {
                        let forms = target.map_or_else(Vec::new, |t| {
                            t.samples
                                .iter()
                                .map(|s| (s.id.clone(), DeltaVector::zero(n)))
                                .collect()
                        });
                        deltas::unpack(b.target_kind, forms)
                    };
                    b.key_table_id = key_table_id;
                }
                require_ok(self.validate_blend_binding(&b))?;
                affected.extend([b.id.clone(), b.target_id.clone()]);
                writes.bindings.push(b);
            }
            BlendAuthoringEdit::Table { id, operation } => {
                let old = self
                    .get_blend_key_table(&id)
                    .ok_or_else(|| Status::error("MISSING_KEY_TABLE", &id))?;
                self.authoring_unlocked(&id)?;
                let mut table = old.clone();
                let mut rebase = false;
                match &operation {
                    BlendTableOperation::Insert { sample } => {
                        if !valid_uuid(&sample.id) || self.contains_id(&sample.id) {
                            return Err(Status::error("DUPLICATE_ID", &sample.id));
                        }
                        if !sample.value.is_finite() || table.keys.contains(&sample.value) {
                            return Err(Status::error("INVALID_KEYS", &id));
                        }
                        if table.samples.is_empty() {
                            table.base_sample_id = sample.id.clone();
                        }
                        table.samples.push(sample.clone());
                    }
                    BlendTableOperation::Move { sample_id, value } => {
                        if old
                            .samples
                            .iter()
                            .any(|s| s.id == *sample_id && s.value != *value)
                        {
                            losses.push(format!(
                                "{id}: sample {sample_id} changes parameter placement"
                            ));
                        }
                        table
                            .samples
                            .iter_mut()
                            .find(|s| s.id == *sample_id)
                            .ok_or_else(|| Status::error("MISSING_SAMPLE", sample_id))?
                            .value = *value;
                    }
                    BlendTableOperation::Delete {
                        sample_id,
                        replacement_base,
                    } => {
                        let i = table
                            .samples
                            .iter()
                            .position(|s| s.id == *sample_id)
                            .ok_or_else(|| Status::error("MISSING_SAMPLE", sample_id))?;
                        if table.samples.len() == 1 {
                            return Err(Status::error(
                                "LAST_BLEND_SAMPLE",
                                "Clear the parameter/table explicitly",
                            ));
                        }
                        if *sample_id == table.base_sample_id {
                            table.base_sample_id = replacement_base.clone().ok_or_else(|| {
                                Status::error("BASE_REPLACEMENT_REQUIRED", sample_id)
                            })?;
                            rebase = true;
                        }
                        table.samples.remove(i);
                        losses.push(format!(
                            "{id}: removed sample {sample_id} and its interpolation slice"
                        ));
                    }
                    BlendTableOperation::SetBase { sample_id } => {
                        if !table.samples.iter().any(|s| s.id == *sample_id) {
                            return Err(Status::error("MISSING_SAMPLE", sample_id));
                        }
                        rebase = table.base_sample_id != *sample_id;
                        table.base_sample_id = sample_id.clone();
                    }
                    BlendTableOperation::SetParameter { parameter_id } => {
                        if table.parameter_id != *parameter_id {
                            losses.push(format!("{id}: driver/availability changes"));
                        }
                        table.parameter_id = parameter_id.clone()
                    }
                    BlendTableOperation::RemapParameter {
                        parameter_id,
                        policy,
                    } => {
                        if table.parameter_id != *parameter_id {
                            losses.push(format!("{id}: driver parameter placement changes"));
                        }
                        let previous = self
                            .get_parameter(&old.parameter_id)
                            .ok_or_else(|| Status::error("MISSING_PARAMETER", &old.parameter_id))?;
                        let next = self
                            .get_parameter(parameter_id)
                            .ok_or_else(|| Status::error("MISSING_PARAMETER", parameter_id))?;
                        if *policy == ParameterRangePolicy::MapKeyValues {
                            for sample in &mut table.samples {
                                sample.value = super::range::map_value(
                                    sample.value,
                                    previous.minimum,
                                    previous.maximum,
                                    next.minimum,
                                    next.maximum,
                                );
                            }
                        }
                        table.parameter_id = parameter_id.clone();
                    }
                }
                table.samples.sort_by(|a, b| a.value.total_cmp(&b.value));
                table.keys.clear();
                self.normalize_table(&mut table)?;
                require_ok(self.validate_blend_key_table(&table))?;
                if table == *old {
                    cost = self.conversion_report_cost(&[id.clone()], cost, limits)?;
                    let report = ConversionReport::between(
                        self.conversion_inventory(&[id.clone()], control),
                        self.conversion_inventory(&[id.clone()], control),
                        &[id.clone()],
                        "Unchanged".into(),
                        vec![],
                    );
                    control.check()?;
                    return Ok(BlendAuthoringPlan {
                        control: control.clone(),
                        report,
                        snapshot: self.snapshot_identity.clone(),
                        writes: Writes {
                            tables: vec![table],
                            ..Default::default()
                        },
                        affected: vec![id],
                        cost,
                        losses,
                    });
                }
                let users: Vec<_> = self
                    .content
                    .blend_bindings
                    .values()
                    .filter(|b| b.key_table_id == id)
                    .collect();
                let bytes = users.iter().try_fold(4096usize, |n, b| {
                    self.delta_width(b.target_kind, &b.target_id)?
                        .checked_mul(4)
                        .and_then(|v| v.checked_add(n + 4096))
                        .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", &id))
                })?;
                let mut report_sources = vec![id.clone()];
                for b in &users {
                    report_sources.extend([b.id.clone(), b.target_id.clone()]);
                    if rebase {
                        if let Some(o) = self.binding_for_mesh(&b.target_id) {
                            report_sources.push(o.id.clone());
                        }
                        if let Some(o) = self.binding_for_scene(&b.target_id) {
                            report_sources.push(o.id.clone());
                        }
                    }
                }
                report_sources.sort();
                report_sources.dedup();
                cost = self.grid_cost_with_report(
                    &report_sources,
                    &[table.samples.len().max(1)],
                    bytes,
                    limits,
                )?;
                for b in users {
                    control.checkpoint()?;
                    self.authoring_unlocked(&b.id)?;
                    let mut next = b.clone();
                    let n = self.delta_width(b.target_kind, &b.target_id)?;
                    let offset = if rebase {
                        let i = old
                            .samples
                            .iter()
                            .position(|s| s.id == table.base_sample_id)
                            .ok_or_else(|| {
                                Status::error("MISSING_SAMPLE", &table.base_sample_id)
                            })?;
                        if i == old.base_key_idx {
                            DeltaVector::zero(n)
                        } else {
                            deltas::pack(&b.keyforms, i)
                        }
                    } else {
                        DeltaVector::zero(n)
                    };
                    if rebase {
                        let masks: std::collections::HashSet<_> = old
                            .samples
                            .iter()
                            .enumerate()
                            .filter(|(i, _)| *i != old.base_key_idx)
                            .map(|(i, _)| deltas::pack(&b.keyforms, i).colors)
                            .collect();
                        if masks.len() > 1 {
                            return Err(Status::error("OPTIONAL_CHANNEL_NOT_REPRESENTABLE", &b.id));
                        }
                    }
                    if rebase && offset.values.iter().any(|v| *v != 0.) {
                        if !self.blend_binding_active(b) {
                            return Err(Status::error("PENDING_REBASE_NOT_REPRESENTABLE", &b.id));
                        }
                        if b.constraint_ids.iter().any(|id| {
                            self.get_blend_constraint(id).is_none_or(|c| {
                                c.parameter_id.is_empty() || c.weights.iter().any(|w| *w != 1.)
                            })
                        }) {
                            return Err(Status::error(
                                "CONSTRAINED_REBASE_NOT_REPRESENTABLE",
                                &b.id,
                            ));
                        }
                        self.rebase_target(&mut writes, b, &offset, control)?;
                    }
                    let mut forms = Vec::new();
                    for s in &table.samples {
                        control.checkpoint()?;
                        let mut v = if let Some(i) = old.samples.iter().position(|k| k.id == s.id) {
                            if i == old.base_key_idx {
                                DeltaVector::zero(n)
                            } else {
                                deltas::pack(&b.keyforms, i)
                            }
                        } else if old.samples.is_empty() {
                            DeltaVector::zero(n)
                        } else {
                            deltas::sample(old, &b.keyforms, s.value)?
                        };
                        if rebase {
                            for (x, d) in v.values.iter_mut().zip(&offset.values) {
                                *x -= d;
                            }
                            if offset.colors != 0 {
                                v.colors = offset.colors;
                            }
                        }
                        if s.id == table.base_sample_id {
                            v.values.fill(0.);
                        }
                        if v.values.iter().any(|v| !v.is_finite()) {
                            return Err(Status::error("NON_FINITE", &b.id));
                        }
                        forms.push((s.id.clone(), v));
                    }
                    next.keyforms = deltas::unpack(b.target_kind, forms);
                    if !rebase {
                        deltas::retain_existing(&b.keyforms, &mut next.keyforms);
                    }
                    affected.extend([b.id.clone(), b.target_id.clone()]);
                    writes.bindings.push(next);
                }
                affected.push(id);
                writes.tables.push(table);
            }
        }
        macro_rules! ids {
            ($field:ident) => {
                affected.extend(writes.$field.iter().map(|v| v.id.clone()));
            };
        }
        ids!(tables);
        ids!(constraints);
        ids!(bindings);
        ids!(meshes);
        ids!(mesh_bindings);
        ids!(transforms);
        ids!(scene_bindings);
        ids!(parts);
        ids!(glues);
        ids!(offscreens);
        self.validate_blend_writes(&writes)?;
        affected.sort();
        affected.dedup();
        let before = self.conversion_inventory(&affected, control);
        let mut after = self.conversion_inventory(&affected, control);
        for t in &writes.tables {
            after.table(t);
        }
        for c in &writes.constraints {
            after.constraint(c);
        }
        for b in &writes.bindings {
            after.blend(
                b,
                writes
                    .tables
                    .iter()
                    .find(|t| t.id == b.key_table_id)
                    .or_else(|| self.get_blend_key_table(&b.key_table_id)),
            );
        }
        for b in &writes.mesh_bindings {
            after.mesh(b);
        }
        for b in &writes.scene_bindings {
            after.scene(b);
        }
        for g in &writes.glues {
            after.glue(g);
        }
        for o in &writes.offscreens {
            after.offscreen(
                o,
                writes
                    .scene_bindings
                    .iter()
                    .find(|b| b.target_id() == o.part_id)
                    .or_else(|| self.binding_for_scene(&o.part_id)),
            );
        }
        let mut report = ConversionReport::between(before, after, &affected, "Shared sample correspondence and native delta channels; rebase compensates all users; changed placement/reset is not behavior preservation".into(), losses.clone());
        report.reconstructed = reconstructed;
        control.check()?;
        Ok(BlendAuthoringPlan {
            control: control.clone(),
            report,
            snapshot: self.snapshot_identity.clone(),
            writes,
            affected,
            cost,
            losses,
        })
    }
    pub fn apply_blend_authoring_plan(&mut self, p: &BlendAuthoringPlan) -> EditResult {
        if let Err(status) = p.control.check() {
            return self.failed(status);
        }
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Finish the active edit",
            ));
        }
        if !Arc::ptr_eq(&self.snapshot_identity, &p.snapshot) {
            return self.failed(Status::error(
                "STALE_PLAN",
                "Replan shared BlendShape conversion",
            ));
        }
        let w = &p.writes;
        let changed = w
            .tables
            .iter()
            .any(|t| self.get_blend_key_table(&t.id) != Some(t))
            || w.constraints
                .iter()
                .any(|c| self.get_blend_constraint(&c.id) != Some(c))
            || w.bindings
                .iter()
                .any(|b| self.get_blend_binding(&b.id) != Some(b))
            || !w.meshes.is_empty()
            || !w.transforms.is_empty()
            || !w.parts.is_empty()
            || !w.glues.is_empty()
            || !w.offscreens.is_empty();
        if !changed {
            return self.failed(Status::ok());
        }
        w.install(self);
        self.changed(
            ChangeKind::Structure,
            self.content.mesh_order.clone(),
            p.affected.clone(),
        )
    }
    fn rebase_target(
        &self,
        w: &mut Writes,
        b: &BlendShapeBinding,
        d: &DeltaVector,
        control: &ConversionControl,
    ) -> Result<(), Status> {
        if let Some(binding) = self.binding_for_mesh(&b.target_id) {
            self.authoring_unlocked(&binding.id)?;
        }
        if let Some(binding) = self.binding_for_scene(&b.target_id) {
            self.authoring_unlocked(&binding.id)?;
        }
        let offset = &d.values;
        // Runtime truncates the ordinary order before adding deltas. Crossing
        // zero would change this discrete operation even for an integer shift.
        let order_shift = |value: f32, shift: f32| -> Result<(), Status> {
            if shift != 0. && (value < 0. || value + shift < 0.) {
                return Err(Status::error(
                    "DISCRETE_DRAW_ORDER_NOT_REPRESENTABLE",
                    &b.id,
                ));
            }
            Ok(())
        };
        let appearance = |a: &mut Appearance, start: usize| {
            a.opacity += offset[start];
            for c in 0..3 {
                a.multiply[c] += offset[start + 1 + c];
                a.screen[c] += offset[start + 4 + c];
            }
        };
        let positions = |p: &mut [Vec2]| {
            for (i, p) in p.iter_mut().enumerate() {
                p.x += offset[i * 2];
                p.y += offset[i * 2 + 1];
            }
        };
        let controllers =
            |bezier: &mut Option<crate::WarpBezier>, count: usize| -> Result<(), Status> {
                if let Some(curve) = bezier {
                    if count == 0 {
                        return Ok(());
                    }
                    if offset[..count * 2]
                        .chunks_exact(2)
                        .any(|p| p[0] != offset[0] || p[1] != offset[1])
                    {
                        return Err(Status::error(
                        "BEZIER_REBASE_NOT_REPRESENTABLE",
                        "Only uniform controller translations can represent this lattice offset",
                    ));
                    }
                    for n in &mut curve.nodes {
                        n.position.x += offset[0];
                        n.position.y += offset[1];
                        for h in &mut n.handles {
                            h.x += offset[0];
                            h.y += offset[1];
                        }
                    }
                }
                Ok(())
            };
        match b.target_kind {
            BlendShapeTargetKind::Mesh => {
                let old = self.get_mesh(&b.target_id).unwrap();
                let n = old.vertex_ids.len();
                let order = offset[n * 2 + 1];
                if order.fract() != 0. {
                    return Err(Status::error(
                        "DISCRETE_DRAW_ORDER_NOT_REPRESENTABLE",
                        &b.id,
                    ));
                }
                let i = w
                    .meshes
                    .iter()
                    .position(|m| m.id == b.target_id)
                    .unwrap_or_else(|| {
                        w.meshes.push(old.clone());
                        w.meshes.len() - 1
                    });
                let m = &mut w.meshes[i];
                let previous_order = m.draw_order.unwrap_or_else(|| {
                    self.mesh_order().iter().position(|id| id == &m.id).unwrap() as f32
                });
                order_shift(previous_order, order)?;
                positions(&mut m.base_positions);
                m.appearance.opacity += offset[n * 2];
                for c in 0..3 {
                    m.appearance.multiply[c] += offset[n * 2 + 2 + c];
                    m.appearance.screen[c] += offset[n * 2 + 5 + c];
                }
                if order != 0. {
                    m.draw_order = Some(
                        m.draw_order.unwrap_or_else(|| {
                            self.mesh_order().iter().position(|id| id == &m.id).unwrap() as f32
                        }) + order,
                    );
                }
                if let Some(old) = self.binding_for_mesh(&b.target_id) {
                    let i = w
                        .mesh_bindings
                        .iter()
                        .position(|x| x.id == old.id)
                        .unwrap_or_else(|| {
                            w.mesh_bindings.push(old.clone());
                            w.mesh_bindings.len() - 1
                        });
                    for f in &mut w.mesh_bindings[i].keyforms {
                        control.checkpoint()?;
                        order_shift(f.draw_order.unwrap_or(previous_order), order)?;
                        positions(&mut f.positions);
                        f.appearance.opacity += offset[n * 2];
                        for c in 0..3 {
                            f.appearance.multiply[c] += offset[n * 2 + 2 + c];
                            f.appearance.screen[c] += offset[n * 2 + 5 + c];
                        }
                        if let Some(o) = &mut f.draw_order {
                            *o += order;
                        }
                    }
                }
            }
            BlendShapeTargetKind::Warp | BlendShapeTargetKind::Rotation => {
                let old = self.get_transform(&b.target_id).unwrap();
                let i = w
                    .transforms
                    .iter()
                    .position(|t| t.id == b.target_id)
                    .unwrap_or_else(|| {
                        w.transforms.push(old.clone());
                        w.transforms.len() - 1
                    });
                let t = &mut w.transforms[i];
                if b.target_kind == BlendShapeTargetKind::Warp {
                    let warp = t.warp_mut().unwrap();
                    let n = warp.points.len();
                    positions(&mut warp.points);
                    controllers(&mut warp.bezier, n)?;
                    appearance(&mut t.appearance, n * 2);
                } else {
                    let p = &mut t.rotation_mut().unwrap().pose;
                    p.origin.x += f64::from(offset[0]);
                    p.origin.y += f64::from(offset[1]);
                    p.angle += offset[2];
                    p.scale += offset[3];
                    appearance(&mut t.appearance, 4);
                }
                if let Some(old) = self.binding_for_scene(&b.target_id) {
                    let i = w
                        .scene_bindings
                        .iter()
                        .position(|s| s.id == old.id)
                        .unwrap_or_else(|| {
                            w.scene_bindings.push(old.clone());
                            w.scene_bindings.len() - 1
                        });
                    match &mut w.scene_bindings[i].track {
                        SceneTrack::Warp { keyforms, .. } => {
                            for f in keyforms {
                                control.checkpoint()?;
                                let n = f.positions.len();
                                positions(&mut f.positions);
                                controllers(&mut f.bezier, n)?;
                                appearance(&mut f.appearance, n * 2);
                            }
                        }
                        SceneTrack::Rotation { keyforms, .. } => {
                            for f in keyforms {
                                control.checkpoint()?;
                                f.rotation.origin.x += f64::from(offset[0]);
                                f.rotation.origin.y += f64::from(offset[1]);
                                f.rotation.angle += offset[2];
                                f.rotation.scale += offset[3];
                                appearance(&mut f.appearance, 4);
                            }
                        }
                        _ => unreachable!(),
                    }
                }
            }
            BlendShapeTargetKind::Part => {
                if offset[0].fract() != 0. {
                    return Err(Status::error(
                        "DISCRETE_DRAW_ORDER_NOT_REPRESENTABLE",
                        &b.id,
                    ));
                }
                let old = self.get_part(&b.target_id).unwrap();
                let i = w
                    .parts
                    .iter()
                    .position(|p| p.id == b.target_id)
                    .unwrap_or_else(|| {
                        w.parts.push(old.clone());
                        w.parts.len() - 1
                    });
                order_shift(w.parts[i].draw_order, offset[0])?;
                w.parts[i].draw_order += offset[0];
                if let Some(old) = self.binding_for_scene(&b.target_id) {
                    let i = w
                        .scene_bindings
                        .iter()
                        .position(|s| s.id == old.id)
                        .unwrap_or_else(|| {
                            w.scene_bindings.push(old.clone());
                            w.scene_bindings.len() - 1
                        });
                    for f in w.scene_bindings[i].track.part_keyforms_mut().unwrap() {
                        control.checkpoint()?;
                        order_shift(f.draw_order, offset[0])?;
                        f.draw_order += offset[0];
                    }
                }
            }
            BlendShapeTargetKind::Glue => {
                let old = self.get_glue(&b.target_id).unwrap();
                let i = w
                    .glues
                    .iter()
                    .position(|g| g.id == b.target_id)
                    .unwrap_or_else(|| {
                        w.glues.push(old.clone());
                        w.glues.len() - 1
                    });
                let g = &mut w.glues[i];
                g.intensity += offset[0];
                if let Some(grid) = &mut g.binding {
                    for f in &mut grid.keyforms {
                        control.checkpoint()?;
                        f.intensity += offset[0];
                    }
                }
            }
            BlendShapeTargetKind::Offscreen => {
                let old = self.get_offscreen(&b.target_id).unwrap();
                let i = w
                    .offscreens
                    .iter()
                    .position(|g| g.id == b.target_id)
                    .unwrap_or_else(|| {
                        w.offscreens.push(old.clone());
                        w.offscreens.len() - 1
                    });
                let g = &mut w.offscreens[i];
                if g.keyforms.is_empty() {
                    g.keyforms.push(OffscreenKeyform {
                        opacity: 1.,
                        ..Default::default()
                    });
                    g.part_keyform_indices.fill(0);
                }
                let bound = self
                    .binding_for_scene(&g.part_id)
                    .filter(|b| b.axes.iter().any(|a| !a.parameter_id.is_empty()))
                    .map(|b| b.track.len());
                if g.part_keyform_indices.iter().any(|i| *i < 0)
                    || (g.part_keyform_indices.is_empty() && bound.is_some())
                {
                    let index = g.keyforms.len() as i32;
                    g.keyforms.push(OffscreenKeyform {
                        opacity: 1.,
                        ..Default::default()
                    });
                    if g.part_keyform_indices.is_empty() {
                        g.part_keyform_indices = vec![index; bound.unwrap()];
                    } else {
                        for i in &mut g.part_keyform_indices {
                            if *i < 0 {
                                *i = index;
                            }
                        }
                    }
                }
                for f in &mut g.keyforms {
                    control.checkpoint()?;
                    f.opacity += offset[0];
                    if d.colors & 1 != 0 {
                        let mut c = f.multiply.unwrap_or([1.; 3]);
                        for i in 0..3 {
                            c[i] += offset[1 + i];
                        }
                        f.multiply = Some(c);
                    }
                    if d.colors & 2 != 0 {
                        let mut c = f.screen.unwrap_or([0.; 3]);
                        for i in 0..3 {
                            c[i] += offset[4 + i];
                        }
                        f.screen = Some(c);
                    }
                }
            }
        }
        Ok(())
    }
    fn validate_blend_writes(&self, w: &Writes) -> Result<(), Status> {
        let mut view = Document::new();
        view.content.parameters = self.content.parameters.clone();
        view.content.blend_key_tables = self.content.blend_key_tables.clone();
        view.content.blend_constraints = self.content.blend_constraints.clone();
        view.content.parts = self.content.parts.clone();
        for b in &w.bindings {
            if let Some(m) = self.get_mesh(&b.target_id) {
                view.content.meshes.insert(m.id.clone(), m.clone());
            }
            if let Some(t) = self.get_transform(&b.target_id) {
                view.content.transforms.insert(t.id.clone(), t.clone());
            }
            if let Some(g) = self.get_glue(&b.target_id) {
                view.content.glues.insert(g.id.clone(), g.clone());
            }
            if let Some(o) = self.get_offscreen(&b.target_id) {
                view.content.offscreens.insert(o.id.clone(), o.clone());
                if let Some(s) = self.binding_for_scene(&o.part_id) {
                    view.content.scene_bindings.insert(s.id.clone(), s.clone());
                }
            }
        }
        w.install(&mut view);
        for m in &w.meshes {
            require_ok(self.validate_mesh_properties(m))?;
            require_ok(crate::geometry::validate_positions(&m.base_positions))?;
        }
        for t in &w.transforms {
            require_ok(self.validate_transform(t))?;
        }
        for p in &w.parts {
            require_ok(self.validate_part(p))?;
        }
        for g in &w.glues {
            require_ok(self.validate_glue(g))?;
        }
        for o in &w.offscreens {
            require_ok(self.validate_offscreen(o))?;
        }
        for mut b in w.mesh_bindings.clone() {
            require_ok(view.canonicalize_binding(&mut b))?;
        }
        for mut b in w.scene_bindings.clone() {
            require_ok(view.canonicalize_scene_binding(&mut b))?;
        }
        for t in &w.tables {
            require_ok(view.validate_blend_key_table(t))?;
        }
        for c in &w.constraints {
            require_ok(view.validate_blend_constraint(c))?;
        }
        for b in &w.bindings {
            require_ok(view.validate_blend_binding(b))?;
        }
        Ok(())
    }
}
