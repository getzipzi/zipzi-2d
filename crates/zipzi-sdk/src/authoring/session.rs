use super::history::{History, Step};
use super::*;
use std::{collections::VecDeque, sync::Arc};
use zipzi_core::state::WriteSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PublicationKind {
    Commit,
    Undo,
    Redo,
    Replace,
    Unchanged,
}

#[cfg(test)]
mod tests {
    use super::*;
    use zipzi_core::{state::DocumentState, ModelSetup, Parameter};
    use zipzi_project::ResourceView;

    #[test]
    fn version_overflow_never_installs_content_history_or_events() {
        let state = DocumentState::new(
            "00000000-0000-4000-8000-000000000001",
            ModelSetup::default(),
        )
        .unwrap();
        let snapshot = Snapshot::new(state, ResourceView::default()).unwrap();
        let mut session = AuthoringSession::new(snapshot.clone());
        session.version.revision = u64::MAX;
        let mut draft = session.begin_draft();
        draft
            .batch(draft.source(), |c| {
                c.create_parameter(Parameter {
                    id: "00000000-0000-4000-8000-000000000002".into(),
                    ..Default::default()
                })
            })
            .unwrap();
        assert_eq!(
            session
                .publish(draft.prepare("overflow").unwrap())
                .unwrap_err()
                .code,
            "VERSION_OVERFLOW"
        );
        assert_eq!(session.snapshot().content_id(), snapshot.content_id());
        assert_eq!(session.history_state().undo_steps, 0);
        assert!(session.take_events().is_empty());
        session.version.revision = 0;
        session.version.generation = u64::MAX;
        assert_eq!(
            session
                .replace(session.version(), snapshot.clone())
                .unwrap_err()
                .code,
            "VERSION_OVERFLOW"
        );
        assert_eq!(session.version.revision, 0);
        assert!(session.take_events().is_empty());
    }
}

/// Durable-in-memory receipt. Delivering a notification cannot roll back state.
#[derive(Debug, Clone)]
pub struct Publication {
    pub version: Version,
    pub content: ContentId,
    pub kind: PublicationKind,
    pub description: Arc<str>,
    pub writes: Arc<WriteSet>,
    pub history: HistoryState,
}

