//! Bounded binary blob messages share authenticated request identity/recovery.
use super::*;
use std::sync::Arc;

pub const MAX_BINARY_CHUNK: usize = 1 << 20;
pub const MAX_BINARY_HEADER: usize = 16 << 10;

#[derive(Clone, Debug)]
pub struct BinarySlice {
    data: Arc<[u8]>,
    offset: usize,
    length: usize,
}
impl BinarySlice {
    pub fn new(data: Arc<[u8]>, offset: usize, length: usize) -> crate::Result<Self> {
        if length > MAX_BINARY_CHUNK
            || offset
                .checked_add(length)
                .is_none_or(|end| end > data.len())
        {
            return Err(RuntimeError::InvalidReference);
        }
        Ok(Self {
            data,
            offset,
            length,
        })
    }
    pub(crate) fn allocation(&self) -> (usize, usize) {
        (self.data.as_ptr() as usize, self.data.len())
    }
    pub fn bytes(&self) -> &[u8] {
        &self.data[self.offset..self.offset + self.length]
    }
}
pub struct BinaryReply {
    pub transfer: Option<zipzi_project::TransferReservation>,
    pub value: Value,
    pub payload: Option<BinarySlice>,
}
impl Protocol {
    fn invalidate_attached_binary(
        &self,
        runtime: &mut Runtime,
        auth: crate::local::Auth,
        header: &Value,
    ) {
        let authenticated = self
            .clients
            .get(&auth.client)
            .is_some_and(|c| c.connection == auth.connection && c.disconnected.is_none())
            && header["protocol"] == CONTRACT
            && serde_json::from_value::<ClientId>(header["client"].clone()).ok()
                == Some(auth.client)
            && serde_json::from_value::<RuntimeInstanceId>(header["runtime"].clone()).ok()
                == Some(runtime.instance_id());
        if authenticated {
            if let Ok(blob) = serde_json::from_value::<BlobId>(header["operation"]["blob"].clone())
            {
                let _ = runtime.fail_attached_upload(auth.client, blob);
            }
        }
    }

    /// The binary header is an ordinary Request. Input hashes include the exact
    /// payload digest; cached output retains immutable backing storage and its
    /// requested range, so release/other writes cannot change a replayed result.
    pub fn receive_binary(
        &mut self,
        runtime: &mut Runtime,
        auth: crate::local::Auth,
        header: Value,
        bytes: Vec<u8>,
        now: Instant,
    ) -> BinaryReply {
        self.expire(runtime, now);
        self.binary_output = None;
        if bytes.len() > MAX_BINARY_CHUNK {
            self.invalidate_attached_binary(runtime, auth, &header);
            return BinaryReply {
                transfer: None,
                value: failure("BUDGET_EXCEEDED", "Binary chunk limit"),
                payload: None,
            };
        }
        if header["operation"]["op"] == "append_blob_binary" {
            let digest: String = Sha256::digest(&bytes)
                .iter()
                .map(|v| format!("{v:02x}"))
                .collect();
            if header["operation"]["length"].as_u64() != Some(bytes.len() as u64)
                || header["operation"]["payload_sha256"].as_str() != Some(digest.as_str())
            {
                self.invalidate_attached_binary(runtime, auth, &header);
                return BinaryReply {
                    transfer: None,
                    value: failure("INVALID_REQUEST", "Binary payload identity differs"),
                    payload: None,
                };
            }
        } else if header["operation"]["op"] != "read_blob_binary" || !bytes.is_empty() {
            return BinaryReply {
                transfer: None,
                value: failure("INVALID_REQUEST", "Unknown binary operation"),
                payload: None,
            };
        }
        self.binary_input = Some(bytes);
        let value = self.receive(runtime, Some(auth), header, now);
        self.binary_input = None;
        BinaryReply {
            transfer: None,
            value,
            payload: self.binary_output.take(),
        }
    }
}
