//! Borrowed admission precedes selected record cloning and adapter expansion.
use super::*;
use std::io::{self, Write};
use zipzi_core::binary_wire::WireWrite;
#[derive(serde::Serialize)]
pub struct ModelingRecordBatch {
    pub source: SourceRef,
    pub version: Version,
    pub state: &'static str,
    pub records: zipzi_sdk::OwnedModelingRecords,
    #[serde(skip)]
    pub lease: zipzi_project::TransferReservation,
}
#[derive(serde::Serialize)]
pub struct ModelingRecordBlob {
    pub source: SourceRef,
    pub blob: BlobId,
    pub length: usize,
    pub signature: String,
}
pub enum ModelingRecordResponse {
    Inline(ModelingRecordBatch),
    Blob(ModelingRecordBlob),
}
struct Counter {
    bytes: usize,
    limit: usize,
}
impl Write for Counter {
    fn write(&mut self, value: &[u8]) -> io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(value.len())
            .filter(|n| *n <= self.limit)
            .ok_or_else(|| io::Error::other("query response too large"))?;
        Ok(value.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
impl Runtime {
    fn modeling_index_bytes(doc: &Document, query: &zipzi_sdk::ModelingQuery) -> Result<usize> {
        let selections = [
            query.assets.len(),
            query.meshes.len(),
            query.parameters.len(),
            query.parts.len(),
            query.transforms.len(),
            query.bindings.len(),
            query.scene_bindings.len(),
            query.blend_key_tables.len(),
            query.blend_constraints.len(),
            query.blend_bindings.len(),
            query.glues.len(),
            query.offscreens.len(),
            query.parameter_groups.len(),
            query.attachments.len(),
            query.motions.len(),
            query.motion_groups.len(),
            query.motion_registrations.len(),
            query.expressions.len(),
            usize::from(query.physics),
            usize::from(query.pose),
        ]
        .into_iter()
        .sum::<usize>();
        if selections > 4096 {
            return Err(RuntimeError::Budget("modeling selections"));
        }
        let count = [
            doc.asset_order().len(),
            doc.mesh_order().len(),
            doc.parameter_order().len(),
            doc.part_order().len(),
            doc.transform_order().len(),
            doc.binding_order().len(),
            doc.scene_binding_order().len(),
            doc.blend_key_table_order().len(),
            doc.blend_constraint_order().len(),
            doc.blend_binding_order().len(),
            doc.glue_order().len(),
            doc.offscreen_order().len(),
            doc.package_attachments().len(),
            doc.motion_order().len(),
            doc.expression_order().len(),
            doc.motion_groups().len(),
            doc.motion_registrations().count(),
            usize::from(doc.physics().is_some()),
            usize::from(doc.pose().is_some()),
            doc.display_info()
                .parameter_groups
                .as_ref()
                .map_or(0, Vec::len),
        ]
        .into_iter()
        .fold(selections, usize::saturating_add);
        Ok(count.saturating_mul(64).saturating_add(4096))
    }
    pub fn modeling_snapshot(
        &self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        query: zipzi_sdk::ModelingQuery,
    ) -> Result<ModelingRecordBatch> {
        self.with_authoring_source(client, session, edit, |doc, source| {
            self.refresh_memory_budget();
            let _indexes = self.reserve_transfer(Self::modeling_index_bytes(doc, &query)?)?;
            let records = zipzi_sdk::modeling_records(doc, &query);
            let lease = self.reserve_transfer(records.expansion_bytes().saturating_add(4096))?;
            Ok(ModelingRecordBatch {
                source,
                version: source.version(),
                state: self.source_state(session)?,
                records: records.into_owned(),
                lease,
            })
        })
    }
    fn source_state(&self, session: SessionId) -> Result<&'static str> {
        Ok(match self.session_info(session)?.state {
            SessionState::Ready => "ready",
            SessionState::Busy(_) => "busy",
            SessionState::Faulted(_) => "faulted",
            _ => "unavailable",
        })
    }
    pub fn modeling_record_response(
        &mut self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        query: zipzi_sdk::ModelingQuery,
    ) -> Result<ModelingRecordResponse> {
        enum Prepared {
            Inline(ModelingRecordBatch),
            Blob(SourceRef, Arc<[u8]>),
        }
        let prepared = self.with_authoring_source(client, session, edit, |doc, source| {
            self.refresh_memory_budget();
            let _indexes = self.reserve_transfer(Self::modeling_index_bytes(doc, &query)?)?;
            let records = zipzi_sdk::modeling_records(doc, &query);
            let encoded = zipzi_core::binary_wire::encoded_len(&records)
                .map_err(|_| RuntimeError::Budget("binary record bytes"))?;
            // Small metadata retains the one-message route. Large arrays and
            // attachments are encoded directly from borrowed typed fields.
            if encoded <= 8192 {
                let mut counter = Counter {
                    bytes: 0,
                    limit: 65536,
                };
                if serde_json::to_writer(&mut counter, &records).is_ok() {
                    let lease = self.reserve_transfer(
                        counter
                            .bytes
                            .saturating_mul(128)
                            .saturating_add(records.expansion_bytes())
                            .saturating_add(4096),
                    )?;
                    return Ok(Prepared::Inline(ModelingRecordBatch {
                        source,
                        version: source.version(),
                        state: self.source_state(session)?,
                        records: records.into_owned(),
                        lease,
                    }));
                }
            }
            if self.blobs.values().filter(|b| b.client == client).count()
                >= self.limits.blobs_per_client
                || self
                    .blobs
                    .values()
                    .map(|b| b.length)
                    .sum::<usize>()
                    .saturating_add(encoded)
                    > self.limits.blob_bytes
            {
                return Err(RuntimeError::Budget("binary result blob bytes/count"));
            }
            let reservation = self
                .image_budget
                .reserve(encoded.saturating_mul(2))
                .map_err(|_| RuntimeError::Budget("binary result preparation"))?;
            let mut bytes = Vec::new();
            bytes
                .try_reserve_exact(encoded)
                .map_err(|_| RuntimeError::Budget("binary record allocation"))?;
            records
                .encode(&mut bytes)
                .map_err(|_| RuntimeError::InvalidReference)?;
            let data: Arc<[u8]> = bytes.into();
            crate::metrics::copy_payload(encoded.saturating_mul(2));
            reservation
                .commit(&data, zipzi_project::ImageBufferKind::Blob)
                .map_err(|_| RuntimeError::Budget("binary record registration"))?;
            Ok(Prepared::Blob(source, data))
        })?;
        Ok(match prepared {
            Prepared::Inline(value) => ModelingRecordResponse::Inline(value),
            Prepared::Blob(source, data) => {
                let length = data.len();
                let blob = self.retain_blob(client, data)?;
                ModelingRecordResponse::Blob(ModelingRecordBlob {
                    source,
                    blob,
                    length,
                    signature: format!("{:032x}", zipzi_sdk::OwnedModelingRecords::SIGNATURE),
                })
            }
        })
    }
}

impl Runtime {
    /// Immutable binary result; no serde Value expansion of its payload.
    pub fn sample_response(
        &mut self,
        client: ClientId,
        value: &crate::authoring::ModelingSampleBatch,
    ) -> Result<serde_json::Value> {
        let data = self.binary_result_blob(client, &value.samples)?;
        Ok(
            serde_json::json!({"source":value.source,"state":value.state,"blob":data.0,"length":data.1,"signature":format!("{:032x}",zipzi_sdk::ModelingSamples::SIGNATURE)}),
        )
    }
    pub(super) fn binary_result_blob<T: WireWrite>(
        &mut self,
        client: ClientId,
        value: &T,
    ) -> Result<(BlobId, usize)> {
        let size = zipzi_core::binary_wire::encoded_len(value)
            .map_err(|_| RuntimeError::Budget("binary output bytes"))?;
        if self.blobs.values().filter(|b| b.client == client).count()
            >= self.limits.blobs_per_client
            || self
                .blobs
                .values()
                .map(|b| b.length)
                .sum::<usize>()
                .saturating_add(size)
                > self.limits.blob_bytes
        {
            return Err(RuntimeError::Budget("binary result blob bytes/count"));
        }
        self.refresh_memory_budget();
        let reservation = self
            .image_budget
            .reserve(size.saturating_mul(2))
            .map_err(|_| RuntimeError::Budget("binary output preparation"))?;
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(size)
            .map_err(|_| RuntimeError::Budget("binary output allocation"))?;
        value
            .encode(&mut bytes)
            .map_err(|_| RuntimeError::InvalidReference)?;
        let data: Arc<[u8]> = bytes.into();
        reservation
            .commit(&data, zipzi_project::ImageBufferKind::Blob)
            .map_err(|_| RuntimeError::Budget("binary output registration"))?;
        Ok((self.retain_blob(client, data)?, size))
    }
}
