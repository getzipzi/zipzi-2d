//! Bounded read-only modeling routes shared by native and IPC adapters.
use super::*;
use crate::authoring::{
    MeshBindingBatch, MeshBindingQuery, ModelingSampleBatch, ModelingSampleQuery,
};

impl Runtime {
    /// Optional quality DTOs, default-pose reference and bounded alpha raster workspace.
    pub(super) fn quality_bytes(doc: &Document, poses: usize) -> usize {
        doc.mesh_order()
            .iter()
            .filter_map(|id| doc.get_mesh(id))
            .fold(1 << 20, |n, m| {
                n.saturating_add(m.triangles.len().saturating_mul(1024).saturating_mul(poses))
                    .saturating_add(
                        m.base_positions
                            .len()
                            .saturating_mul(128)
                            .saturating_mul(poses.max(1)),
                    )
            })
    }
    /// Bound frozen evaluator workspace, typed output, diagnostic worst case and
    /// Python objects before any frame or source clone is allocated.
    pub(super) fn sample_bytes(doc: &Document, poses: usize) -> usize {
        let geometry = doc
            .mesh_order()
            .iter()
            .filter_map(|id| doc.get_mesh(id))
            .fold(4096usize, |n, m| {
                n.saturating_add(m.base_positions.len().saturating_mul(512))
                    .saturating_add(m.triangles.len().saturating_mul(384))
                    .saturating_add(m.masks.len().saturating_mul(256))
            });
        doc.estimated_content_bytes()
            .saturating_mul(8)
            .saturating_add(geometry.saturating_mul(poses.max(1)))
    }

    pub fn binding_read_bytes(&self, client: ClientId, input: &MeshBindingQuery) -> Result<usize> {
        use zipzi_core::binary_wire::WireWrite;
        self.with_authoring_source(client, input.session, input.edit, |doc, _| {
            let ids = input.ids.as_deref().unwrap_or_else(|| doc.binding_order());
            if ids.len() > 256 {
                return Err(RuntimeError::Budget("mesh binding count"));
            }
            let mut bytes = 4096usize;
            for id in ids {
                let binding = doc.get_binding(id).ok_or_else(|| {
                    RuntimeError::Sdk(zipzi_sdk::SdkError {
                        code: "MISSING_BINDING".into(),
                        message: "Mesh binding does not exist".into(),
                        operation: "bindings.get",
                        object_ids: vec![id.clone()],
                        field_path: None,
                        expected_version: None,
                        actual_version: None,
                        referrers: Box::new([]),
                    })
                })?;
                bytes = bytes.saturating_add(binding.expansion_bytes().saturating_mul(8));
            }
            Ok(bytes)
        })
    }
    pub fn mesh_binding_snapshot(
        &self,
        client: ClientId,
        input: MeshBindingQuery,
    ) -> Result<MeshBindingBatch> {
        self.check_bytes(self.binding_read_bytes(client, &input)?, 0, 0)?;
        self.with_authoring_source(client, input.session, input.edit, |doc, source| {
            let ids = input.ids.as_deref().unwrap_or_else(|| doc.binding_order());
            Ok(MeshBindingBatch {
                source,
                bindings: ids
                    .iter()
                    .map(|id| doc.get_binding(id).unwrap().clone())
                    .collect(),
            })
        })
    }
    pub fn binding_response(
        &mut self,
        client: ClientId,
        input: MeshBindingQuery,
    ) -> Result<serde_json::Value> {
        use zipzi_core::binary_wire::WireWrite;
        let _lease = self.reserve_transfer(self.binding_read_bytes(client, &input)?)?;
        let value = self.mesh_binding_snapshot(client, input)?;
        let size = zipzi_core::binary_wire::encoded_len(&value.bindings)
            .map_err(|_| RuntimeError::Budget("binary bindings"))?;
        if size <= 8192 {
            return serde_json::to_value(value).map_err(|_| RuntimeError::InvalidReference);
        }
        let (blob, length) = self.binary_result_blob(client, &value.bindings)?;
        Ok(
            serde_json::json!({"source":value.source,"blob":blob,"length":length,"signature":format!("{:032x}",Vec::<zipzi_core::MeshBinding>::SIGNATURE)}),
        )
    }

    pub fn sample_modeling(
        &self,
        client: ClientId,
        input: ModelingSampleQuery,
    ) -> Result<ModelingSampleBatch> {
        if input.parameters.len() > 64 || input.parameters.iter().any(|p| p.len() > 256) {
            return Err(RuntimeError::Budget("modeling samples"));
        }
        // All input/source/output admission precedes the frozen document clone
        // and evaluation. No model, Preview or history is mutated by this route.
        let (source, lease) =
            self.with_authoring_source(client, input.session, input.edit, |doc, source| {
                if input.expected.is_some_and(|expected| expected != source) {
                    return Err(RuntimeError::InvalidReference);
                }
                self.refresh_memory_budget();
                let lease = self
                    .reserve_transfer(
                        Self::sample_bytes(doc, input.parameters.len()).saturating_add(
                            if input.checks.quality {
                                Self::quality_bytes(doc, input.parameters.len())
                            } else {
                                0
                            },
                        ),
                    )
                    .map_err(|_| RuntimeError::Budget("modeling sample workspace"))?;
                Ok((source, lease))
            })?;
        let snapshot = if let Some(edit) = input.edit {
            self.edit_snapshot(client, edit)?
        } else {
            self.source_snapshot(input.session)?
        };
        let state = match snapshot.state {
            SessionState::Ready => "ready",
            SessionState::Busy(_) => "busy",
            SessionState::Faulted(_) => "faulted",
            SessionState::Closing => "closing",
            SessionState::Closed => "closed",
        };
        Ok(ModelingSampleBatch {
            lease: Some(Arc::new(lease)),
            data: zipzi_contract::runtime::ModelingSampleData {
                source,
                state,
                samples: snapshot
                    .snapshot
                    .sample_modeling(input.parameters, input.checks)?,
            },
        })
    }
}
