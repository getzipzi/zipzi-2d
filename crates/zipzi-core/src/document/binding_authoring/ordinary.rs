use super::lattice::{self, active};
use super::*;
use crate::{
    GlueBinding, GlueKeyform, MeshKeyform, OffscreenKeyform, PartKeyform, RotationKeyform,
    SceneTrack, WarpKeyform,
};
use zipzi_core::OffscreenLookup;
use zipzi_core::SceneTrackEvaluation;

pub use zipzi_contract::document::{BindingGridEdit, BindingGridTarget};

#[derive(Debug, Clone)]
pub(super) enum GridFragment {
    Delete {
        id: String,
        scene: bool,
    },
    Mesh {
        binding: MeshBinding,
        base: Option<Mesh>,
        create: bool,
    },
    Scene {
        binding: SceneBinding,
        transform: Option<Transform>,
        part: Option<Part>,
        offscreen: Option<Offscreen>,
        create: bool,
    },
    Glue(Glue),
}

#[derive(Debug, Clone)]
pub struct BindingGridPlan {
    control: ConversionControl,
    report: ConversionReport,
    pub(super) snapshot: Arc<()>,
    pub(super) fragment: GridFragment,
    cost: ConversionCost,
    guarantee: GridGuarantee,
    affected: Vec<String>,
}
impl BindingGridPlan {
    pub fn report(&self) -> &ConversionReport {
        &self.report
    }
    pub fn cost(&self) -> ConversionCost {
        self.cost
    }
    pub fn guarantee(&self) -> &GridGuarantee {
        &self.guarantee
    }
    pub fn affected_ids(&self) -> &[String] {
        &self.affected
    }
    pub fn axes(&self) -> &[BindingAxis] {
        match &self.fragment {
            GridFragment::Delete { .. } => &[],
            GridFragment::Mesh { binding, .. } => &binding.axes,
            GridFragment::Scene { binding, .. } => &binding.axes,
            GridFragment::Glue(g) => &g.binding.as_ref().unwrap().axes,
        }
    }
    pub fn mesh_binding(&self) -> Option<&MeshBinding> {
        if let GridFragment::Mesh { binding, .. } = &self.fragment {
            Some(binding)
        } else {
            None
        }
    }
}
impl Document {
    pub fn plan_binding_grid(
        &self,
        edit: BindingGridEdit,
        limits: AuthoringLimits,
    ) -> Result<BindingGridPlan, Status> {
        self.plan_binding_grid_with_control(edit, limits, &ConversionControl::default())
    }
    pub fn plan_binding_grid_with_control(
        &self,
        edit: BindingGridEdit,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<BindingGridPlan, Status> {
        self.plan_binding_grid_impl(edit, limits, control, true)
    }
    pub(in crate::document) fn plan_deletion_grid(
        &self,
        edit: BindingGridEdit,
        limits: AuthoringLimits,
        control: &ConversionControl,
    ) -> Result<BindingGridPlan, Status> {
        self.plan_binding_grid_impl(edit, limits, control, false)
    }
    fn plan_binding_grid_impl(
        &self,
        edit: BindingGridEdit,
        limits: AuthoringLimits,
        control: &ConversionControl,
        check_locks: bool,
    ) -> Result<BindingGridPlan, Status> {
        control.checkpoint()?;
        if let BindingGridEdit::Delete { owner } = &edit {
            let (target, scene) = if let Some(b) = self.get_binding(owner) {
                (b.mesh_id.clone(), false)
            } else if let Some(b) = self.get_scene_binding(owner) {
                (b.target_id().to_owned(), true)
            } else {
                return Err(Status::error("MISSING_BINDING", owner));
            };
            if check_locks {
                self.authoring_unlocked(owner)?;
                self.authoring_unlocked(&target)?;
            }
            if scene && self.offscreen_for_part(&target).is_some() {
                return Err(Status::error(
                    "OFFSCREEN_MAPPING_REQUIRES_CONVERSION",
                    owner,
                ));
            }
            let affected = vec![owner.clone(), target];
            let cost = self.conversion_report_cost(
                &affected,
                limits.check_conversion(0, 0, self.estimated_content_bytes())?,
                limits,
            )?;
            let before = self.conversion_inventory(&affected, control);
            let mut after = self.conversion_inventory(&affected, control);
            after.clear(owner);
            let report = ConversionReport::between(before, after, &affected,
                "Return to the unchanged base target".into(),
                vec!["Discard all owned binding axes, samples and keyforms; shared parameters and the base target are retained".into()]);
            control.check()?;
            return Ok(BindingGridPlan {
                control: control.clone(),
                report,
                snapshot: self.snapshot_identity.clone(),
                fragment: GridFragment::Delete {
                    id: owner.clone(),
                    scene,
                },
                cost,
                guarantee: GridGuarantee::BaseShape,
                affected,
            });
        }
        let (id, target, op, create) = match edit {
            BindingGridEdit::Delete { .. } => unreachable!(),
            BindingGridEdit::Create { id, target } => (id, target, None, true),
            BindingGridEdit::Edit { owner, operation } => {
                let target = if let Some(b) = self.get_binding(&owner) {
                    BindingGridTarget::Mesh(b.mesh_id.clone())
                } else if let Some(b) = self.get_scene_binding(&owner) {
                    BindingGridTarget::Scene(b.target_id().into())
                } else if self.get_glue(&owner).is_some_and(|g| g.binding.is_some()) {
                    BindingGridTarget::Glue(owner.clone())
                } else {
                    return Err(Status::error("MISSING_BINDING", owner));
                };
                (owner, target, Some(operation), false)
            }
        };
        let target_id = match &target {
            BindingGridTarget::Mesh(t)
            | BindingGridTarget::Scene(t)
            | BindingGridTarget::Glue(t) => t,
        };
        if check_locks {
            self.authoring_unlocked(&id)?;
            self.authoring_unlocked(target_id)?;
        }
        if create && !matches!(&target, BindingGridTarget::Glue(_)) && self.contains_id(&id) {
            return Err(Status::error("DUPLICATE_ID", &id));
        }
        let old_axes: &[BindingAxis] = if create {
            &[]
        } else {
            match &target {
                BindingGridTarget::Mesh(_) => &self.get_binding(&id).unwrap().axes,
                BindingGridTarget::Scene(_) => &self.get_scene_binding(&id).unwrap().axes,
                BindingGridTarget::Glue(_) => {
                    &self.get_glue(&id).unwrap().binding.as_ref().unwrap().axes
                }
            }
        };
        let bytes_per_form = 1024usize
            .checked_add(
                match &target {
                    BindingGridTarget::Mesh(t) => self
                        .get_mesh(t)
                        .ok_or_else(|| Status::error("MISSING_MESH", t))?
                        .base_positions
                        .len()
                        .checked_mul(16),
                    BindingGridTarget::Scene(t) => {
                        if let Some(w) = self.get_transform(t).and_then(Transform::warp) {
                            w.points.len().checked_mul(16).and_then(|n| {
                                n.checked_add(w.bezier.as_ref().map_or(0, |b| {
                                    b.nodes.len() * std::mem::size_of::<crate::WarpBezierNode>()
                                }))
                            })
                        } else if self.get_transform(t).is_some() || self.get_part(t).is_some() {
                            Some(256)
                        } else {
                            return Err(Status::error("MISSING_OBJECT", t));
                        }
                    }
                    BindingGridTarget::Glue(t) => {
                        if self.get_glue(t).is_none() {
                            return Err(Status::error("MISSING_GLUE", t));
                        }
                        Some(64)
                    }
                }
                .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", &id))?,
            )
            .ok_or_else(|| Status::error("AUTHORING_CAPACITY_OVERFLOW", &id))?;
        let mut report_sources = vec![id.clone(), target_id.clone()];
        if let Some(o) = self.offscreen_for_part(target_id) {
            report_sources.push(o.id.clone());
        }
        report_sources.sort();
        report_sources.dedup();
        let grid_limits = self.reserve_report_limits(&report_sources, limits)?;
        let lattice = if let Some(op) = op {
            self.convert_lattice(old_axes, op, bytes_per_form, grid_limits, control)?
        } else {
            lattice::Lattice {
                axes: vec![],
                weights: vec![vec![(0, 1.)]],
                coordinates: vec![vec![]],
                cost: grid_limits.check_grid(
                    &[],
                    bytes_per_form,
                    self.estimated_content_bytes(),
                )?,
                guarantee: GridGuarantee::BaseShape,
            }
        };
        self.check_binding_ids(
            &id,
            lattice.axes.iter().flat_map(|a| {
                std::iter::once(a.id.clone()).chain(a.samples.iter().map(|s| s.id.clone()))
            }),
        )?;
        let driven = !active(&lattice.axes).is_empty();
        let old_driven = !active(old_axes).is_empty();
        let mut affected = vec![id.clone(), target_id.clone()];
        let report_owner = id.clone();
        let report_target = target_id.clone();
        let fragment = match target {
            BindingGridTarget::Mesh(t) => {
                let mesh = self.get_mesh(&t).unwrap();
                if create && self.binding_for_mesh(&t).is_some() {
                    return Err(Status::error("BINDING_CONFLICT", t));
                }
                let old = (!create).then(|| self.get_binding(&id).unwrap());
                let fallback = mesh.draw_order.unwrap_or_else(|| {
                    self.mesh_order().iter().position(|id| id == &t).unwrap() as f32
                });
                let base_form = MeshKeyform {
                    positions: if old_driven {
                        vec![]
                    } else {
                        mesh.base_positions.clone()
                    },
                    appearance: mesh.appearance,
                    draw_order: mesh.draw_order,
                    ..Default::default()
                };
                let form = |slot: usize| {
                    if old_driven {
                        &old.unwrap().keyforms[slot]
                    } else {
                        &base_form
                    }
                };
                let mut forms = Vec::new();
                for (w, coordinates) in lattice.weights.iter().zip(&lattice.coordinates) {
                    control.checkpoint()?;
                    let f = if w.len() == 1 {
                        form(w[0].0).clone()
                    } else {
                        MeshKeyform {
                            positions: lattice::points(w, |i| form(i).positions.as_slice())?,
                            appearance: lattice::appearance(w, |i| form(i).appearance),
                            draw_order: if w.iter().all(|(i, _)| form(*i).draw_order.is_none()) {
                                None
                            } else {
                                Some(
                                    w.iter()
                                        .map(|(i, w)| form(*i).draw_order.unwrap_or(fallback) * w)
                                        .sum(),
                                )
                            },
                            ..Default::default()
                        }
                    };
                    forms.push(MeshKeyform {
                        keys: vec![],
                        samples: coordinates.clone(),
                        ..f
                    });
                }
                let base = if !driven && old_driven {
                    let f = &forms[0];
                    let mut m = mesh.clone();
                    m.base_positions = f.positions.clone();
                    m.appearance = f.appearance;
                    m.draw_order = f.draw_order;
                    require_ok(self.validate_mesh_properties(&m))?;
                    Some(m)
                } else {
                    None
                };
                let mut binding = MeshBinding {
                    id,
                    mesh_id: t,
                    axes: lattice.axes.clone(),
                    keyforms: if driven { forms } else { vec![] },
                };
                require_ok(self.canonicalize_binding(&mut binding))?;
                GridFragment::Mesh {
                    binding,
                    base,
                    create,
                }
            }
            BindingGridTarget::Scene(t) => {
                if create && self.binding_for_scene(&t).is_some() {
                    return Err(Status::error("BINDING_CONFLICT", t));
                }
                let source = (!create).then(|| self.get_scene_binding(&id).unwrap());
                let mut transform = None;
                let mut part = None;
                let track = if let Some(base) = self.get_transform(&t) {
                    if let Some(warp) = base.warp() {
                        let base_form = WarpKeyform {
                            positions: if old_driven {
                                vec![]
                            } else {
                                warp.points.clone()
                            },
                            appearance: base.appearance,
                            bezier: if old_driven {
                                None
                            } else {
                                warp.bezier.clone()
                            },
                            ..Default::default()
                        };
                        let form = |slot: usize| {
                            if old_driven {
                                match &source.unwrap().track {
                                    SceneTrack::Warp { keyforms, .. } => &keyforms[slot],
                                    _ => unreachable!(),
                                }
                            } else {
                                &base_form
                            }
                        };
                        let mut forms = Vec::new();
                        for (w, coordinates) in lattice.weights.iter().zip(&lattice.coordinates) {
                            control.checkpoint()?;
                            let f = if w.len() == 1 {
                                form(w[0].0).clone()
                            } else {
                                WarpKeyform {
                                    positions: lattice::points(w, |i| {
                                        form(i).positions.as_slice()
                                    })?,
                                    appearance: lattice::appearance(w, |i| form(i).appearance),
                                    bezier: lattice::bezier(w, |i| form(i).bezier.as_ref())?,
                                    ..Default::default()
                                }
                            };
                            forms.push(WarpKeyform {
                                keys: vec![],
                                samples: coordinates.clone(),
                                ..f
                            });
                        }
                        if !driven && old_driven {
                            let mut b = base.clone();
                            b.warp_mut().unwrap().points = forms[0].positions.clone();
                            b.warp_mut().unwrap().bezier = forms[0].bezier.clone();
                            b.appearance = forms[0].appearance;
                            require_ok(self.validate_transform(&b))?;
                            transform = Some(b);
                        }
                        SceneTrack::Warp {
                            target_id: t.clone().into(),
                            keyforms: if driven { forms } else { vec![] },
                        }
                    } else {
                        let base_form = RotationKeyform {
                            rotation: base.rotation().unwrap().pose,
                            appearance: base.appearance,
                            ..Default::default()
                        };
                        let form = |slot: usize| {
                            if old_driven {
                                match &source.unwrap().track {
                                    SceneTrack::Rotation { keyforms, .. } => &keyforms[slot],
                                    _ => unreachable!(),
                                }
                            } else {
                                &base_form
                            }
                        };
                        let mut forms = Vec::new();
                        for (w, coordinates) in lattice.weights.iter().zip(&lattice.coordinates) {
                            control.checkpoint()?;
                            let f = if w.len() == 1 {
                                form(w[0].0).clone()
                            } else {
                                RotationKeyform {
                                    rotation: lattice::rotation(w, |i| form(i).rotation)?,
                                    appearance: lattice::appearance(w, |i| form(i).appearance),
                                    ..Default::default()
                                }
                            };
                            forms.push(RotationKeyform {
                                keys: vec![],
                                samples: coordinates.clone(),
                                ..f
                            });
                        }
                        if !driven && old_driven {
                            let mut b = base.clone();
                            b.rotation_mut().unwrap().pose = forms[0].rotation;
                            b.appearance = forms[0].appearance;
                            require_ok(self.validate_transform(&b))?;
                            transform = Some(b);
                        }
                        SceneTrack::Rotation {
                            target_id: t.clone().into(),
                            keyforms: if driven { forms } else { vec![] },
                        }
                    }
                } else {
                    let base = self.get_part(&t).unwrap();
                    let form = |slot: usize| {
                        if old_driven {
                            source.unwrap().track.sample(slot).draw_order
                        } else {
                            base.draw_order
                        }
                    };
                    let forms: Vec<_> = lattice
                        .weights
                        .iter()
                        .zip(&lattice.coordinates)
                        .map(|(w, c)| PartKeyform {
                            keys: vec![],
                            samples: c.clone(),
                            draw_order: w.iter().map(|(i, w)| form(*i) * w).sum(),
                        })
                        .collect();
                    if !driven && old_driven {
                        let mut b = base.clone();
                        b.draw_order = forms[0].draw_order;
                        require_ok(validate_draw_order(b.draw_order, &b.id))?;
                        part = Some(b);
                    }
                    SceneTrack::Part {
                        target_id: t.clone().into(),
                        keyforms: if driven { forms } else { vec![] },
                    }
                };
                let offscreen = if let Some(os) = self.offscreen_for_part(&t) {
                    if check_locks {
                        self.authoring_unlocked(&os.id)?;
                    }
                    affected.push(os.id.clone());
                    let mut out = os.clone();
                    let mut mapping = Vec::new();
                    for weights in &lattice.weights {
                        control.checkpoint()?;
                        if weights.len() == 1 {
                            let index = os.keyform_index(
                                weights[0].0,
                                if old_driven {
                                    Some(source.unwrap().track.len())
                                } else {
                                    None
                                },
                            )?;
                            mapping.push(index.map_or(-1, |i| i as i32));
                        } else {
                            let get = |slot| -> Result<OffscreenKeyform, Status> {
                                Ok(os
                                    .keyform_index(
                                        slot,
                                        if old_driven {
                                            Some(source.unwrap().track.len())
                                        } else {
                                            None
                                        },
                                    )?
                                    .map_or(
                                        OffscreenKeyform {
                                            opacity: 1.,
                                            ..Default::default()
                                        },
                                        |i| os.keyforms[i].clone(),
                                    ))
                            };
                            let mut f = OffscreenKeyform {
                                opacity: 0.,
                                multiply: Some([0.; 3]),
                                screen: Some([0.; 3]),
                            };
                            for &(i, w) in weights {
                                let v = get(i)?;
                                f.opacity += v.opacity * w;
                                for c in 0..3 {
                                    f.multiply.as_mut().unwrap()[c] +=
                                        v.multiply.unwrap_or([1.; 3])[c] * w;
                                    f.screen.as_mut().unwrap()[c] +=
                                        v.screen.unwrap_or([0.; 3])[c] * w;
                                }
                            }
                            mapping.push(out.keyforms.len() as i32);
                            out.keyforms.push(f);
                        }
                    }
                    out.part_keyform_indices = mapping;
                    Some(out)
                } else {
                    None
                };
                let mut binding = SceneBinding {
                    id,
                    axes: lattice.axes.clone(),
                    track,
                };
                // A small final-view workspace contains only changed target storage.
                let mut view = Document::new();
                view.content.parameters = self.content.parameters.clone();
                if let Some(t) = transform.as_ref().or_else(|| self.get_transform(&t)) {
                    view.content.transforms.insert(t.id.clone(), t.clone());
                }
                if let Some(p) = part.as_ref().or_else(|| self.get_part(&t)) {
                    view.content.parts.insert(p.id.clone(), p.clone());
                }
                if let Some(os) = &offscreen {
                    view.content.offscreens.insert(os.id.clone(), os.clone());
                    view.content.offscreen_order.push(os.id.clone());
                }
                require_ok(view.canonicalize_scene_binding(&mut binding))?;
                if let Some(os) = &offscreen {
                    view.content
                        .scene_bindings
                        .insert(binding.id.clone(), binding.clone());
                    view.lookup.take();
                    view.binding_lookup.take();
                    require_ok(self.validate_offscreen_key_count(
                        os,
                        if driven { binding.track.len() } else { 1 },
                    ))?;
                }
                GridFragment::Scene {
                    binding,
                    transform,
                    part,
                    offscreen,
                    create,
                }
            }
            BindingGridTarget::Glue(t) => {
                if id != t {
                    return Err(Status::error("INVALID_BINDING_OWNER", id));
                }
                let original = self.get_glue(&t).unwrap();
                if create && original.binding.is_some() {
                    return Err(Status::error("BINDING_CONFLICT", t));
                }
                let mut glue = original.clone();
                let form = |slot: usize| {
                    if old_driven {
                        original.binding.as_ref().unwrap().keyforms[slot].intensity
                    } else {
                        original.intensity
                    }
                };
                let forms: Vec<_> = lattice
                    .weights
                    .iter()
                    .zip(&lattice.coordinates)
                    .map(|(w, c)| GlueKeyform {
                        samples: c.clone(),
                        intensity: w.iter().map(|(i, w)| form(*i) * w).sum(),
                    })
                    .collect();
                if !driven && old_driven {
                    glue.intensity = forms[0].intensity;
                }
                glue.binding = Some(GlueBinding {
                    axes: lattice.axes.clone(),
                    keyforms: if driven { forms } else { vec![] },
                });
                require_ok(self.validate_glue(&glue))?;
                GridFragment::Glue(glue)
            }
        };
        affected.sort();
        affected.dedup();
        let cost = self.conversion_report_cost(&affected, lattice.cost, limits)?;
        let before = self.conversion_inventory(&affected, control);
        let mut after = self.conversion_inventory(&affected, control);
        match &fragment {
            GridFragment::Delete { .. } => unreachable!(),
            GridFragment::Mesh { binding, .. } => after.mesh(binding),
            GridFragment::Scene {
                binding, offscreen, ..
            } => {
                after.scene(binding);
                if let Some(o) = offscreen {
                    after.offscreen(o, Some(binding));
                }
            }
            GridFragment::Glue(g) => after.glue(g),
        }
        let mut report = ConversionReport::between(
            before,
            after,
            &affected,
            format!(
                "{:?}; domain interpolation uses native channel rules; precision snapping is excluded",
                lattice.guarantee
            ),
            vec![],
        );
        report.grid_guarantee = Some(lattice.guarantee.clone());
        if matches!(
            lattice.guarantee,
            GridGuarantee::DimensionRemoved { .. }
                | GridGuarantee::SamplesRemoved { .. }
                | GridGuarantee::ParameterPlacementChanged
        ) {
            report.losses.push(format!(
                "{:?}: parameter behavior changes; removed addresses are listed",
                lattice.guarantee
            ));
        }
        for (w, c) in lattice.weights.iter().zip(&lattice.coordinates) {
            control.checkpoint()?;
            let mut samples = c.clone();
            samples.sort();
            let destination = ConversionAddress {
                owner: if driven {
                    report_owner.clone()
                } else {
                    report_target.clone()
                },
                element: if driven {
                    ConversionElement::Keyform(samples)
                } else {
                    ConversionElement::Base
                },
            };
            report.reconstructed.push(ConversionReconstruction {
                destination,
                sources: w
                    .iter()
                    .map(|&(slot, weight)| {
                        (
                            self.grid_form_address(&report_owner, slot, &report_target),
                            weight,
                        )
                    })
                    .collect(),
            });
        }
        control.check()?;
        Ok(BindingGridPlan {
            control: control.clone(),
            report,
            snapshot: self.snapshot_identity.clone(),
            fragment,
            cost,
            guarantee: lattice.guarantee,
            affected,
        })
    }
    pub fn apply_binding_grid_plan(&mut self, plan: &BindingGridPlan) -> EditResult {
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
            return self.failed(Status::error("STALE_PLAN", "Replan the grid"));
        }
        let changed = match &plan.fragment {
            GridFragment::Delete { id, scene } => {
                if *scene {
                    self.content.scene_bindings.remove(id);
                    self.content.scene_binding_order.retain(|x| x != id);
                } else {
                    self.content.bindings.remove(id);
                    self.content.binding_order.retain(|x| x != id);
                }
                true
            }
            GridFragment::Mesh {
                binding,
                base,
                create,
            } => {
                if !*create
                    && self.get_binding(&binding.id) == Some(binding)
                    && base
                        .as_ref()
                        .is_none_or(|m| self.get_mesh(&m.id) == Some(m))
                {
                    false
                } else {
                    self.content
                        .bindings
                        .insert(binding.id.clone(), binding.clone());
                    if *create {
                        self.content.binding_order.push(binding.id.clone());
                    }
                    if let Some(m) = base {
                        self.content.meshes.insert(m.id.clone(), m.clone());
                    }
                    true
                }
            }
            GridFragment::Scene {
                binding,
                transform,
                part,
                offscreen,
                create,
            } => {
                if !*create
                    && self.get_scene_binding(&binding.id) == Some(binding)
                    && transform
                        .as_ref()
                        .is_none_or(|t| self.get_transform(&t.id) == Some(t))
                    && part
                        .as_ref()
                        .is_none_or(|p| self.get_part(&p.id) == Some(p))
                    && offscreen
                        .as_ref()
                        .is_none_or(|o| self.get_offscreen(&o.id) == Some(o))
                {
                    false
                } else {
                    self.content
                        .scene_bindings
                        .insert(binding.id.clone(), binding.clone());
                    if *create {
                        self.content.scene_binding_order.push(binding.id.clone());
                    }
                    if let Some(t) = transform {
                        self.content.transforms.insert(t.id.clone(), t.clone());
                    }
                    if let Some(p) = part {
                        self.content.parts.insert(p.id.clone(), p.clone());
                    }
                    if let Some(o) = offscreen {
                        self.content.offscreens.insert(o.id.clone(), o.clone());
                    }
                    true
                }
            }
            GridFragment::Glue(g) => {
                if self.get_glue(&g.id) == Some(g) {
                    false
                } else {
                    self.content.glues.insert(g.id.clone(), g.clone());
                    true
                }
            }
        };
        if !changed {
            return self.failed(Status::ok());
        }
        self.changed(
            ChangeKind::Structure,
            self.content.mesh_order.clone(),
            plan.affected.clone(),
        )
    }
}
