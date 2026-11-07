use super::*;
use zipzi_contract::authoring::{MeshBindingCreateSpec, MeshKeyformSpec};
use zipzi_core::{
    Appearance, BindingAxis, BindingAxisEditing, MeshBinding, MeshKeyform, SceneBinding,
    SceneKeyform,
};

impl Commands<'_> {
    pub fn create_binding(&mut self, mut binding: MeshBinding) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(&binding.mesh_id)?;
            this.builder.canonicalize_binding(&mut binding)?;
            this.builder.create_binding(binding)
        })
    }
    pub fn replace_binding(&mut self, mut binding: MeshBinding) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(&binding.id)?;
            this.unlocked(&binding.mesh_id)?;
            this.builder.canonicalize_binding(&mut binding)?;
            this.builder.replace_binding(binding)
        })
    }
    pub fn set_mesh_keyform(&mut self, id: &str, form: MeshKeyform) -> Result<(), Status> {
        self.run(|this| this.builder.set_mesh_keyform(id, form))
    }
    pub fn set_mesh_keyform_spec(&mut self, id: &str, form: MeshKeyformSpec) -> Result<(), Status> {
        self.set_mesh_keyform(id, form.into_keyform())
    }
    pub fn create_scene_binding(&mut self, mut binding: SceneBinding) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(binding.target_id())?;
            this.builder.canonicalize_scene_binding(&mut binding)?;
            this.builder.create_scene_binding(binding)
        })
    }
    pub fn replace_scene_binding(&mut self, mut binding: SceneBinding) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(&binding.id)?;
            this.unlocked(binding.target_id())?;
            this.builder.canonicalize_scene_binding(&mut binding)?;
            this.builder.replace_scene_binding(binding)
        })
    }
    pub fn set_scene_keyform(&mut self, id: &str, form: SceneKeyform) -> Result<(), Status> {
        self.run(|this| this.builder.set_scene_keyform(id, form))
    }
    pub fn create_mesh_binding_spec(
        &mut self,
        spec: MeshBindingCreateSpec,
    ) -> Result<String, Status> {
        let id = spec.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.create_binding(MeshBinding {
            id: id.clone(),
            mesh_id: spec.mesh_id,
            axes: spec
                .axes
                .into_iter()
                .map(|axis| BindingAxis::new(axis.parameter_id, axis.keys))
                .collect(),
            keyforms: spec
                .keyforms
                .into_iter()
                .map(MeshKeyformSpec::into_keyform)
                .collect(),
        })?;
        Ok(id)
    }
}

trait MeshKeyformSpecApply {
    fn into_keyform(self) -> MeshKeyform;
}
impl MeshKeyformSpecApply for MeshKeyformSpec {
    fn into_keyform(self) -> MeshKeyform {
        let mut appearance = Appearance::default();
        if let Some(value) = self.opacity {
            appearance.opacity = value;
        }
        if let Some(value) = self.multiply {
            appearance.multiply = value;
        }
        if let Some(value) = self.screen {
            appearance.screen = value;
        }
        MeshKeyform {
            keys: self.keys,
            samples: vec![],
            positions: self.positions,
            appearance,
            draw_order: self.draw_order,
        }
    }
}

impl Commands<'_> {
    pub fn edit_binding_grid(
        &mut self,
        edit: zipzi_core::document::BindingGridEdit,
        limits: zipzi_core::state::ConversionLimits,
        control: &zipzi_core::state::ConversionControl,
    ) -> Result<zipzi_core::state::ConversionOutcome, Status> {
        self.run(|this| this.builder.edit_binding_grid(edit, limits, control))
    }
    pub fn edit_blend_shape(
        &mut self,
        edit: zipzi_core::document::BlendAuthoringEdit,
        limits: zipzi_core::state::ConversionLimits,
        control: &zipzi_core::state::ConversionControl,
    ) -> Result<zipzi_core::state::ConversionOutcome, Status> {
        self.run(|this| this.builder.edit_blend_shape(edit, limits, control))
    }
    pub fn edit_parameter_range(
        &mut self,
        edit: zipzi_core::document::ParameterRangeEdit,
        limits: zipzi_core::state::ConversionLimits,
        control: &zipzi_core::state::ConversionControl,
    ) -> Result<zipzi_core::state::ConversionOutcome, Status> {
        self.run(|this| this.builder.edit_parameter_range(edit, limits, control))
    }
}

impl Commands<'_> {
    pub fn replace_blend_binding(
        &mut self,
        binding: zipzi_core::BlendShapeBinding,
    ) -> Result<(), Status> {
        self.run(|this| {
            this.unlocked(&binding.id)?;
            this.unlocked(&binding.target_id)?;
            this.builder.replace_blend_binding(binding)
        })
    }
}
