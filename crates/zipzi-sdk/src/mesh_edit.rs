//! Selected mesh authoring inputs. Defaults and identity allocation live in SDK,
//! not in a language bridge or a serialization of the persisted Mesh record.
use crate::{EditSession, MeshGeometry, SdkError};
use serde::{Deserialize, Serialize};
use zipzi_core::{Mesh, Vec2};

zipzi_core::api_record! {
    #[derive(Debug, Clone, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub struct MeshCreateSpec {
        /// Human-readable display name; use the stable ID for identity because names can change.
        pub name: String,
        /// Owned vertex IDs, positions, UVs and triangles; triangles contain vertex IDs, not
        /// position offsets.
        pub geometry: MeshGeometry,
        #[serde(default)]
        /// Image-asset ID used by this mesh; the default empty string leaves its texture
        /// reference unassigned.
        pub texture_asset_id: String,
        /// Optional new mesh UUID; None allocates a fresh UUID in the SDK when the create
        /// operation executes.
        pub id: Option<String>,
    }
}
zipzi_core::api_union! {
    tag = "op";
    #[derive(Debug, Clone, Serialize, Deserialize)]
#[derive(zipzi_api_derive::BinaryWire)]
    pub enum MeshOperation {
        Create {
            /// Creation specification; native admission supplies identities/defaults and
            /// rejects invalid input.
            spec: MeshCreateSpec,
        },
        Rename {
            /// Stable document object ID used by references; it is distinct from the exported
            /// runtime_id.
            id: String,
            /// Human-readable display name; use the stable ID for identity because names can
            /// change.
            name: String,
            /// Required current display name; None disables this name precondition.
            expected_name: Option<String>,
        },
        SetPositions {
            /// Stable document object ID used by references; it is distinct from the exported
            /// runtime_id.
            id: String,
            /// Stable vertex IDs selecting which positions to replace; values are paired with
            /// positions in the supplied order.
            vertex_ids: Vec<u32>,
            /// Replacement absolute mesh-local XY positions aligned with the supplied
            /// vertex_ids selection.
            positions: Vec<Vec2>,
        },
    }
}

impl MeshOperation {
    pub fn apply(self, edit: &mut EditSession<'_>) -> Result<String, SdkError> {
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
