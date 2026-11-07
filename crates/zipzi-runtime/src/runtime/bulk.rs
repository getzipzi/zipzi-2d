//! Client-owned buffers. Allocation precedes copying and leases follow readers.
use super::*;

#[derive(Clone)]
pub struct BlobBytes {
    data: Arc<Vec<u8>>,
    _lease: Arc<TransferReservation>,
}
impl BlobBytes {
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }
}
pub(super) struct BlobEntry {
    pub owner: ClientId,
    length: usize,
    data: Arc<Vec<u8>>,
    lease: Arc<TransferReservation>,
    sealed: bool,
}
impl Runtime {
    pub fn create_blob(&mut self, client: ClientId, length: usize) -> Result<BlobId> {
        self.admit_blob(client, length)?;
        let lease = Arc::new(self.reserve_transfer(length)?);
        let mut data = Vec::new();
        data.try_reserve_exact(length)
            .map_err(|_| error("RESOURCE_LIMIT", "Bulk allocation failed"))?;
        let blob = BlobId::new();
        self.blobs.insert(
            blob,
            BlobEntry {
                owner: client,
                length,
                data: Arc::new(data),
                lease,
                sealed: false,
            },
        );
        Ok(blob)
    }
    fn admit_blob(&self, client: ClientId, length: usize) -> Result<()> {
        self.client(client)?;
        if length > 256 << 20 || self.blobs.values().filter(|b| b.owner == client).count() >= 64 {
            return Err(error("RESOURCE_LIMIT", "Bulk length/count limit"));
        }
        Ok(())
    }
    fn owned_blob(&self, client: ClientId, blob: BlobId) -> Result<&BlobEntry> {
        self.client(client)?;
        let entry = self
            .blobs
            .get(&blob)
            .ok_or_else(|| error("BLOB_NOT_FOUND", "Bulk buffer was released"))?;
        if entry.owner != client {
            return Err(error("NOT_OWNER", "Bulk buffer belongs to another client"));
        }
        Ok(entry)
    }
    pub fn append_blob(
        &mut self,
        client: ClientId,
        blob: BlobId,
        offset: usize,
        bytes: &[u8],
    ) -> Result<usize> {
        let entry = self.owned_blob(client, blob)?;
        if entry.sealed
            || offset != entry.data.len()
            || bytes.len() > 1 << 20
            || offset
                .checked_add(bytes.len())
                .is_none_or(|n| n > entry.length)
        {
            return Err(error(
                "INVALID_BLOB_RANGE",
                "Upload must append within its declared size",
            ));
        }
        let entry = self.blobs.get_mut(&blob).unwrap();
        Arc::get_mut(&mut entry.data)
            .expect("unsealed buffer is exclusive")
            .extend_from_slice(bytes);
        Ok(entry.data.len())
    }
    pub fn seal_blob(&mut self, client: ClientId, blob: BlobId) -> Result<usize> {
        let entry = self.owned_blob(client, blob)?;
        if entry.data.len() != entry.length {
            return Err(error("INCOMPLETE_BLOB", "Upload is incomplete"));
        }
        let entry = self.blobs.get_mut(&blob).unwrap();
        entry.sealed = true;
        Ok(entry.length)
    }
    pub fn blob(&self, client: ClientId, blob: BlobId) -> Result<BlobBytes> {
        let entry = self.owned_blob(client, blob)?;
        if !entry.sealed {
            return Err(error("INCOMPLETE_BLOB", "Upload is not sealed"));
        }
        Ok(BlobBytes {
            data: entry.data.clone(),
            _lease: entry.lease.clone(),
        })
    }
    pub fn read_blob_range(
        &self,
        client: ClientId,
        blob: BlobId,
        offset: usize,
        length: usize,
    ) -> Result<BlobBytes> {
        let data = self.blob(client, blob)?;
        if length > 1 << 20
            || offset
                .checked_add(length)
                .is_none_or(|end| end > data.bytes().len())
        {
            return Err(error("INVALID_BLOB_RANGE", "Binary range exceeds buffer"));
        }
        // Cache only the requested bounded chunk. Releasing a large source blob
        // cannot pin its whole allocation through a tiny recovered response.
        let lease = Arc::new(self.reserve_transfer(length)?);
        Ok(BlobBytes {
            data: Arc::new(data.bytes()[offset..offset + length].to_vec()),
            _lease: lease,
        })
    }
    pub fn release_blob(&mut self, client: ClientId, blob: BlobId) -> Result<()> {
        self.owned_blob(client, blob)?;
        self.blobs.remove(&blob);
        Ok(())
    }
    pub(crate) fn copy_blob(&mut self, client: ClientId, bytes: &[u8]) -> Result<BlobId> {
        let blob = self.create_blob(client, bytes.len())?;
        let entry = self.blobs.get_mut(&blob).unwrap();
        Arc::get_mut(&mut entry.data)
            .unwrap()
            .extend_from_slice(bytes);
        entry.sealed = true;
        Ok(blob)
    }
    pub fn replace_image_blob(
        &mut self,
        client: ClientId,
        expected: DraftSource,
        id: String,
        blob: BlobId,
        width: u32,
        height: u32,
    ) -> Result<DraftSource> {
        // Ownership/admission errors leave the Draft intact; accepted command
        // and resource validation failures follow ordinary batch abort rules.
        let bytes = self.blob(client, blob)?;
        let resources = self.resources.clone();
        let (source, ()) = self.batch(client, expected, move |commands| {
            let length = (width as usize)
                .checked_mul(height as usize)
                .and_then(|n| n.checked_mul(4));
            if width == 0 || height == 0 || length != Some(bytes.bytes().len()) {
                return Err(error(
                    "INVALID_IMAGE",
                    "RGBA dimensions do not match bulk bytes",
                ));
            }
            let lease = resources.budget().reserve_transfer(bytes.bytes().len())?;
            let image = zipzi_project::PreparedImageContent::from_rgba(
                width,
                height,
                bytes.bytes().to_vec().into(),
                resources.budget(),
            )?;
            let resource = resources.adopt_image(image)?;
            commands.replace_image(&id, resource)?;
            drop(lease);
            Ok(())
        })?;
        Ok(source)
    }
    pub fn read_image_blob(
        &mut self,
        client: ClientId,
        source: QuerySource,
        id: &str,
    ) -> Result<serde_json::Value> {
        let snapshot = self.capture(client, source)?;
        let asset = snapshot
            .snapshot
            .document()
            .asset(id)
            .ok_or_else(|| error("MISSING_ASSET", id))?;
        let image = snapshot.snapshot.resources().read_texture(asset)?;
        let blob = self.copy_blob(client, &image.rgba)?;
        Ok(
            serde_json::json!({"source":source, "content":snapshot.snapshot.content_id(), "blob":blob,"width":image.width,"height":image.height,"length":image.rgba.len()}),
        )
    }
    pub fn read_job_blob(&mut self, client: ClientId, job: JobId) -> Result<serde_json::Value> {
        let result = self
            .job(client, job)?
            .result
            .clone()
            .ok_or_else(|| error("JOB_BUSY", "Job is outstanding"))??;
        let JobValue::Read(read) = result else {
            return Err(error("INVALID_JOB_RESULT", "Job has no byte result"));
        };
        // Read results are immutable and already leased. A Blob owns another
        // handle to that same allocation, rather than copying every full sample
        // or observation result onto the owner thread. Chunk recovery still
        // retains only bounded copies through read_blob_range().
        self.admit_blob(client, read.bytes().len())?;
        let (data, lease) = read.blob_parts();
        let blob = BlobId::new();
        self.blobs.insert(
            blob,
            BlobEntry {
                owner: client,
                length: read.bytes().len(),
                data,
                lease,
                sealed: true,
            },
        );
        Ok(serde_json::json!({"source":read.source,"blob":blob,"length":read.bytes().len()}))
    }
}
