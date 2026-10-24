//! Immutable, client-owned bulk payloads. A reservation precedes buffer growth.
use super::*;
pub(super) struct BlobEntry {
    pub client: ClientId,
    pub length: usize,
    data: Vec<u8>,
    sealed: Option<Arc<[u8]>>,
    reservation: Option<zipzi_project::ImageMemoryReservation>,
}
impl Runtime {
    pub fn create_blob(&mut self, client: ClientId, length: usize) -> Result<BlobId> {
        self.client(client)?;
        if length > 256 << 20
            || self.blobs.values().filter(|b| b.client == client).count()
                >= self.limits.blobs_per_client
        {
            return Err(RuntimeError::Budget("blob length/count"));
        }
        if self
            .blobs
            .values()
            .map(|b| b.length)
            .sum::<usize>()
            .saturating_add(length)
            > self.limits.blob_bytes
        {
            return Err(RuntimeError::Budget("blob bytes"));
        }
        self.check_bytes(length, 0, 0)?;
        let reservation = self.image_budget.reserve(length).map_err(resource_error)?;
        let id = BlobId {
            runtime: self.instance,
            serial: self.serial(),
        };
        self.blobs.insert(
            id,
            BlobEntry {
                client,
                length,
                data: Vec::new(),
                sealed: None,
                reservation: Some(reservation),
            },
        );
        Ok(id)
    }
    pub fn append_blob(
        &mut self,
        client: ClientId,
        blob: BlobId,
        offset: usize,
        bytes: &[u8],
    ) -> Result<usize> {
        self.client(client)?;
        let entry = self
            .blobs
            .get_mut(&blob)
            .ok_or(RuntimeError::InvalidReference)?;
        if entry.client != client {
            return Err(RuntimeError::WrongOwner);
        }
        if entry.sealed.is_some()
            || offset != entry.data.len()
            || bytes.len() > 1 << 20
            || offset
                .checked_add(bytes.len())
                .is_none_or(|n| n > entry.length)
        {
            return Err(RuntimeError::InvalidReference);
        }
        entry.data.extend_from_slice(bytes);
        Ok(entry.data.len())
    }
    pub fn seal_blob(&mut self, client: ClientId, blob: BlobId) -> Result<usize> {
        self.client(client)?;
        let entry = self
            .blobs
            .get_mut(&blob)
            .ok_or(RuntimeError::InvalidReference)?;
        if entry.client != client {
            return Err(RuntimeError::WrongOwner);
        }
        if entry.sealed.is_none() {
            if entry.data.len() != entry.length {
                return Err(RuntimeError::InvalidReference);
            }
            let data: Arc<[u8]> = std::mem::take(&mut entry.data).into();
            entry
                .reservation
                .take()
                .unwrap()
                .commit(&data, zipzi_project::ImageBufferKind::Blob)
                .map_err(resource_error)?;
            entry.sealed = Some(data);
        }
        Ok(entry.length)
    }
    pub fn blob(&self, client: ClientId, blob: BlobId) -> Result<Arc<[u8]>> {
        self.client(client)?;
        let entry = self
            .blobs
            .get(&blob)
            .ok_or(RuntimeError::InvalidReference)?;
        if entry.client != client {
            return Err(RuntimeError::WrongOwner);
        }
        entry.sealed.clone().ok_or(RuntimeError::Busy)
    }
    pub fn release_blob(&mut self, client: ClientId, blob: BlobId) -> Result<()> {
        self.client(client)?;
        if let Some(entry) = self.blobs.get(&blob) {
            if entry.client != client {
                return Err(RuntimeError::WrongOwner);
            }
            self.blobs.remove(&blob);
        }
        Ok(())
    }
    pub fn blob_count(&self) -> usize {
        self.blobs.len()
    }
}
fn resource_error(status: zipzi_core::Status) -> RuntimeError {
    RuntimeError::Sdk(zipzi_sdk::SdkError {
        code: status.code.into(),
        message: status.message.into(),
        operation: "blob",
        object_ids: vec![],
        field_path: None,
        expected_version: None,
        actual_version: None,
        referrers: Box::new([]),
    })
}
