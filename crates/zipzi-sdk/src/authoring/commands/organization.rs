use super::*;
impl Commands<'_> {
    pub fn edit_organization(
        &mut self,
        edit: zipzi_core::document::OrganizationEdit,
    ) -> Result<zipzi_core::state::OrganizationOutcome, Status> {
        self.run(|this| this.builder.edit_organization(edit))
    }
    pub fn replace_display_info(
        &mut self,
        info: zipzi_core::document::DisplayInfo,
    ) -> Result<(), Status> {
        self.run(|this| this.builder.replace_display_info(info))
    }
}

impl Commands<'_> {
    pub fn edit_scene(
        &mut self,
        edit: zipzi_core::document::SceneEdit,
        limits: zipzi_core::state::ConversionLimits,
        control: &zipzi_core::state::ConversionControl,
    ) -> Result<zipzi_core::state::SceneOutcome, Status> {
        self.run(|this| this.builder.edit_scene(edit, limits, control))
    }
}

impl Commands<'_> {
    pub fn edit_scene_content(
        &mut self,
        edit: zipzi_core::document::SceneContentEdit,
        limits: zipzi_core::state::ConversionLimits,
        control: &zipzi_core::state::ConversionControl,
    ) -> Result<zipzi_core::state::SceneContentOutcome, Status> {
        self.run(|this| this.builder.edit_scene_content(edit, limits, control))
    }
}

impl Commands<'_> {
    pub fn edit_reference(
        &mut self,
        edit: zipzi_core::document::ReferenceEdit,
    ) -> Result<zipzi_core::state::ReferenceOutcome, Status> {
        self.run(|this| this.builder.edit_reference(edit))
    }
    pub fn replace_model3_settings(
        &mut self,
        settings: zipzi_core::document::Model3Settings,
    ) -> Result<(), Status> {
        self.run(|this| this.builder.replace_model3_settings(settings))
    }
}

impl Commands<'_> {
    pub fn group_deformer(
        &mut self,
        create: zipzi_core::document::SceneEdit,
        sources: Vec<String>,
        limits: zipzi_core::state::ConversionLimits,
        control: &zipzi_core::state::ConversionControl,
    ) -> Result<zipzi_core::state::SceneOutcome, Status> {
        self.run(|this| {
            this.builder
                .group_deformer(create, sources, limits, control)
        })
    }
}

impl Commands<'_> {
    pub fn review_deletion(
        &mut self,
        request: &zipzi_contract::document::DeletionRequest,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<zipzi_core::state::DeletionOutcome, Status> {
        self.run(|this| this.builder.review_deletion(request, limits, control))
    }
    pub fn edit_deletion(
        &mut self,
        request: &zipzi_core::document::DeletionRequest,
        limits: zipzi_core::state::ConversionLimits,
        control: &zipzi_core::state::ConversionControl,
    ) -> Result<zipzi_core::state::DeletionOutcome, Status> {
        self.run(|this| this.builder.edit_deletion(request, limits, control))
    }
}

impl Commands<'_> {
    pub fn patch_scene_content(
        &mut self,
        owner: &str,
        target: &SceneContentTarget,
        expected: SceneContentValue,
        value: SceneContentValue,
        limits: ConversionLimits,
        control: &ConversionControl,
    ) -> Result<SceneContentOutcome, Status> {
        self.run(|this| {
            this.builder
                .patch_scene_content(owner, target, expected, value, limits, control)
        })
    }
}
