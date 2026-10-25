//! Selected mesh authoring inputs. Defaults and identity allocation live in SDK,
//! not in a language bridge or a serialization of the persisted Mesh record.
use crate::{EditSession, MeshGeometry, SdkError};
use serde::{Deserialize, Serialize};
use zipzi_core::{Mesh, Vec2};

zipzi_core::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct MeshCreateSpec {
        pub name: String,
        pub geometry: MeshGeometry,
        #[serde(default)]
        pub texture_asset_id: String,
        pub id: Option<String>,
    }
}
zipzi_core::api_union! {
    tag = "op";
    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub enum MeshOperation {
        Create { spec: MeshCreateSpec },
        Rename { id: String, name: String },
        SetPositions { id: String, vertex_ids: Vec<u32>, positions: Vec<Vec2> },
    }
}

impl MeshOperation {
    pub fn apply(self, edit: &mut EditSession<'_>) -> Result<String, SdkError> {
        match self {
            Self::Create { spec } => edit.create_mesh_spec(spec),
            Self::Rename { id, name } => {
                edit.rename_mesh(&id, &name)?;
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
            ..Default::default()
        })?;
        Ok(id)
    }
}
