//! Binary frames use the same authenticated request IDs and recovery window.
use super::*;
#[derive(Clone)]
pub struct BinarySlice {
    data: BlobBytes,
    offset: usize,
    length: usize,
}
impl BinarySlice {
    pub fn new(data: BlobBytes, offset: usize, length: usize) -> Result<Self> {
        if length > MAX_BINARY_CHUNK
            || offset
                .checked_add(length)
                .is_none_or(|end| end > data.bytes().len())
        {
            return Err(error("INVALID_BLOB_RANGE", "Binary range exceeds buffer"));
        }
        Ok(Self {
            data,
            offset,
            length,
        })
    }
    pub fn bytes(&self) -> &[u8] {
        &self.data.bytes()[self.offset..self.offset + self.length]
    }
    pub(super) fn retained_bytes(&self) -> usize {
        self.data.bytes().len()
    }
}
pub struct BinaryReply {
    pub transfer: Option<zipzi_project::TransferReservation>,
    pub value: Value,
    pub payload: Option<BinarySlice>,
}
pub(super) fn validate_payload(length: usize, expected: &str, bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_BINARY_CHUNK
        || length != bytes.len()
        || Sha256::digest(bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
            != expected
    {
        return Err(error("INVALID_REQUEST", "Binary payload identity differs"));
    }
    Ok(())
}
impl Protocol {
    fn binary_input(operation: &Value, bytes: &[u8]) -> Result<()> {
        match operation["op"].as_str() {
            Some("append_blob_binary") => validate_payload(
                operation["length"]
                    .as_u64()
                    .and_then(|n| usize::try_from(n).ok())
                    .unwrap_or(usize::MAX),
                operation["payload_sha256"].as_str().unwrap_or(""),
                bytes,
            ),
            Some("read_blob_binary") if bytes.is_empty() => Ok(()),
            _ => Err(error(
                "INVALID_REQUEST",
                "Unexpected binary operation or payload",
            )),
        }
    }
    pub fn receive_binary(
        &mut self,
        runtime: &mut Runtime,
        auth: crate::local::Auth,
        header: Value,
        bytes: Vec<u8>,
        now: Instant,
    ) -> BinaryReply {
        self.binary_output = None;
        // Check exact payload identity before looking up a cached request: a
        // replay with substituted bytes must never return a successful receipt.
        if let Err(e) = Self::binary_input(&header["operation"], &bytes) {
            return BinaryReply {
                transfer: None,
                value: status(e),
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
    /// Local owned values enter the same operation handlers without wire encoding.
    pub fn execute_binary(
        &mut self,
        runtime: &mut Runtime,
        client: ClientId,
        operation: Value,
        bytes: Vec<u8>,
    ) -> BinaryReply {
        self.binary_output = None;
        let result = Self::binary_input(&operation, &bytes).and_then(|()| {
            let op = serde_json::from_value(operation)
                .map_err(|e| error("INVALID_REQUEST", &e.to_string()))?;
            self.binary_input = Some(bytes);
            Ok(self.execute_response(runtime, client, op))
        });
        self.binary_input = None;
        BinaryReply {
            transfer: None,
            value: result.unwrap_or_else(status),
            payload: self.binary_output.take(),
        }
    }
}
