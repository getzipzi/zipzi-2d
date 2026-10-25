//! Bounded read-only modeling routes shared by native and IPC adapters.
use super::*;
use crate::authoring::{
    MeshBindingBatch, MeshBindingQuery, ModelingSampleBatch, ModelingSampleQuery,
};

impl Runtime {
    pub fn mesh_binding_snapshot(
        &self,
        client: ClientId,
        input: MeshBindingQuery,
    ) -> Result<MeshBindingBatch> {
        self.with_authoring_source(client, input.session, input.edit, |doc, source| {
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
                bytes = bytes
                    .saturating_add(
                        binding
                            .id
                            .len()
                            .saturating_add(binding.mesh_id.len())
                            .saturating_mul(6),
                    )
                    .saturating_add(1024);
                for axis in &binding.axes {
                    bytes = bytes
                        .saturating_add(
                            axis.id
                                .len()
                                .saturating_add(axis.parameter_id.len())
                                .saturating_mul(6),
                        )
                        .saturating_add(256);
                    for sample in &axis.samples {
                        bytes = bytes
                            .saturating_add(sample.id.len().saturating_mul(6))
                            .saturating_add(128);
                    }
                }
                for keyform in &binding.keyforms {
                    bytes = bytes
                        .saturating_add(keyform.positions.len().saturating_mul(128))
                        .saturating_add(1024);
                    for sample in &keyform.samples {
                        bytes = bytes
                            .saturating_add(sample.len().saturating_mul(6))
                            .saturating_add(64);
                    }
                }
                if bytes > 1 << 20 {
                    return Err(RuntimeError::Budget("mesh binding bytes"));
                }
            }
            self.check_bytes(bytes.saturating_mul(8), 0, 0)?;
            Ok(MeshBindingBatch {
                source,
                bindings: ids
                    .iter()
                    .map(|id| doc.get_binding(id).unwrap().clone())
                    .collect(),
            })
        })
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
        let source =
            self.with_authoring_source(client, input.session, input.edit, |doc, source| {
                if input.expected.is_some_and(|expected| expected != source) {
                    return Err(RuntimeError::InvalidReference);
                }
                let document = doc.estimated_content_bytes();
                let parameters =
                    input
                        .parameters
                        .iter()
                        .flat_map(|p| p.keys())
                        .fold(0usize, |n, id| {
                            n.saturating_add(id.len().saturating_mul(6))
                                .saturating_add(128)
                        });
                let response = document
                    .saturating_mul(input.parameters.len().max(1))
                    .saturating_mul(8)
                    .saturating_add(parameters)
                    .saturating_add(4096);
                if response > 8 << 20 {
                    return Err(RuntimeError::Budget(
                        "modeling sample batch; split the batch",
                    ));
                }
                self.check_bytes(
                    response
                        .saturating_mul(128)
                        .saturating_add(document.saturating_mul(4)),
                    0,
                    0,
                )?;
                Ok(source)
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
            source,
            state,
            samples: snapshot
                .snapshot
                .sample_modeling(input.parameters, input.checks)?,
        })
    }
}
