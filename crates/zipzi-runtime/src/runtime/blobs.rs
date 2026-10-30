//! Immutable, client-owned bulk payloads. A reservation precedes buffer growth.
use super::*;
pub(super) struct BlobEntry {
    pub attached_edit: Option<EditId>,
    pub client: ClientId,
    pub length: usize,
    data: Vec<u8>,
    sealed: Option<Arc<[u8]>>,
    reservation: Option<zipzi_project::ImageMemoryReservation>,
}
impl Runtime {
    /// Register immutable, already-accounted resource content without copying.
    pub(crate) fn retain_blob(&mut self, client: ClientId, data: Arc<[u8]>) -> Result<BlobId> {
        self.client(client)?;
        if data.len() > 256 << 20
            || self.blobs.values().filter(|b| b.client == client).count()
                >= self.limits.blobs_per_client
            || self
                .blobs
                .values()
                .map(|b| b.length)
                .sum::<usize>()
                .saturating_add(data.len())
                > self.limits.blob_bytes
        {
            return Err(RuntimeError::Budget("retained blob bytes/count"));
        }
        self.check_bytes(1024, 0, 0)?;
        self.image_budget
            .track(&data, zipzi_project::ImageBufferKind::Blob)
            .map_err(resource_error)?;
        let id = BlobId::from_parts(self.instance, self.serial());
        self.blobs.insert(
            id,
            BlobEntry {
                attached_edit: None,
                client,
                length: data.len(),
                data: vec![],
                sealed: Some(data),
                reservation: None,
            },
        );
        Ok(id)
    }
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
        // Allocate the admitted capacity once. Geometric Vec growth can exceed
        // the reservation, even when logical upload length remains in range.
        let mut data = Vec::new();
        data.try_reserve_exact(length)
            .map_err(|_| RuntimeError::Budget("blob allocation"))?;
        let id = BlobId::from_parts(self.instance, self.serial());
        self.blobs.insert(
            id,
            BlobEntry {
                attached_edit: None,
                client,
                length,
                data,
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
            .get(&blob)
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
            self.fail_attached_upload(client, blob)?;
            return Err(RuntimeError::InvalidReference);
        }
        let entry = self.blobs.get_mut(&blob).unwrap();
        entry.data.extend_from_slice(bytes);
        crate::metrics::copy_payload(bytes.len());
        Ok(entry.data.len())
    }
    pub fn seal_blob(&mut self, client: ClientId, blob: BlobId) -> Result<usize> {
        self.client(client)?;
        let entry = self
            .blobs
            .get(&blob)
            .ok_or(RuntimeError::InvalidReference)?;
        if entry.client != client {
            return Err(RuntimeError::WrongOwner);
        }
        let length = entry.length;
        if entry.sealed.is_some() {
            return Ok(length);
        }
        if entry.data.len() != length {
            self.fail_attached_upload(client, blob)?;
            return Err(RuntimeError::InvalidReference);
        }
        let peak = match self.image_budget.reserve(length).map_err(resource_error) {
            Ok(lease) => lease,
            Err(error) => {
                self.fail_attached_upload(client, blob)?;
                return Err(error);
            }
        };
        let entry = self.blobs.get_mut(&blob).unwrap();
        let data: Arc<[u8]> = std::mem::take(&mut entry.data).into();
        crate::metrics::copy_payload(entry.length);
        entry
            .reservation
            .take()
            .unwrap()
            .commit(&data, zipzi_project::ImageBufferKind::Blob)
            .map_err(resource_error)?;
        entry.sealed = Some(data);
        drop(peak);
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
        if blob.runtime_instance() != self.instance {
            return Err(RuntimeError::InvalidReference);
        }
        if let Some(entry) = self.blobs.get(&blob) {
            if entry.client != client {
                return Err(RuntimeError::WrongOwner);
            }
            self.fail_attached_upload(client, blob)?;
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
