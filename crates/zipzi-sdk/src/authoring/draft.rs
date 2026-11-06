use super::*;
use zipzi_core::state::{DocumentBuilder, WriteSet};
use zipzi_project::ResourceView;

/// Isolated, non-cloneable candidate. Hosts own its client/lifecycle authorization.
#[derive(Debug)]
pub struct Draft {
    source: DraftSource,
    baseline: Snapshot,
    workspace: Option<(DocumentBuilder, ResourceView)>,
}

/// An independently prepared preview. Only its exact originating candidate can
/// install it; preparation has no access to the live Draft's mutable workspace.
#[derive(Debug)]
pub struct PreparedPreview {
    source: DraftSource,
    builder: DocumentBuilder,
    resources: ResourceView,
}
impl PreparedPreview {
    pub fn source(&self) -> DraftSource {
        self.source
    }
    pub fn read_snapshot(&self) -> Result<Snapshot, Status> {
        Snapshot::new(self.builder.preview()?, self.resources.clone())
    }
}

/// An already frozen, resource-validated candidate. Consumed by publication.
#[derive(Debug)]
pub struct PreparedChange {
    pub(super) source: DraftSource,
    pub(super) snapshot: Snapshot,
    pub(super) writes: WriteSet,
    pub(super) description: String,
}
impl PreparedChange {
    pub fn source(&self) -> DraftSource {
        self.source
    }
    pub fn snapshot(&self) -> &Snapshot {
        &self.snapshot
    }
    pub fn writes(&self) -> &WriteSet {
        &self.writes
    }
}
impl Draft {
    pub(super) fn new(base: Version, snapshot: Snapshot) -> Self {
        Self {
            source: DraftSource {
                draft: DraftId::new(),
                base,
                sequence: 0,
            },
            workspace: Some((snapshot.document().builder(), snapshot.resources().clone())),
            baseline: snapshot,
        }
    }
    pub fn source(&self) -> DraftSource {
        self.source
    }
    pub fn is_active(&self) -> bool {
        self.workspace.is_some()
    }
    /// Fixed gesture source; baseline-relative commands need not accumulate drift.
    pub fn baseline(&self) -> &Snapshot {
        &self.baseline
    }
    pub fn document(&self) -> Result<&DocumentBuilder, Status> {
        self.workspace
            .as_ref()
            .map(|(b, _)| b)
            .ok_or_else(|| error("DRAFT_ENDED", "Draft has ended"))
    }
    pub fn read_snapshot(&self) -> Result<SourcedSnapshot, Status> {
        let (builder, resources) = self
            .workspace
            .as_ref()
            .ok_or_else(|| error("DRAFT_ENDED", "Draft has ended"))?;
        Ok(SourcedSnapshot {
            source: QuerySource::Draft(self.source),
            snapshot: Snapshot::new(builder.preview()?, resources.clone())?,
        })
    }
    /// One sequence increment for a successful batch. Any error, including an
    /// ignored command error or unwind, ends the candidate and drops its changes.
    pub fn batch<T>(
        &mut self,
        expected: DraftSource,
        run: impl FnOnce(&mut Commands<'_>) -> Result<T, Status>,
    ) -> Result<T, Status> {
        let (mut builder, mut resources) = self
            .workspace
            .take()
            .ok_or_else(|| error("DRAFT_ENDED", "Draft has ended"))?;
        if expected != self.source {
            return Err(error("STALE_SOURCE", "Candidate query source is stale"));
        }
        let next = self
            .source
            .next_sequence()
            .ok_or_else(|| error("VERSION_OVERFLOW", "Candidate sequence exhausted"))?;
        let mut commands = Commands::new(&mut builder, &mut resources);
        let value = run(&mut commands)?;
        commands.finish()?;
        // A batch may assemble forward references internally. Retain canonical
        // identities before exposing its successful result to the next batch.
        builder.validate_candidate()?;
        self.workspace = Some((builder, resources));
        self.source = next;
        Ok(value)
    }
    pub fn cancel(mut self) {
        self.workspace.take();
    }
    /// A rejected preview does not destroy the previous valid gesture position.
    /// Ordinary `batch` commands retain their abort-on-failure semantics.
    pub fn prepare_preview<T>(
        &self,
        expected: DraftSource,
        run: impl FnOnce(&mut Commands<'_>) -> Result<T, Status>,
    ) -> Result<(PreparedPreview, T), Status> {
        if expected != self.source {
            return Err(error("STALE_SOURCE", "Preview source is stale"));
        }
        let (mut builder, mut resources) = self
            .workspace
            .as_ref()
            .ok_or_else(|| error("DRAFT_ENDED", "Draft has ended"))?
            .clone();
        let mut commands = Commands::new(&mut builder, &mut resources);
        let value = run(&mut commands)?;
        commands.finish()?;
        builder.validate_candidate()?;
        // Resource validation is also preparation, never a post-install failure.
        Snapshot::new(builder.preview()?, resources.clone())?;
        Ok((
            PreparedPreview {
                source: expected,
                builder,
                resources,
            },
            value,
        ))
    }
    pub fn install_preview(&mut self, prepared: PreparedPreview) -> Result<DraftSource, Status> {
        if self.workspace.is_none() {
            return Err(error("DRAFT_ENDED", "Draft has ended"));
        }
        if prepared.source != self.source {
            return Err(error("STALE_SOURCE", "Prepared preview source is stale"));
        }
        let source = self
            .source
            .next_sequence()
            .ok_or_else(|| error("VERSION_OVERFLOW", "Candidate sequence exhausted"))?;
        self.workspace = Some((prepared.builder, prepared.resources));
        self.source = source;
        Ok(source)
    }
    /// Consuming preparation also consumes failed/stale commit attempts.
    pub fn prepare(mut self, description: impl Into<String>) -> Result<PreparedChange, Status> {
        let (builder, resources) = self
            .workspace
            .take()
            .ok_or_else(|| error("DRAFT_ENDED", "Draft has ended"))?;
        let frozen = builder.freeze()?;
        Ok(PreparedChange {
            source: self.source,
            snapshot: Snapshot::new(frozen.state, resources)?,
            writes: frozen.writes,
            description: description.into(),
        })
    }
}
