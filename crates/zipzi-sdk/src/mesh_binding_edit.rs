//! Numeric authoring inputs; canonical axis/sample identities stay in core.
use crate::{EditSession, SdkError};
use zipzi_core::BindingAxisEditing;
use zipzi_core::{Appearance, BindingAxis, MeshBinding, MeshKeyform};

pub use zipzi_contract::authoring::{
    MeshBindingAxisSpec, MeshBindingCreateSpec, MeshBindingOperation, MeshKeyformSpec,
};

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
pub trait MeshBindingOperationApply {
    fn apply(self, edit: &mut EditSession<'_>) -> Result<String, SdkError>;
}
impl MeshBindingOperationApply for MeshBindingOperation {
    fn apply(self, edit: &mut EditSession<'_>) -> Result<String, SdkError> {
        match self {
            Self::Create { spec } => edit.create_mesh_binding_spec(spec),
            Self::SetKeyform { id, keyform } => {
                edit.set_mesh_keyform(&id, keyform.into_keyform())?;
                Ok(id)
            }
        }
    }
}
impl EditSession<'_> {
    pub fn create_mesh_binding_spec(
        &mut self,
        spec: MeshBindingCreateSpec,
    ) -> Result<String, SdkError> {
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
