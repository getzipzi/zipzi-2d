use super::*;
#[derive(Debug, Clone)]
pub struct SceneContentOutcome {
    pub affected: Vec<String>,
    pub report: SceneContentReport,
    pub work: ConversionWork,
}
impl DocumentState {
    pub fn scene_content_value(
        &self,
        owner: &str,
        target: &SceneContentTarget,
    ) -> Result<SceneContentValue, Status> {
        self.content.model().scene_content_value(owner, target)
    }
}
impl DocumentBuilder {
    pub fn edit_scene_content(
        &mut self,
        edit: SceneContentEdit,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<SceneContentOutcome, Status> {
        let mut result = None;
        self.write(|this| {
            let changes = this
                .content
                .model()
                .plan_scene_content_with_control(edit, limits, control)?;
            result = Some(SceneContentOutcome {
                affected: changes.domain.affected.clone(),
                report: changes.report.clone(),
                work: changes.cost,
            });
            this.install_scene_delta(changes.domain.delta)?;
            control.check()
        })?;
        Ok(result.unwrap())
    }
}

impl DocumentBuilder {
    pub fn patch_scene_content(
        &mut self,
        owner: &str,
        target: &SceneContentTarget,
        expected: SceneContentValue,
        value: SceneContentValue,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<SceneContentOutcome, Status> {
        let mut result = None;
        self.write(|this| {
            let model = this.content.model();
            if model.scene_content_value(owner, target)? != expected {
                return Err(Status::error("STALE_FIELD", owner));
            }
            let edit = model.scene_content_patch(owner, target, value)?;
            result = Some(this.edit_scene_content(edit, limits, control)?);
            Ok(())
        })?;
        Ok(result.unwrap())
    }
}
