use super::*;
use crate::jobs::ReadTask;

impl Runtime {
    pub(super) fn spatial_bytes(
        doc: &Document,
        frames: usize,
        queries: &[zipzi_sdk::SpatialQuery],
    ) -> Result<usize> {
        if queries.len() > 64
            || queries
                .iter()
                .map(|q| match q {
                    zipzi_sdk::SpatialQuery::Bounds { targets, .. } => targets.len(),
                    _ => 0,
                })
                .sum::<usize>()
                > 4096
        {
            return Err(RuntimeError::Budget("spatial query selection"));
        }
        let ancestry = doc
            .part_order()
            .iter()
            .filter_map(|id| doc.get_part(id))
            .fold(0usize, |n, p| {
                n.saturating_add(p.id.len())
                    .saturating_add(p.name.len())
                    .saturating_add(128)
            });
        let mut paths = 0usize;
        for query in queries {
            if let zipzi_sdk::SpatialQuery::Hits { max_candidates, .. } = query {
                if *max_candidates > 4096 {
                    return Err(RuntimeError::Budget("spatial candidates"));
                }
                paths = paths.saturating_add(
                    ancestry.saturating_mul((*max_candidates).min(doc.mesh_order().len())),
                );
            }
        }
        Ok(Self::sample_bytes(doc, frames)
            .saturating_mul(queries.len().saturating_add(1))
            .saturating_mul(3)
            .saturating_add(paths.saturating_mul(frames).saturating_mul(4))
            .saturating_add(serialized_bytes(&queries)?.saturating_mul(4)))
    }
    pub fn request_spatial_samples(
        &mut self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        parameters: Vec<zipzi_core::PreviewValues>,
        queries: Vec<zipzi_sdk::SpatialQuery>,
        expected: Option<SourceRef>,
    ) -> Result<JobId> {
        self.client(client)?;
        self.check_job_slot(client)?;
        if parameters.len() > 64 || parameters.iter().any(|p| p.len() > 256) {
            return Err(RuntimeError::Budget("spatial samples"));
        }
        let (document, bytes) =
            self.with_authoring_source(client, session, edit, |doc, source| {
                if expected.is_some_and(|s| s != source) {
                    return Err(RuntimeError::InvalidReference);
                }
                Ok((
                    doc.estimated_content_bytes(),
                    Self::spatial_bytes(doc, parameters.len(), &queries)?,
                ))
            })?;
        self.check_bytes(bytes.saturating_add(document), 0, document)?;
        let source = if let Some(edit) = edit {
            self.edit_snapshot(client, edit)?
        } else {
            self.source_snapshot(session)?
        };
        let task = ReadTask::Spatial {
            source: source.clone(),
            parameters,
            queries,
        };
        self.queue_read(client, source, task, bytes, false)
    }
    pub fn spatial_response(
        &mut self,
        client: ClientId,
        value: &crate::spatial::SpatialSampleBatch,
    ) -> Result<serde_json::Value> {
        use zipzi_core::binary_wire::WireWrite;
        let (blob, length) = self.binary_result_blob(client, &value.samples)?;
        Ok(
            serde_json::json!({"source":value.source,"blob":blob,"length":length,"signature":format!("{:032x}",zipzi_sdk::SpatialSamples::SIGNATURE)}),
        )
    }
}
