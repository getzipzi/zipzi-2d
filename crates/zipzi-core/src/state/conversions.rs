use super::*;
use crate::model_validation::conversion::{
    ordinary::GridFragment,
    range::{map_value, GridOwner},
};
use crate::BindingAxisEditing;

impl DocumentBuilder {
    fn binding_metadata_ids(&self) -> Vec<String> {
        self.content
            .model()
            .binding_identities()
            .iter()
            .map(|i| i.id.clone())
            .collect()
    }
    fn clean_conversion_metadata(&mut self, before: Vec<String>) {
        let removed = {
            let model = self.content.model();
            before
                .into_iter()
                .filter(|id| !model.contains_id(id))
                .collect::<Vec<_>>()
        };
        self.clear_owned_editor_objects(&removed);
    }
    /// Compute and install a complete grid change inside this candidate.
    /// A report is descriptive; it is never a separately applicable mutation plan.
    pub fn edit_binding_grid(
        &mut self,
        edit: BindingGridEdit,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<ConversionOutcome, Status> {
        let mut report = None;
        self.write(|this| {
            let changes = this
                .content
                .model()
                .plan_binding_grid_with_control(edit, limits, control)?;
            report = Some(ConversionOutcome {
                report: changes.report().clone(),
                work: changes.cost(),
            });
            this.install_grid_changes(changes)?;
            control.check()
        })?;
        Ok(report.unwrap())
    }
    pub fn edit_blend_shape(
        &mut self,
        edit: BlendAuthoringEdit,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<ConversionOutcome, Status> {
        let mut report = None;
        self.write(|this| {
            let changes = this
                .content
                .model()
                .plan_blend_authoring_with_control(edit, limits, control)?;
            report = Some(ConversionOutcome {
                report: changes.report().clone(),
                work: changes.cost(),
            });
            this.install_blend_changes(changes)?;
            control.check()
        })?;
        Ok(report.unwrap())
    }
    pub fn edit_parameter_range(
        &mut self,
        edit: ParameterRangeEdit,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<ConversionOutcome, Status> {
        let mut report = None;
        self.write(|this| {
            let plan = this
                .content
                .model()
                .plan_parameter_range_with_control(edit, limits, control)?;
            report = Some(ConversionOutcome {
                report: plan.report.clone(),
                work: plan.cost(),
            });
            let (minimum, maximum) = (plan.parameter.minimum, plan.parameter.maximum);
            let map = |v: &mut f32| {
                *v = map_value(*v, plan.old_min, plan.old_max, minimum, maximum);
            };
            this.replace_parameter(plan.parameter)?;
            for a in plan.axes {
                match a.owner {
                    GridOwner::Mesh => {
                        let mut b = this.binding(&a.id).unwrap().clone();
                        let dimension = b.axes[..a.axis]
                            .iter()
                            .filter(|a| !a.parameter_id.is_empty())
                            .count();
                        b.axes[a.axis].set_values(a.keys);
                        for f in &mut b.keyforms {
                            map(&mut f.keys[dimension]);
                        }
                        this.replace_binding(b)?;
                    }
                    GridOwner::Scene => {
                        let mut b = this.scene_binding(&a.id).unwrap().clone();
                        let dimension = b.axes[..a.axis]
                            .iter()
                            .filter(|a| !a.parameter_id.is_empty())
                            .count();
                        b.axes[a.axis].set_values(a.keys);
                        match &mut b.track {
                            crate::SceneTrack::Warp { keyforms, .. } => {
                                for f in keyforms {
                                    map(&mut f.keys[dimension]);
                                }
                            }
                            crate::SceneTrack::Rotation { keyforms, .. } => {
                                for f in keyforms {
                                    map(&mut f.keys[dimension]);
                                }
                            }
                            crate::SceneTrack::Part { keyforms, .. } => {
                                for f in keyforms {
                                    map(&mut f.keys[dimension]);
                                }
                            }
                        }
                        this.replace_scene_binding(b)?;
                    }
                    GridOwner::Glue => {
                        let mut g = this.glue(&a.id).unwrap().clone();
                        g.binding.as_mut().unwrap().axes[a.axis].set_values(a.keys);
                        this.replace_glue(g)?;
                    }
                }
            }
            for (id, keys) in plan.tables {
                let mut t = this.blend_key_table(&id).unwrap().clone();
                t.keys = keys.clone();
                for (s, value) in t.samples.iter_mut().zip(keys) {
                    s.value = value;
                }
                this.replace_blend_key_table(t)?;
            }
            for (id, keys) in plan.constraints {
                let mut c = this.blend_constraint(&id).unwrap().clone();
                c.keys = keys.clone();
                for (s, value) in c.samples.iter_mut().zip(keys) {
                    s.value = value;
                }
                this.replace_blend_constraint(c)?;
            }
            control.check()
        })?;
        Ok(report.unwrap())
    }
}

impl DocumentBuilder {
    pub(super) fn install_grid_changes(
        &mut self,
        changes: crate::model_validation::conversion::ordinary::GridChanges,
    ) -> Result<(), Status> {
        let mut before = self.binding_metadata_ids();
        match changes.fragment {
            GridFragment::Delete { id, scene } => {
                before.push(id.clone());
                if scene {
                    self.remove_scene_binding(&id)?;
                } else {
                    self.remove_binding(&id)?;
                }
            }
            GridFragment::Mesh {
                binding,
                base,
                create,
            } => {
                if create {
                    self.create_binding(binding)?;
                } else {
                    self.replace_binding(binding)?;
                }
                if let Some(base) = base {
                    self.replace_mesh(base)?;
                }
            }
            GridFragment::Scene {
                binding,
                transform,
                part,
                offscreen,
                create,
            } => {
                if create {
                    self.create_scene_binding(binding)?;
                } else {
                    self.replace_scene_binding(binding)?;
                }
                if let Some(v) = transform {
                    self.replace_transform(v)?;
                }
                if let Some(v) = part {
                    self.replace_part(v)?;
                }
                if let Some(v) = offscreen {
                    self.replace_offscreen(v)?;
                }
            }
            GridFragment::Glue(g) => self.replace_glue(g)?,
        }
        self.clean_conversion_metadata(before);
        Ok(())
    }
    pub(super) fn install_blend_changes(
        &mut self,
        changes: crate::model_validation::conversion::blend::BlendChanges,
    ) -> Result<(), Status> {
        let before = self.binding_metadata_ids();
        let w = changes.writes;
        macro_rules! put {
            ($values:expr, $get:ident, $create:ident, $replace:ident) => {
                for v in $values {
                    if self.$get(&v.id).is_some() {
                        self.$replace(v)?;
                    } else {
                        self.$create(v)?;
                    }
                }
            };
        }
        put!(
            w.tables,
            blend_key_table,
            create_blend_key_table,
            replace_blend_key_table
        );
        put!(
            w.constraints,
            blend_constraint,
            create_blend_constraint,
            replace_blend_constraint
        );
        put!(
            w.bindings,
            blend_binding,
            create_blend_binding,
            replace_blend_binding
        );
        put!(w.meshes, mesh, create_mesh, replace_mesh);
        put!(w.mesh_bindings, binding, create_binding, replace_binding);
        put!(w.transforms, transform, create_transform, replace_transform);
        put!(
            w.scene_bindings,
            scene_binding,
            create_scene_binding,
            replace_scene_binding
        );
        put!(w.parts, part, create_part, replace_part);
        put!(w.glues, glue, create_glue, replace_glue);
        put!(w.offscreens, offscreen, create_offscreen, replace_offscreen);
        self.clean_conversion_metadata(before);
        Ok(())
    }
}
