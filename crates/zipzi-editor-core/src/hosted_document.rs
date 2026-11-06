//! Typed view reference; Runtime retains formal content and history.
use zipzi_runtime::{ClientId, Result, Runtime, SessionKey};

/// A view's typed reference to a hosted session; contains no authoring state or
/// history. Read access borrows the sole Runtime-owned SDK instance directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostedDocument {
    client: ClientId,
    session: SessionKey,
}
impl HostedDocument {
    pub fn new(client: ClientId, session: SessionKey) -> Self {
        Self { client, session }
    }
    pub fn session_id(self) -> SessionKey {
        self.session
    }
    pub fn read(self, runtime: &Runtime) -> Result<&zipzi_sdk::AuthoringSession> {
        runtime.session_read(self.client, self.session)
    }
}