/// The sole owner that switches formal model content. Workers receive Snapshot,
/// Draft or PreparedChange; none of those carries this Session or its history.
#[derive(Debug)]
pub struct AuthoringSession {
    pub(super) current: Snapshot,
    pub(super) version: Version,
    history: History,
    events: VecDeque<Publication>,
    pub(super) project: Option<ProjectContext>,
    pub(super) saved: Option<SavedMarker>,
    pub(super) saving: Option<Arc<std::sync::atomic::AtomicU8>>,
}
impl AuthoringSession {
    pub fn new(snapshot: Snapshot) -> Self {
        Self {
            current: snapshot,
            version: Version {
                session: SessionKey::new(),
                generation: 0,
                revision: 0,
            },
            history: History::default(),
            events: VecDeque::new(),
            project: None,
            saved: None,
            saving: None,
        }
    }
    pub fn version(&self) -> Version {
        self.version
    }
    pub fn snapshot(&self) -> &Snapshot {
        &self.current
    }
    pub fn read_snapshot(&self) -> SourcedSnapshot {
        SourcedSnapshot {
            source: QuerySource::Published(self.version),
            snapshot: self.current.clone(),
        }
    }
    pub fn begin_draft(&self) -> Draft {
        Draft::new(self.version, self.current.clone())
    }
    pub fn history_state(&self) -> HistoryState {
        self.history.state()
    }
    pub fn set_history_limits(&mut self, limits: HistoryLimits) -> HistoryState {
        self.history.limits = limits;
        self.history.trim();
        self.history.state()
    }
    pub fn take_events(&mut self) -> VecDeque<Publication> {
        std::mem::take(&mut self.events)
    }
    fn check_source(&self, version: Version) -> Result<(), Status> {
        self.check_not_saving()?;
        if version == self.version {
            Ok(())
        } else {
            Err(error("STALE_SOURCE", "Formal publication source is stale"))
        }
    }
    pub fn publish(&mut self, prepared: PreparedChange) -> Result<Publication, Status> {
        self.publish_with_control(prepared, || true)
    }
    pub fn publish_with_control(
        &mut self,
        prepared: PreparedChange,
        mut keep_going: impl FnMut() -> bool,
    ) -> Result<Publication, Status> {
        self.check_source(prepared.source.base)?;
        if !keep_going() {
            return Err(error("CANCELLED", "Publication cancelled"));
        }
        let writes = Arc::new(prepared.writes);
        let description: Arc<str> = prepared.description.into();
        if writes.is_empty() {
            return Ok(Publication {
                version: self.version,
                content: self.current.content_id(),
                kind: PublicationKind::Unchanged,
                description,
                writes,
                history: self.history.state(),
            });
        }
        let version = self
            .version
            .next_publication()
            .ok_or_else(|| error("VERSION_OVERFLOW", "Publication revision exhausted"))?;
        let mut history = self.history.clone();
        history.push(Step {
            before: self.current.clone(),
            after: prepared.snapshot.clone(),
            description: description.clone(),
            writes: writes.clone(),
        });
        if !keep_going() {
            return Err(error("CANCELLED", "Publication cancelled"));
        }
        self.install(
            prepared.snapshot,
            version,
            history,
            PublicationKind::Commit,
            description,
            writes,
        )
    }
    pub fn undo(&mut self, expected: Version) -> Result<Publication, Status> {
        self.restore(expected, false)
    }
    pub fn redo(&mut self, expected: Version) -> Result<Publication, Status> {
        self.restore(expected, true)
    }
    fn restore(&mut self, expected: Version, redo: bool) -> Result<Publication, Status> {
        self.check_source(expected)?;
        let index = if redo {
            self.history.cursor
        } else {
            self.history
                .cursor
                .checked_sub(1)
                .ok_or_else(|| error("NO_UNDO", "No undo step"))?
        };
        let step = self
            .history
            .steps
            .get(index)
            .ok_or_else(|| error("NO_REDO", "No redo step"))?;
        let snapshot = if redo {
            step.after.clone()
        } else {
            step.before.clone()
        };
        let description = step.description.clone();
        let writes = step.writes.clone();
        let version = self
            .version
            .next_publication()
            .ok_or_else(|| error("VERSION_OVERFLOW", "Publication revision exhausted"))?;
        let mut history = self.history.clone();
        history.cursor = if redo { index + 1 } else { index };
        self.install(
            snapshot,
            version,
            history,
            if redo {
                PublicationKind::Redo
            } else {
                PublicationKind::Undo
            },
            description,
            writes,
        )
    }
    /// Prepared load results replace content only against their original source.
    /// A raw replacement starts an unsaved document with no project destination.
    pub fn replace(
        &mut self,
        expected: Version,
        snapshot: Snapshot,
    ) -> Result<Publication, Status> {
        self.check_source(expected)?;
        let version = self
            .version
            .next_generation()
            .ok_or_else(|| error("VERSION_OVERFLOW", "Document generation exhausted"))?;
        let history = History {
            limits: self.history.limits,
            ..Default::default()
        };
        let receipt = self.install(
            snapshot,
            version,
            history,
            PublicationKind::Replace,
            Arc::from("Replace document"),
            Arc::default(),
        )?;
        self.project = None;
        self.saved = None;
        Ok(receipt)
    }
    /// Prepare all fallible metadata before the single formal installation point.
    fn install(
        &mut self,
        snapshot: Snapshot,
        version: Version,
        history: History,
        kind: PublicationKind,
        description: Arc<str>,
        writes: Arc<WriteSet>,
    ) -> Result<Publication, Status> {
        self.events
            .try_reserve(1)
            .map_err(|_| error("RESOURCE_LIMIT", "Cannot reserve publication event"))?;
        let receipt = Publication {
            version,
            content: snapshot.content_id(),
            kind,
            description,
            writes,
            history: history.state(),
        };
        // No callbacks, model validation, resource acquisition or recoverable
        // business failures follow this point.
        self.current = snapshot;
        self.version = version;
        self.history = history;
        self.events.push_back(receipt.clone());
        Ok(receipt)
    }
}
