//! Selected mesh authoring inputs. Defaults and identity allocation live in SDK,
//! not in a language bridge or a serialization of the persisted Mesh record.
use crate::{EditSession, SdkError};
use zipzi_core::Mesh;

pub use zipzi_contract::authoring::MeshCreateSpec;
pub use zipzi_contract::authoring::MeshOperation;

pub trait MeshOperationApply {
    fn apply(self, edit: &mut EditSession<'_>) -> Result<String, SdkError>;
}
impl MeshOperationApply for MeshOperation {
    fn apply(self, edit: &mut EditSession<'_>) -> Result<String, SdkError> {
        match self {
            Self::Create { spec } => edit.create_mesh_spec(spec),
            Self::Rename {
                id,
                name,
                expected_name,
            } => {
                if let Some(expected) =
                    expected_name.filter(|_| edit.candidate_document().get_mesh(&id).is_some())
                {
                    edit.edit_scene(crate::SceneEdit::Patch {
                        id: id.clone(),
                        expected: crate::SceneFieldValue::Label(expected),
                        value: crate::SceneFieldValue::Label(name),
                        clear_glue_pairs: false,
                    })?;
                } else {
                    edit.rename_mesh(&id, &name)?;
                }
                Ok(id)
            }
            Self::SetPositions {
                id,
                vertex_ids,
                positions,
            } => {
                edit.update_positions(&id, &vertex_ids, &positions)?;
                Ok(id)
            }
        }
    }
}
impl EditSession<'_> {
    pub fn create_mesh_spec(&mut self, spec: MeshCreateSpec) -> Result<String, SdkError> {
        let id = spec.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        self.create_mesh(Mesh {
            id: id.clone(),
            name: spec.name,
            texture_asset_id: spec.texture_asset_id.into(),
            vertex_ids: spec.geometry.vertex_ids,
            base_positions: spec.geometry.positions,
            uvs: spec.geometry.uvs,
            triangles: spec.geometry.triangles,
            meshing: spec.geometry.meshing,
            ..Default::default()
        })?;
        Ok(id)
    }
}
