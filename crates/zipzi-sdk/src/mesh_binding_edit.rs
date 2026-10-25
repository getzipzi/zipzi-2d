//! Numeric authoring inputs; canonical axis/sample identities stay in core.
use crate::{EditSession, SdkError};
use serde::{Deserialize, Serialize};
use zipzi_core::{Appearance, BindingAxis, MeshBinding, MeshKeyform, Vec2};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
pub struct MeshBindingAxisSpec {
    pub parameter_id: String,
    pub keys: Vec<f32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
pub struct MeshKeyformSpec {
    pub keys: Vec<f32>,
    pub positions: Vec<Vec2>,
    pub opacity: Option<f32>,
    pub multiply: Option<[f32; 3]>,
    pub screen: Option<[f32; 3]>,
    pub draw_order: Option<f32>,
}
impl MeshKeyformSpec {
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
pub struct MeshBindingCreateSpec {
    pub mesh_id: String,
    pub axes: Vec<MeshBindingAxisSpec>,
    pub keyforms: Vec<MeshKeyformSpec>,
    pub id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
pub enum MeshBindingOperation {
    Create {
        spec: MeshBindingCreateSpec,
    },
    SetKeyform {
        id: String,
        keyform: MeshKeyformSpec,
    },
}
impl MeshBindingOperation {
    pub fn apply(self, edit: &mut EditSession<'_>) -> Result<String, SdkError> {
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
