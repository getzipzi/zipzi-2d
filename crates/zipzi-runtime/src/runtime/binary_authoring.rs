//! Candidate-bound binary preparation. Every failure after attachment aborts on
//! the host; a client never has to send an extra abort to enforce atomicity.
use super::*;
use crate::authoring::{EditOperation, WriteResult};
use zipzi_core::binary_wire::{Reader, WireRead, WireWrite};
pub const AUTHORING_MAGIC: &[u8; 4] = b"ZZA1";
fn invalid_wire(error: impl std::fmt::Display) -> RuntimeError {
    RuntimeError::Sdk(zipzi_sdk::SdkError {
        code: "INVALID_BINARY_INPUT".into(),
        message: error.to_string().into(),
        operation: "edit.apply",
        object_ids: vec![],
        field_path: None,
        expected_version: None,
        actual_version: None,
        referrers: Box::new([]),
    })
}
impl Runtime {
    pub fn start_authoring_upload(
        &mut self,
        client: ClientId,
        edit: EditId,
        length: usize,
    ) -> Result<BlobId> {
        let entry = self.edit(client, edit)?;
        if entry.state != EditState::Editing {
            return Err(RuntimeError::Aborted);
        }
        if entry.pending_upload.is_some() {
            return Err(RuntimeError::Busy);
        }
        let blob = self.create_blob(client, length)?;
        self.blobs.get_mut(&blob).unwrap().attached_edit = Some(edit);
        self.edits.get_mut(&edit).unwrap().pending_upload = Some(blob);
        Ok(blob)
    }
    pub(crate) fn fail_attached_upload(&mut self, client: ClientId, blob: BlobId) -> Result<()> {
        self.client(client)?;
        let Some(entry) = self.blobs.get(&blob) else {
            return Ok(());
        };
        if entry.client != client {
            return Err(RuntimeError::WrongOwner);
        }
        if let Some(edit) = entry.attached_edit {
            self.abort_edit(client, edit)?;
        }
        Ok(())
    }
    pub fn apply_authoring_blob(
        &mut self,
        client: ClientId,
        edit: EditId,
        blob: BlobId,
    ) -> Result<WriteResult> {
        let entry = self.edit(client, edit)?;
        if entry.pending_upload != Some(blob) {
            self.abort_edit(client, edit)?;
            return Err(RuntimeError::InvalidReference);
        }
        let result = (|| {
            let data = self.blob(client, blob)?;
            let mut measured = Reader::new(&data).map_err(invalid_wire)?;
            if measured.take(4).map_err(invalid_wire)? != AUTHORING_MAGIC
                || u128::decode(&mut measured).map_err(invalid_wire)?
                    != Vec::<EditOperation>::SIGNATURE
            {
                return Err(invalid_wire("Binary authoring contract differs"));
            }
            let count = measured.count().map_err(invalid_wire)?;
            if count > 1024 {
                return Err(RuntimeError::Budget("authoring operations"));
            }
            let mut cost = 4096usize;
            for _ in 0..count {
                cost = cost.saturating_add(
                    EditOperation::input_cost(&mut measured).map_err(invalid_wire)?,
                );
            }
            measured.finish().map_err(invalid_wire)?;
            self.refresh_memory_budget();
            let _input = self.reserve_transfer(cost.saturating_mul(4))?;
            let mut input = Reader::new(&data).map_err(invalid_wire)?;
            input.take(20).map_err(invalid_wire)?;
            let operations = Vec::<EditOperation>::decode(&mut input).map_err(invalid_wire)?;
            input.finish().map_err(invalid_wire)?;
            self.edits.get_mut(&edit).unwrap().pending_upload = None;
            self.blobs.get_mut(&blob).unwrap().attached_edit = None;
            self.apply_authoring(client, edit, operations)
        })();
        if result.is_err() {
            self.abort_edit(client, edit)?;
            self.blobs.remove(&blob);
        }
        result
    }
}
