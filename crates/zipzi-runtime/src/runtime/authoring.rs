use super::*;
use crate::authoring::{MeshSnapshot, MeshWriteResult};

impl Runtime {
    pub fn edit_meshes(
        &mut self,
        client: ClientId,
        edit: EditId,
        operations: Vec<zipzi_sdk::MeshOperation>,
    ) -> Result<MeshWriteResult> {
        self.edit(client, edit)?;
        if operations.len() > 1024 {
            return Err(RuntimeError::Budget("mesh operations"));
        }
        let ids = self.apply_edit(client, edit, |candidate| {
            operations
                .into_iter()
                .map(|op| op.apply(candidate))
                .collect::<std::result::Result<Vec<_>, _>>()
        })?;
        let entry = self.edit(client, edit)?;
        Ok(MeshWriteResult {
            ids,
            source: SourceRef::Edit {
                session: entry.session,
                edit,
                base: entry.base,
                sequence: entry.sequence,
            },
        })
    }

    pub fn mesh_snapshot(
        &self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        id: &str,
    ) -> Result<MeshSnapshot> {
        self.client(client)?;
        let frozen;
        let (doc, source) = if let Some(edit) = edit {
            let entry = self.edit(client, edit)?;
            if entry.session != session {
                return Err(RuntimeError::InvalidReference);
            }
            (
                self.edit_document(client, edit)?,
                SourceRef::Edit {
                    session,
                    edit,
                    base: entry.base,
                    sequence: entry.sequence,
                },
            )
        } else {
            let entry = self.entry(session)?;
            let source = SourceRef::Formal {
                session,
                version: entry.info.version,
            };
            if let Some(sdk) = &entry.sdk {
                (sdk.document(), source)
            } else {
                frozen = self.source_snapshot(session)?;
                (frozen.snapshot.document(), frozen.source)
            }
        };
        let mesh = doc.get_mesh(id).ok_or_else(|| {
            RuntimeError::Sdk(zipzi_sdk::SdkError {
                code: "MISSING_MESH".into(),
                message: "Mesh does not exist".into(),
                operation: "meshes.get",
                object_ids: vec![id.into()],
                field_path: None,
                expected_version: None,
                actual_version: None,
                referrers: Box::new([]),
            })
        })?;
        // A conservative bound for owned records AND their possible wire
        // expansion, before cloning. Large queries need later paged/buffer APIs.
        let bytes = mesh
            .vertex_ids
            .len()
            .saturating_mul(128)
            .saturating_add(mesh.triangles.len().saturating_mul(64))
            .saturating_add(mesh.id.len())
            .saturating_add(mesh.name.len())
            .saturating_add(4096);
        if bytes > 1 << 20 {
            return Err(RuntimeError::Budget("mesh snapshot"));
        }
        self.check_bytes(bytes.saturating_mul(128), 0, 0)?;
        Ok(MeshSnapshot {
            source,
            id: mesh.id.clone(),
            name: mesh.name.clone(),
            geometry: zipzi_sdk::MeshGeometry {
                vertex_ids: mesh.vertex_ids.clone(),
                positions: mesh.base_positions.clone(),
                uvs: mesh.uvs.clone(),
                triangles: mesh.triangles.clone(),
            },
        })
    }
}
