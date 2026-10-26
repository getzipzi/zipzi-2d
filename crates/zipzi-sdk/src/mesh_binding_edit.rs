//! Numeric authoring inputs; canonical axis/sample identities stay in core.
use crate::{EditSession, SdkError};
use serde::{Deserialize, Serialize};
use zipzi_core::{Appearance, BindingAxis, MeshBinding, MeshKeyform, Vec2};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
pub struct MeshBindingAxisSpec {
    /// Stable document parameter ID, not its exported runtime identifier.
    pub parameter_id: String,
    /// Strictly increasing finite parameter sample values for this axis, in parameter units;
    /// not sample indices.
    pub keys: Vec<f32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
pub struct MeshKeyformSpec {
    /// One actual parameter sample value per binding axis, in axis order; each must match a
    /// declared axis sample.
    pub keys: Vec<f32>,
    /// Absolute mesh-local positions, one per target mesh vertex in vertex_ids order; not
    /// displacement vectors.
    pub positions: Vec<Vec2>,
    /// Optional finite nonnegative opacity factor; None uses the native neutral default 1.
    /// Values above 1 are retained for import roundtrips.
    pub opacity: Option<f32>,
    /// Optional RGB multiply factors; None uses neutral (1, 1, 1).
    pub multiply: Option<[f32; 3]>,
    /// Optional RGB screen factors; None uses neutral (0, 0, 0).
    pub screen: Option<[f32; 3]>,
    /// Optional keyform drawing-order coordinate; None leaves the order unspecified for native
    /// evaluation.
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
    /// Stable ID of the target mesh in this document.
    pub mesh_id: String,
    /// Ordered parameter axes; keyform coordinates follow this order and samples retain their
    /// own identities.
    pub axes: Vec<MeshBindingAxisSpec>,
    /// Shape samples for this binding; coordinate/sample identities must match its axes or key
    /// table.
    pub keyforms: Vec<MeshKeyformSpec>,
    /// Optional new binding UUID; None allocates a fresh UUID when the create operation executes.
    pub id: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire, zipzi_api_derive::ApiShape)]
pub enum MeshBindingOperation {
    Create {
        /// Creation specification; native admission supplies identities/defaults and rejects
        /// invalid input.
        spec: MeshBindingCreateSpec,
    },
    SetKeyform {
        /// Stable document object ID used by references; it is distinct from the exported
        /// runtime_id.
        id: String,
        /// Complete shape sample to insert or replace; unrelated samples are retained.
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
