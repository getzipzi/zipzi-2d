use super::*;
#[derive(Debug, Clone)]
pub struct ReferenceOutcome {
    pub effects: ReferenceEffects,
    pub affected: Vec<String>,
    pub copied_ids: BTreeMap<String, String>,
}
impl DocumentState {
    /// Check a small batch of new IDs without indexing every motion key.
    pub(super) fn contains_any_identity(&self, ids: &BTreeSet<&str>) -> bool {
        let model = self.content.model();
        let reserved = ids.iter().any(|id| {
            model.base_identity_set().contains(*id)
                || model.binding_identity(id).is_some()
                || model.reference_identity(id).is_some()
                || model.get_glue_pair(id).is_some()
        });
        let mut motion_ids = self
            .content
            .motions
            .shared_map()
            .values()
            .flat_map(|clip| super::motions::clip_ids(clip));
        reserved
            || match ids.first() {
                Some(id) if ids.len() == 1 => motion_ids.any(|other| other == *id),
                _ => motion_ids.any(|id| ids.contains(id)),
            }
    }

    pub fn reference_selection(
        &self,
        owner: &str,
        field: ReferenceSelectionField,
    ) -> Result<&[String], Status> {
        match field {
            ReferenceSelectionField::MeshMasks => {
                self.mesh(owner).map(|value| value.masks.as_slice())
            }
            ReferenceSelectionField::OffscreenMasks => {
                self.offscreen(owner).map(|value| value.masks.as_slice())
            }
            ReferenceSelectionField::BlendConstraints => self
                .blend_binding(owner)
                .map(|value| value.constraint_ids.as_slice()),
        }
        .ok_or_else(|| Status::error("WRONG_REFERENCE_SELECTION", owner))
    }
    pub fn reference_collection(
        &self,
        collection: &ReferenceCollection,
    ) -> Result<Vec<String>, Status> {
        self.content.model().reference_collection(collection)
    }
    pub fn reference_fields(&self, id: &str) -> Result<ReferenceFields, Status> {
        self.content.model().reference_fields(id)
    }
}
impl DocumentBuilder {
    pub(super) fn validate_combined_parameter_payload(&self) -> Result<bool, Status> {
        fn identities(info: &DisplayInfo) -> impl Iterator<Item = &str> {
            info.combined_parameters.iter().flatten().flat_map(|set| {
                std::iter::once(set.id.as_str())
                    .chain(set.members.iter().map(|member| member.id.as_str()))
            })
        }

        let old = self.base.display_info();
        let new = self.display_info();
        let mut previous = old.clone();
        previous
            .combined_parameters
            .clone_from(&new.combined_parameters);
        if previous != *new {
            return Ok(false);
        }
        let old_ids: BTreeSet<_> = identities(old).collect();
        let mut new_ids = BTreeSet::new();
        if identities(new).any(|id| !valid_uuid(id) || !new_ids.insert(id))
            || old_ids
                .difference(&new_ids)
                .any(|id| self.editor_state().objects.contains_key(*id))
        {
            return Ok(false);
        }
        let added: BTreeSet<_> = new_ids.difference(&old_ids).copied().collect();
        if !added.is_empty() && self.base.contains_any_identity(&added) {
            return Ok(false);
        }
        if let Some(issue) = self
            .content
            .model()
            .validate_display_info()
            .into_iter()
            .next()
        {
            return Err(issue.status);
        }
        Ok(true)
    }

    pub fn edit_reference(&mut self, edit: ReferenceEdit) -> Result<ReferenceOutcome, Status> {
        let mut outcome = None;
        self.write(|this| {
            let changes = this.content.model().plan_reference_edit(edit)?;
            let before = this
                .content
                .model()
                .reference_identities()
                .iter()
                .map(|v| v.id.clone())
                .collect::<Vec<_>>();
            if let Some((id, fields)) = &changes.patch {
                let delta = this.content.model().reference_fields_delta(id, fields);
                this.install_scene_delta(delta)?;
            } else {
                this.install_scene_delta(changes.domain.delta)?;
            }
            let model = this.content.model();
            let removed = before
                .into_iter()
                .filter(|id| model.reference_identity(id).is_none())
                .collect::<Vec<_>>();
            this.clear_owned_editor_objects(&removed);
            outcome = Some(ReferenceOutcome {
                effects: changes.effects,
                affected: changes.domain.affected,
                copied_ids: changes.domain.copied,
            });
            Ok(())
        })?;
        Ok(outcome.unwrap())
    }
    pub fn replace_model3_settings(&mut self, settings: Model3Settings) -> Result<(), Status> {
        self.write(|this| {
            let model = this.content.model();
            model.validate_reference_replacement_unlocked(&settings, this.display_info())?;
            let before = model
                .reference_identities()
                .iter()
                .map(|v| v.id.clone())
                .collect::<Vec<_>>();
            this.set_model3_settings(settings)?;
            let model = this.content.model();
            let removed = before
                .into_iter()
                .filter(|id| model.reference_identity(id).is_none())
                .collect::<Vec<_>>();
            this.clear_owned_editor_objects(&removed);
            Ok(())
        })
    }
}

impl DocumentState {
    pub fn reference_projection(&self) -> std::sync::Arc<ReferenceProjection> {
        std::sync::Arc::new(self.content.model().reference_projection())
    }
}
