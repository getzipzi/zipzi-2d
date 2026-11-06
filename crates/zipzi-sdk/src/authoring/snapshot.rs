use super::*;
use zipzi_core::state::DocumentState;
use zipzi_project::ResourceView;

/// A validated model/resource pair. Resource ownership cannot be separated by callers.
#[derive(Debug, Clone)]
pub struct Snapshot {
    document: DocumentState,
    resources: ResourceView,
}
impl Snapshot {
    pub fn new(document: DocumentState, resources: ResourceView) -> Result<Self, Status> {
        let resources = resources.for_state(&document)?;
        Ok(Self {
            document,
            resources,
        })
    }
    pub fn document(&self) -> &DocumentState {
        &self.document
    }
    pub fn resources(&self) -> &ResourceView {
        &self.resources
    }
    pub fn content_id(&self) -> ContentId {
        self.document.content_id()
    }
}

/// Query provenance travels with the frozen input, including candidate sequence.
#[derive(Debug, Clone)]
pub struct SourcedSnapshot {
    pub source: QuerySource,
    pub snapshot: Snapshot,
}
