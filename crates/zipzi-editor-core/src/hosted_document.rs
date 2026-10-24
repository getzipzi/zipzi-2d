//! Minimal R1 bridge. Formal ownership migration of Editor Document is R2.
use zipzi_runtime::{Result, Runtime, SessionId};

/// A view's typed reference to a hosted session; contains no authoring state or
/// history. Read access borrows the sole Runtime-owned SDK instance directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostedDocument {
    session: SessionId,
}
impl HostedDocument {
    pub fn new(session: SessionId) -> Self {
        Self { session }
    }
    pub fn session_id(self) -> SessionId {
        self.session
    }
    pub fn read(self, runtime: &Runtime) -> Result<&zipzi_sdk::AuthoringSession> {
        runtime.session(self.session)
    }
}
