use super::*;
use crate::model_validation::organization::{Delta, OrganizationChanges};

#[derive(Debug, Clone)]
pub struct OrganizationOutcome {
    pub affected: Vec<String>,
    pub copied_ids: BTreeMap<String, String>,
    pub effects: OrganizationEffects,
}
impl DocumentState {
    pub(super) fn organization_index(
        &self,
    ) -> &crate::model_validation::organization::OrganizationIndex {
        self.read_cache
            .organization
            .get_or_init(|| self.content.model().shared_organization_index())
    }
    pub fn organization_collection(
        &self,
        collection: &OrganizationCollection,
    ) -> Result<Vec<String>, Status> {
        self.content.model().organization_collection(collection)
    }
    pub fn organization_subtree(&self, ids: &[String]) -> Result<Vec<String>, Status> {
        self.content.model().organization_subtree(ids)
    }
    pub fn ordered_parameter_ids(&self) -> Vec<String> {
        self.organization_index().parameter_ids().to_vec()
    }
    pub fn resolved_drawing_groups(&self) -> Vec<DrawOrderGroup> {
        self.content.model().resolved_drawing_groups()
    }
}
impl DocumentBuilder {
    pub fn edit_organization(
        &mut self,
        edit: OrganizationEdit,
    ) -> Result<OrganizationOutcome, Status> {
        let mut result = None;
        self.write(|this| {
            let model = this.content.model();
            if let OrganizationEdit::Duplicate { ids, .. } = &edit {
                let selected = model.organization_subtree(ids)?;
                ConversionLimits::default().check_conversion(
                    selected.len(),
                    super::cost::scene_payload(&model, &selected.into_iter().collect()),
                )?;
            }
            let changes = model.plan_organization_edit(edit)?;
            result = Some(this.install_organization(changes)?);
            Ok(())
        })?;
        Ok(result.unwrap())
    }
    fn install_organization(
        &mut self,
        changes: OrganizationChanges,
    ) -> Result<OrganizationOutcome, Status> {
        self.install_scene_delta(changes.delta)?;
        Ok(OrganizationOutcome {
            affected: changes.affected,
            copied_ids: changes.copied,
            effects: changes.effects,
        })
    }
    pub(super) fn install_scene_delta(&mut self, delta: Delta) -> Result<(), Status> {
        self.scene_initializations.extend(
            delta
                .transforms
                .iter()
                .filter(|t| self.transform(&t.id).is_none())
                .map(|t| t.id.clone())
                .collect::<Vec<_>>(),
        );
        let mut removed = delta.removed.clone();
        removed.extend(
            self.content
                .model()
                .binding_identities()
                .iter()
                .filter(|v| delta.removed.contains(&v.owner))
                .map(|v| v.id.clone()),
        );
        self.clear_owned_editor_objects(&removed);
        let mut editor = self.editor_state().clone();
        for id in &removed {
            editor.rotation_bone_lengths.remove(id);
            editor.rotation_bone_reference_scales.remove(id);
            editor.rotation_bone_angle_offsets.remove(id);
        }
        for (source, target) in &delta.rotation_bone_copies {
            for values in [
                &mut editor.rotation_bone_lengths,
                &mut editor.rotation_bone_reference_scales,
                &mut editor.rotation_bone_angle_offsets,
            ] {
                if let Some(value) = values.get(source).copied() {
                    values.insert(target.clone(), value);
                }
            }
        }
        self.set_editor_state(editor)?;
        macro_rules! remove {
            ($get:ident, $remove:ident) => {
                for id in &delta.removed {
                    if self.$get(id).is_some() {
                        self.$remove(id)?;
                    }
                }
            };
        }
        remove!(parameter, remove_parameter);
        remove!(part, remove_part);
        remove!(mesh, remove_mesh);
        remove!(transform, remove_transform);
        remove!(offscreen, remove_offscreen);
        remove!(binding, remove_binding);
        remove!(scene_binding, remove_scene_binding);
        remove!(blend_binding, remove_blend_binding);
        remove!(glue, remove_glue);
        macro_rules! put {
            ($values:expr, $get:ident, $create:ident, $replace:ident) => {
                for value in $values {
                    if self.$get(&value.id).is_some() {
                        self.$replace(value)?;
                    } else {
                        self.$create(value)?;
                    }
                }
            };
        }
        put!(
            delta.parameters,
            parameter,
            create_parameter,
            replace_parameter
        );
        put!(delta.parts, part, create_part, replace_part);
        put!(delta.meshes, mesh, create_mesh, replace_mesh);
        put!(
            delta.transforms,
            transform,
            create_transform,
            replace_transform
        );
        put!(
            delta.offscreens,
            offscreen,
            create_offscreen,
            replace_offscreen
        );
        put!(delta.bindings, binding, create_binding, replace_binding);
        put!(
            delta.scene_bindings,
            scene_binding,
            create_scene_binding,
            replace_scene_binding
        );
        put!(
            delta.blends,
            blend_binding,
            create_blend_binding,
            replace_blend_binding
        );
        put!(delta.glues, glue, create_glue, replace_glue);
        for (id, parent) in delta.parents {
            if let Some(p) = self.part(&id) {
                let mut p = p.clone();
                p.parent_id = parent;
                self.replace_part(p)?;
            } else if let Some(m) = self.mesh(&id) {
                let mut m = m.clone();
                m.part_id = parent;
                self.replace_mesh(m)?;
            } else if let Some(t) = self.transform(&id) {
                let mut t = t.clone();
                t.part_id = (!parent.is_empty()).then(|| parent.into());
                self.replace_transform(t)?;
            } else if let Some(o) = self.offscreen(&id) {
                let mut o = o.clone();
                o.part_id = parent;
                self.replace_offscreen(o)?;
            }
        }
        if let Some(v) = delta.groups {
            self.set_display_info(v)?;
        }
        if let Some(v) = delta.hierarchy {
            self.set_hierarchy_order(v)?;
        }
        if let Some(v) = delta.drawing {
            self.set_draw_order_groups(Some(v))?;
        }
        if let Some(v) = delta.model3 {
            self.set_model3_settings(v)?;
        }
        Ok(())
    }
    pub fn replace_display_info(&mut self, info: DisplayInfo) -> Result<(), Status> {
        self.write(|this| {
            this.content
                .model()
                .validate_organization_metadata_unlocked(&info)?;
            this.content
                .model()
                .validate_reference_replacement_unlocked(this.model3_settings(), &info)?;
            let references = this
                .content
                .model()
                .reference_identities()
                .iter()
                .map(|v| v.id.clone())
                .collect::<Vec<_>>();
            let removed = this
                .display_info()
                .parameter_groups
                .iter()
                .flatten()
                .filter(|old| {
                    !info
                        .parameter_groups
                        .iter()
                        .flatten()
                        .any(|new| old.id == new.id)
                })
                .map(|g| g.id.clone())
                .collect::<Vec<_>>();
            this.clear_owned_editor_objects(&removed);
            this.set_display_info(info)?;
            let model = this.content.model();
            let retired = references
                .into_iter()
                .filter(|id| model.reference_identity(id).is_none())
                .collect::<Vec<_>>();
            this.clear_owned_editor_objects(&retired);
            Ok(())
        })
    }
}
