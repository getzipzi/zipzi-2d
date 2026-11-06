use super::*;
#[derive(Debug, Clone)]
pub struct ReferenceOutcome {
    pub effects: ReferenceEffects,
    pub affected: Vec<String>,
    pub copied_ids: BTreeMap<String, String>,
}
impl DocumentState {
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
