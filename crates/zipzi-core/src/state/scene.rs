use super::*;
#[derive(Debug, Clone)]
pub struct SceneOutcome {
    pub affected: Vec<String>,
    pub copied_ids: BTreeMap<String, String>,
    pub effects: OrganizationEffects,
    pub conversion: Option<ConversionOutcome>,
    pub work: Option<ConversionWork>,
}
impl DocumentState {
    pub fn scene_collection(&self, collection: &SceneCollection) -> Result<Vec<String>, Status> {
        self.content.model().scene_collection(collection)
    }
    pub fn scene_field(&self, id: &str, field: SceneField) -> Result<SceneFieldValue, Status> {
        self.content.model().scene_field(id, field)
    }
    pub fn reparent_support(&self, edit: &ReparentEdit) -> Result<ReparentSupport, Status> {
        self.content.model().reparent_support(edit)
    }
}
impl DocumentBuilder {
    pub fn edit_scene(
        &mut self,
        edit: SceneEdit,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<SceneOutcome, Status> {
        let mut outcome = None;
        self.write(|this| {
            let changes = this
                .content
                .model()
                .plan_scene_edit_with_control(edit, limits, control)?;
            outcome = Some(SceneOutcome {
                affected: changes.domain.affected.clone(),
                copied_ids: changes.domain.copied.clone(),
                effects: changes.domain.effects.clone(),
                work: changes
                    .cost
                    .or_else(|| changes.reparent.as_ref().map(|p| p.cost())),
                conversion: changes.reparent.as_ref().map(|p| ConversionOutcome {
                    report: p.report().clone(),
                    work: p.cost(),
                }),
            });
            this.install_scene_delta(changes.domain.delta)?;
            if let Some(reparent) = changes.reparent {
                this.install_scene_delta(reparent.delta())?;
            }
            for (id, value, clear) in changes.patches {
                let delta = this.content.model().scene_field_delta(&id, &value, clear);
                this.install_scene_delta(delta)?;
            }
            control.check()
        })?;
        Ok(outcome.unwrap())
    }
}

impl DocumentBuilder {
    pub fn group_deformer(
        &mut self,
        create: SceneEdit,
        sources: Vec<String>,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<SceneOutcome, Status> {
        let mut outcome = None;
        self.write(|this| {
            let changes = this
                .content
                .model()
                .plan_deformer_group(create, sources, limits, control)?;
            outcome = Some(SceneOutcome {
                affected: changes.domain.affected.clone(),
                copied_ids: changes.domain.copied.clone(),
                effects: changes.domain.effects.clone(),
                conversion: None,
                work: changes.cost,
            });
            this.install_scene_delta(changes.domain.delta)?;
            control.check()
        })?;
        Ok(outcome.unwrap())
    }
}
