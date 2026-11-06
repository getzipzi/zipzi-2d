mod bulk;
mod hosting;
mod tasks;
pub use bulk::BlobBytes;
pub(crate) mod imports;
mod wire;
use crate::jobs::{Batch, Identity, Output, Read, Task, TaskKind};
use crate::*;
pub use hosting::HostStateStamp;
use std::{
    collections::{HashMap, HashSet, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use zipzi_core::{state::DocumentState, ModelSetup};
use zipzi_project::{
    archive::ProjectFingerprint, ImageMemoryBudget, ProjectIo, ResourceView, TransferReservation,
};
use zipzi_sdk::{AuthoringSession, Commands, Draft, Publication, Snapshot};

struct SessionEntry {
    sdk: AuthoringSession,
    users: HashSet<ClientId>,
    saving: Option<JobId>,
    events: VecDeque<PublicationEvent>,
    event_floor: Version,
    protected_paths: Vec<PathBuf>,
    editor_context: Option<crate::editor_context::EditorContext>,
}
enum DraftState {
    Ready(Draft),
    Working(JobId),
}
struct DraftEntry {
    owner: ClientId,
    source: DraftSource,
    state: DraftState,
}
struct JobEntry {
    info: JobInfo,
    cancel: Cancellation,
    kind: TaskKind,
    entered: bool,
    target: Option<PathBuf>,
}
type Leases = (Arc<TransferReservation>, Arc<TransferReservation>);

/// All mutations are serialized by `&mut Runtime`. No API hands out mutable
/// Session access, and dispatch has no Session to take out or restore.
pub struct Runtime {
    limits: RuntimeLimits,
    instance: RuntimeInstanceId,
    resources: zipzi_project::ResourceStore,
    project_io: ProjectIo,
    clients: HashSet<ClientId>,
    sessions: HashMap<SessionKey, SessionEntry>,
    drafts: HashMap<DraftId, DraftEntry>,
    jobs: HashMap<JobId, JobEntry>,
    queue: VecDeque<Mutex<Work>>,
    budget: ImageMemoryBudget,
    paths: HashMap<PathBuf, SessionKey>,
    reservations: HashMap<PathBuf, JobId>,
    blobs: HashMap<BlobId, bulk::BlobEntry>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self::new(RuntimeLimits::default())
    }
}
impl Runtime {
    pub fn new(limits: RuntimeLimits) -> Self {
        let budget = ImageMemoryBudget::new(limits.task_bytes);
        budget.bind_host_budget(0, limits.task_bytes);
        let resources = zipzi_project::ResourceStore::new(budget.clone());
        let project_io = ProjectIo::new(
            std::env::temp_dir().join("zipzi-projects"),
            resources.clone(),
        );
        Self {
            instance: RuntimeInstanceId::new(),
            resources,
            project_io,
            limits,
            clients: HashSet::new(),
            sessions: HashMap::new(),
            drafts: HashMap::new(),
            jobs: HashMap::new(),
            queue: VecDeque::new(),
            budget,
            paths: HashMap::new(),
            reservations: HashMap::new(),
            blobs: HashMap::new(),
        }
    }
    pub fn register_client(&mut self) -> Result<ClientId> {
        if self.clients.len() >= self.limits.clients {
            return Err(error("RESOURCE_LIMIT", "Client limit"));
        }
        let id = ClientId::new();
        self.clients.insert(id);
        Ok(id)
    }
    fn client(&self, client: ClientId) -> Result<()> {
        if self.clients.contains(&client) {
            Ok(())
        } else {
            Err(error("CLIENT_NOT_FOUND", "Client is detached"))
        }
    }
    fn session(&self, client: ClientId, session: SessionKey) -> Result<&SessionEntry> {
        self.client(client)?;
        let entry = self
            .sessions
            .get(&session)
            .ok_or_else(|| error("SESSION_NOT_FOUND", "Session does not exist"))?;
        if !entry.users.contains(&client) {
            return Err(error("NOT_OWNER", "Client is not attached to Session"));
        }
        Ok(entry)
    }
    fn writable(&self, client: ClientId, session: SessionKey) -> Result<&SessionEntry> {
        let entry = self.session(client, session)?;
        if entry.saving.is_some() {
            return Err(error("PROJECT_BUSY", "Save is outstanding"));
        }
        Ok(entry)
    }
    fn source(&self, client: ClientId, expected: Version) -> Result<&SessionEntry> {
        let entry = self.writable(client, expected.session)?;
        if entry.sdk.version() != expected {
            return Err(error("STALE_SOURCE", "Publication source is stale"));
        }
        Ok(entry)
    }
    pub fn create_session(
        &mut self,
        client: ClientId,
        id: &str,
        setup: ModelSetup,
    ) -> Result<SessionKey> {
        let snapshot = Snapshot::new(DocumentState::new(id, setup)?, ResourceView::default())?;
        self.insert_session(client, snapshot)
    }
    pub fn insert_session(&mut self, client: ClientId, snapshot: Snapshot) -> Result<SessionKey> {
        self.client(client)?;
        if self.sessions.len() >= self.limits.sessions {
            return Err(error("RESOURCE_LIMIT", "Session limit"));
        }
        let sdk = AuthoringSession::new(snapshot);
        let event_floor = sdk.version();
        let id = event_floor.session;
        self.sessions.insert(
            id,
            SessionEntry {
                sdk,
                protected_paths: Vec::new(),
                editor_context: None,
                users: HashSet::from([client]),
                saving: None,
                events: VecDeque::new(),
                event_floor,
            },
        );
        Ok(id)
    }
    pub fn attach(&mut self, client: ClientId, session: SessionKey) -> Result<()> {
        self.client(client)?;
        self.sessions
            .get_mut(&session)
            .ok_or_else(|| error("SESSION_NOT_FOUND", "Session does not exist"))?
            .users
            .insert(client);
        Ok(())
    }
    pub fn session_read(&self, client: ClientId, session: SessionKey) -> Result<&AuthoringSession> {
        Ok(&self.session(client, session)?.sdk)
    }
    pub fn read_snapshot(&self, client: ClientId, session: SessionKey) -> Result<SourcedSnapshot> {
        Ok(self.session(client, session)?.sdk.read_snapshot())
    }
    pub fn begin_draft(&mut self, client: ClientId, expected: Version) -> Result<DraftSource> {
        let entry = self.source(client, expected)?;
        if self.drafts.len() >= self.limits.drafts {
            return Err(error("RESOURCE_LIMIT", "Draft limit"));
        }
        let draft = entry.sdk.begin_draft();
        let source = draft.source();
        self.drafts.insert(
            source.draft,
            DraftEntry {
                owner: client,
                source,
                state: DraftState::Ready(draft),
            },
        );
        Ok(source)
    }
    fn draft(&self, client: ClientId, id: DraftId) -> Result<&DraftEntry> {
        self.client(client)?;
        let entry = self
            .drafts
            .get(&id)
            .ok_or_else(|| error("DRAFT_ENDED", "Draft has ended"))?;
        if entry.owner != client {
            return Err(error("NOT_OWNER", "Draft belongs to another client"));
        }
        Ok(entry)
    }
    fn admit_draft(&self, client: ClientId, id: DraftId) -> Result<()> {
        let entry = self.draft(client, id)?;
        self.writable(client, entry.source.base.session)?;
        if matches!(entry.state, DraftState::Working(_)) {
            return Err(error("DRAFT_BUSY", "Worker owns this Draft"));
        }
        Ok(())
    }
    pub fn read_draft(&self, client: ClientId, expected: DraftSource) -> Result<SourcedSnapshot> {
        let entry = self.draft(client, expected.draft)?;
        if entry.source != expected {
            return Err(error("STALE_SOURCE", "Candidate source is stale"));
        }
        match &entry.state {
            DraftState::Ready(draft) => draft.read_snapshot(),
            DraftState::Working(_) => Err(error("DRAFT_BUSY", "Worker owns this Draft")),
        }
    }
    // Consume only after ownership and busy admission. All accepted command /
    // prepare failures end the candidate; another client's request cannot end it.
    fn take_draft(&mut self, client: ClientId, expected: DraftSource) -> Result<Draft> {
        self.admit_draft(client, expected.draft)?;
        let entry = self.drafts.remove(&expected.draft).unwrap();
        let DraftState::Ready(draft) = entry.state else {
            unreachable!()
        };
        if entry.source != expected {
            return Err(error("STALE_SOURCE", "Candidate source is stale"));
        }
        self.source(client, expected.base)?;
        Ok(draft)
    }
    pub fn batch<T>(
        &mut self,
        client: ClientId,
        expected: DraftSource,
        run: impl FnOnce(&mut Commands<'_>) -> Result<T>,
    ) -> Result<(DraftSource, T)> {
        let mut draft = self.take_draft(client, expected)?;
        let value =
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| draft.batch(expected, run)))
                .unwrap_or_else(|_| Err(error("COMMAND_PANIC", "Draft command panicked")))?;
        let source = draft.source();
        self.drafts.insert(
            source.draft,
            DraftEntry {
                owner: client,
                source,
                state: DraftState::Ready(draft),
            },
        );
        Ok((source, value))
    }
    pub fn publish(
        &mut self,
        client: ClientId,
        expected: DraftSource,
        description: impl Into<String>,
    ) -> Result<Publication> {
        let draft = self.take_draft(client, expected)?;
        let prepared = draft.prepare(description)?;
        let receipt = self
            .sessions
            .get_mut(&expected.base.session)
            .unwrap()
            .sdk
            .publish(prepared)?;
        self.record_publications(expected.base.session, client);
        Ok(receipt)
    }
    pub fn preview<T>(
        &mut self,
        client: ClientId,
        expected: DraftSource,
        run: impl FnOnce(&mut Commands<'_>) -> Result<T>,
    ) -> Result<(DraftSource, T)> {
        self.admit_draft(client, expected.draft)?;
        self.source(client, expected.base)?;
        let entry = self.drafts.get_mut(&expected.draft).unwrap();
        let DraftState::Ready(draft) = &mut entry.state else {
            unreachable!()
        };
        let (prepared, value) = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            draft.prepare_preview(expected, run)
        }))
        .unwrap_or_else(|_| Err(error("PREVIEW_PANIC", "Preview preparation panicked")))?;
        entry.source = draft.install_preview(prepared)?;
        Ok((entry.source, value))
    }
    pub fn cancel_draft(&mut self, client: ClientId, draft: DraftId) -> Result<()> {
        self.draft(client, draft)?;
        if let DraftState::Working(job) = self.drafts.remove(&draft).unwrap().state {
            self.cancel_job(client, job)?;
        }
        Ok(())
    }
    pub fn undo(&mut self, client: ClientId, expected: Version) -> Result<Publication> {
        self.source(client, expected)?;
        let receipt = self
            .sessions
            .get_mut(&expected.session)
            .unwrap()
            .sdk
            .undo(expected)?;
        self.record_publications(expected.session, client);
        Ok(receipt)
    }
    pub fn redo(&mut self, client: ClientId, expected: Version) -> Result<Publication> {
        self.source(client, expected)?;
        let receipt = self
            .sessions
            .get_mut(&expected.session)
            .unwrap()
            .sdk
            .redo(expected)?;
        self.record_publications(expected.session, client);
        Ok(receipt)
    }
    pub fn replace(
        &mut self,
        client: ClientId,
        expected: Version,
        snapshot: Snapshot,
    ) -> Result<Publication> {
        self.source(client, expected)?;
        let publication = self
            .sessions
            .get_mut(&expected.session)
            .unwrap()
            .sdk
            .replace(expected, snapshot)?;
        self.paths.retain(|_, owner| *owner != expected.session);
        self.record_publications(expected.session, client);
        Ok(publication)
    }
    fn record_publications(&mut self, session: SessionKey, client: ClientId) {
        let entry = self.sessions.get_mut(&session).unwrap();
        entry
            .events
            .extend(
                entry
                    .sdk
                    .take_events()
                    .into_iter()
                    .map(|publication| PublicationEvent {
                        client,
                        publication,
                    }),
            );
        while entry.events.len() > self.limits.events_per_session {
            entry.event_floor = entry.events.pop_front().unwrap().publication.version;
        }
    }
    pub fn subscribe(&self, client: ClientId, session: SessionKey) -> Result<SessionSubscription> {
        let entry = self.session(client, session)?;
        Ok(SessionSubscription {
            snapshot: entry.sdk.read_snapshot(),
            history: entry.sdk.history_state(),
            cursor: entry.sdk.version(),
        })
    }
    /// Bounded publication receipts. A slow consumer must resubscribe after a
    /// gap; no client can drain another client's notifications.
    pub fn publications_since(
        &self,
        client: ClientId,
        cursor: Version,
    ) -> Result<Vec<Publication>> {
        Ok(self
            .publication_events_since(client, cursor)?
            .into_iter()
            .map(|event| event.publication)
            .collect())
    }
    pub fn publication_events_since(
        &self,
        client: ClientId,
        cursor: Version,
    ) -> Result<Vec<PublicationEvent>> {
        let entry = self.session(client, cursor.session)?;
        if cursor.revision < entry.event_floor.revision {
            return Err(error(
                "EVENT_GAP",
                "Publication cursor was evicted; resubscribe",
            ));
        }
        if cursor != entry.event_floor
            && !entry.events.iter().any(|e| e.publication.version == cursor)
        {
            return Err(error("INVALID_CURSOR", "Unknown publication cursor"));
        }
        Ok(entry
            .events
            .iter()
            .filter(|e| e.publication.version.revision > cursor.revision)
            .cloned()
            .collect())
    }
    fn admit_job(&self, budget: TaskBudget) -> Result<Leases> {
        if self.jobs.len() >= self.limits.jobs {
            return Err(error("RESOURCE_LIMIT", "Job slots full"));
        }
        if budget.result_bytes > self.limits.result_bytes {
            return Err(error("RESOURCE_LIMIT", "Result bound exceeds host limit"));
        }
        let workspace = Arc::new(self.budget.reserve_transfer(budget.workspace_bytes)?);
        let result = Arc::new(self.budget.reserve_transfer(budget.result_bytes)?);
        Ok((workspace, result))
    }
    fn enqueue(
        &mut self,
        client: ClientId,
        source: QuerySource,
        task: Task,
        leases: Leases,
        target: Option<PathBuf>,
    ) -> JobId {
        let id = JobId::new();
        let cancel = Cancellation::default();
        let kind = task.kind();
        if let Some(target) = &target {
            self.reservations.insert(target.clone(), id);
        }
        self.jobs.insert(
            id,
            JobEntry {
                info: JobInfo {
                    id,
                    client,
                    source,
                    state: JobState::Queued,
                    result: None,
                },
                cancel: cancel.clone(),
                kind,
                entered: false,
                target,
            },
        );
        self.queue.push_back(Mutex::new(Work {
            identity: Identity {
                job: id,
                client,
                source,
            },
            task,
            cancel,
            workspace: leases.0,
            result_lease: leases.1,
        }));
        id
    }
    pub fn request_batch(
        &mut self,
        client: ClientId,
        expected: DraftSource,
        budget: TaskBudget,
        run: impl FnOnce(&mut Commands<'_>, &Cancellation) -> Result<()> + Send + 'static,
    ) -> Result<JobId> {
        self.admit_draft(client, expected.draft)?;
        let leases = self.admit_job(budget)?;
        let draft = self.take_draft(client, expected)?;
        let run: Batch = Box::new(run);
        let job = self.enqueue(
            client,
            QuerySource::Draft(expected),
            Task::Batch { draft, run },
            leases,
            None,
        );
        self.drafts.insert(
            expected.draft,
            DraftEntry {
                owner: client,
                source: expected,
                state: DraftState::Working(job),
            },
        );
        Ok(job)
    }
    pub fn request_publish(
        &mut self,
        client: ClientId,
        expected: DraftSource,
        description: impl Into<String>,
        budget: TaskBudget,
    ) -> Result<JobId> {
        self.admit_draft(client, expected.draft)?;
        let leases = self.admit_job(budget)?;
        let draft = self.take_draft(client, expected)?;
        Ok(self.enqueue(
            client,
            QuerySource::Draft(expected),
            Task::Publish {
                draft,
                description: description.into(),
            },
            leases,
            None,
        ))
    }
    pub fn request_read(
        &mut self,
        client: ClientId,
        source: QuerySource,
        budget: TaskBudget,
        run: impl FnOnce(SourcedSnapshot, &Cancellation) -> Result<Vec<u8>> + Send + 'static,
    ) -> Result<JobId> {
        let snapshot = self.capture(client, source)?;
        let leases = self.admit_job(budget)?;
        let run: Read = Box::new(run);
        Ok(self.enqueue(
            client,
            source,
            Task::Read {
                snapshot,
                run,
                max_bytes: budget.result_bytes,
            },
            leases,
            None,
        ))
    }
    fn capture(&self, client: ClientId, source: QuerySource) -> Result<SourcedSnapshot> {
        Ok(match source {
            QuerySource::Published(expected) => {
                let entry = self.session(client, expected.session)?;
                if entry.sdk.version() != expected {
                    return Err(error("STALE_SOURCE", "Read source is stale"));
                }
                entry.sdk.read_snapshot()
            }
            QuerySource::Draft(expected) => self.read_draft(client, expected)?,
        })
    }
    pub fn request_read_steps(
        &mut self,
        client: ClientId,
        source: QuerySource,
        budget: TaskBudget,
        run: impl FnMut(&SourcedSnapshot, &Cancellation) -> Result<ReadProgress> + Send + 'static,
    ) -> Result<JobId> {
        let snapshot = self.capture(client, source)?;
        let leases = self.admit_job(budget)?;
        Ok(self.enqueue(
            client,
            source,
            Task::ReadSteps {
                snapshot,
                run: Box::new(run),
                max_bytes: budget.result_bytes,
            },
            leases,
            None,
        ))
    }
    fn target(&self, path: &Path, session: SessionKey) -> Result<()> {
        if self.paths.get(path).is_some_and(|owner| *owner != session)
            || self
                .reservations
                .keys()
                .any(|reserved| path.starts_with(reserved) || reserved.starts_with(path))
        {
            Err(error("PROJECT_BUSY", "Project path is owned or reserved"))
        } else {
            Ok(())
        }
    }
    pub fn request_open(
        &mut self,
        client: ClientId,
        expected: Version,
        io: &ProjectIo,
        path: &Path,
        budget: TaskBudget,
    ) -> Result<JobId> {
        self.source(client, expected)?;
        let path = io.normalize_path(path)?;
        self.target(&path, expected.session)?;
        let leases = self.admit_job(budget)?;
        let mut io = io.clone();
        io.set_open_memory(Some(leases.0.clone()));
        Ok(self.enqueue(
            client,
            QuerySource::Published(expected),
            Task::Open {
                io,
                path: path.clone(),
            },
            leases,
            Some(path),
        ))
    }
    pub fn request_save(
        &mut self,
        client: ClientId,
        expected: Version,
        io: &ProjectIo,
        budget: TaskBudget,
    ) -> Result<JobId> {
        let path = self
            .source(client, expected)?
            .sdk
            .project()
            .ok_or_else(|| error("NO_PROJECT_PATH", "Use save_as for a new document"))?
            .path()
            .to_owned();
        self.request_save_as(client, expected, io, &path, None, budget)
    }
    pub fn request_save_as(
        &mut self,
        client: ClientId,
        expected: Version,
        io: &ProjectIo,
        path: &Path,
        fingerprint: Option<&ProjectFingerprint>,
        budget: TaskBudget,
    ) -> Result<JobId> {
        let entry = self.source(client, expected)?;
        let target = io.normalize_path(path)?;
        if entry.protected_paths.iter().any(|source| source == &target) {
            return Err(error(
                "INVALID_DESTINATION",
                "Save would overwrite an imported source",
            ));
        }
        // Do not let an entered mutation finish during external publication.
        if self.jobs.values().any(|j| {
            !j.info.state.is_terminal()
                && j.info.source.session() == expected.session
                && j.kind != TaskKind::Read
        }) {
            return Err(error("PROJECT_BUSY", "Mutation work is outstanding"));
        }
        let path = io.normalize_path(path)?;
        self.target(&path, expected.session)?;
        let leases = self.admit_job(budget)?;
        let prepared = self
            .sessions
            .get_mut(&expected.session)
            .unwrap()
            .sdk
            .prepare_save_as(io, &path, fingerprint)?;
        let job = self.enqueue(
            client,
            QuerySource::Published(expected),
            Task::Save(prepared),
            leases,
            Some(path),
        );
        self.sessions.get_mut(&expected.session).unwrap().saving = Some(job);
        Ok(job)
    }
    pub fn next_work(&mut self) -> Option<Work> {
        if self
            .jobs
            .values()
            .filter(|j| j.entered && !j.info.state.is_terminal())
            .count()
            >= self.limits.running_jobs
        {
            return None;
        }
        let work = self
            .queue
            .pop_front()?
            .into_inner()
            .unwrap_or_else(|p| p.into_inner());
        let entry = self.jobs.get_mut(&work.identity.job).unwrap();
        entry.entered = true;
        if !entry.cancel.is_cancelled() {
            entry.info.state = JobState::Running;
        }
        Some(work)
    }
    pub fn dispatch(&mut self, executor: &mut impl Executor) {
        while let Some(work) = self.next_work() {
            if let Err(work) = executor.submit(work) {
                let entry = self.jobs.get_mut(&work.identity.job).unwrap();
                entry.entered = false;
                if !entry.cancel.is_cancelled() {
                    entry.info.state = JobState::Queued;
                }
                self.queue.push_front(Mutex::new(*work));
                break;
            }
        }
    }
    pub fn job(&self, client: ClientId, job: JobId) -> Result<&JobInfo> {
        self.client(client)?;
        let entry = self
            .jobs
            .get(&job)
            .ok_or_else(|| error("JOB_NOT_FOUND", "Job does not exist"))?;
        if entry.info.client != client {
            return Err(error("NOT_OWNER", "Job belongs to another client"));
        }
        Ok(&entry.info)
    }
    pub fn cancel_job(&mut self, client: ClientId, job: JobId) -> Result<()> {
        self.job(client, job)?;
        let entry = self.jobs.get_mut(&job).unwrap();
        if !entry.info.state.is_terminal() {
            entry.cancel.cancel();
            entry.info.state = JobState::Cancelling;
        }
        Ok(())
    }
    pub fn release_job(&mut self, client: ClientId, job: JobId) -> Result<()> {
        if !self.job(client, job)?.state.is_terminal() {
            return Err(error("JOB_BUSY", "Completion is outstanding"));
        }
        self.jobs.remove(&job);
        Ok(())
    }
    pub fn release_client(&mut self, client: ClientId) {
        self.blobs.retain(|_, blob| blob.owner != client);
        self.clients.remove(&client);
        self.drafts.retain(|_, d| d.owner != client);
        for entry in self.sessions.values_mut() {
            entry.users.remove(&client);
        }
        self.jobs.retain(|_, entry| {
            if entry.info.client != client {
                return true;
            }
            if entry.info.state.is_terminal() {
                return false;
            }
            entry.cancel.cancel();
            entry.info.state = JobState::Cancelling;
            true
        });
    }
    pub fn close_session(
        &mut self,
        client: ClientId,
        session: SessionKey,
        discard_unsaved: bool,
    ) -> Result<()> {
        let entry = self.session(client, session)?;
        if entry.users.len() > 1 {
            return Err(error("SESSION_IN_USE", "Other clients are attached"));
        }
        if self
            .jobs
            .values()
            .any(|j| !j.info.state.is_terminal() && j.info.source.session() == session)
        {
            return Err(error("SESSION_BUSY", "Jobs are outstanding"));
        }
        if entry.sdk.is_dirty() && !discard_unsaved {
            return Err(error("UNSAVED_CHANGES", "Session has unsaved content"));
        }
        self.sessions.remove(&session);
        self.drafts.retain(|_, d| d.source.base.session != session);
        self.paths.retain(|_, owner| *owner != session);
        Ok(())
    }
    pub fn task_bytes(&self) -> usize {
        self.budget.usage().transfer_bytes
    }

    /// Only the owner thread installs prepared content. Saving is special: a
    /// successful external publication wins over late cancellation/detachment.
    /// A completion delivered to the wrong host is returned intact for rerouting.
    pub fn complete(&mut self, completion: Completion) -> std::result::Result<(), Completion> {
        let identity = completion.identity;
        let Some(entry) = self.jobs.get(&identity.job) else {
            return Err(completion);
        };
        if !entry.entered
            || entry.info.state.is_terminal()
            || entry.info.client != identity.client
            || entry.info.source != identity.source
        {
            return Err(completion);
        }
        let mut entry = self.jobs.remove(&identity.job).unwrap();
        if let Ok(Output::Yield(work)) = completion.outcome {
            entry.entered = false;
            entry.info.state = if entry.cancel.is_cancelled() {
                JobState::Cancelling
            } else {
                JobState::Queued
            };
            self.jobs.insert(identity.job, entry);
            // Even a cancelled/detached continuation returns to a worker. Its
            // potentially large private buffers are never destroyed on the owner.
            self.queue.push_back(Mutex::new(*work));
            return Ok(());
        }
        let outcome = self.install(identity, &entry, completion.outcome);
        self.record_publications(identity.base().session, identity.client);
        if let QuerySource::Draft(source) = identity.source {
            if self
                .drafts
                .get(&source.draft)
                .is_some_and(|d| matches!(d.state, DraftState::Working(job) if job == identity.job))
            {
                self.drafts.remove(&source.draft);
            }
        }
        if entry.kind == TaskKind::Save {
            self.sessions
                .get_mut(&identity.base().session)
                .unwrap()
                .saving = None;
        }
        if let Some(path) = &entry.target {
            self.reservations.remove(path);
        }
        entry.info.state = match &outcome {
            Ok(_) => JobState::Succeeded,
            Err(e) if e.code == "CANCELLED" => JobState::Cancelled,
            Err(_) => JobState::Failed,
        };
        entry.info.result = Some(outcome);
        if self.clients.contains(&identity.client) {
            self.jobs.insert(identity.job, entry);
        }
        Ok(())
    }
    fn install(
        &mut self,
        identity: Identity,
        entry: &JobEntry,
        outcome: Result<Output>,
    ) -> Result<JobValue> {
        let output = outcome?;
        let session = identity.base().session;
        if let Output::Save(completed) = output {
            // Work/Completion fields are private; this receipt is tied to the
            // admitted job and SDK permit, with a resident, exclusive Session.
            let receipt = self
                .sessions
                .get_mut(&session)
                .unwrap()
                .sdk
                .complete_save(&completed)?;
            self.paths.retain(|_, owner| *owner != session);
            self.paths.insert(receipt.path.clone(), session);
            return Ok(JobValue::Saved(receipt));
        }
        if let Output::Export(receipt) = output {
            return Ok(JobValue::Exported(receipt));
        }
        entry.cancel.check()?;
        self.client(identity.client)?;
        if let Output::Read(value) = output {
            return Ok(JobValue::Read(value));
        }
        self.source(identity.client, identity.base())?;
        match output {
            Output::Draft(draft) => {
                let QuerySource::Draft(source) = identity.source else {
                    unreachable!()
                };
                let expected = self.draft(identity.client, source.draft)?;
                if expected.source != source
                    || !matches!(expected.state, DraftState::Working(job) if job == identity.job)
                    || source.next_sequence() != Some(draft.source())
                {
                    return Err(error("STALE_SOURCE", "Draft worker source is stale"));
                }
                let source = draft.source();
                self.drafts.insert(
                    source.draft,
                    DraftEntry {
                        owner: identity.client,
                        source,
                        state: DraftState::Ready(draft),
                    },
                );
                Ok(JobValue::DraftUpdated(source))
            }
            Output::Publish(prepared) => {
                if identity.source != QuerySource::Draft(prepared.source()) {
                    return Err(error("STALE_SOURCE", "Prepared source is stale"));
                }
                self.sessions
                    .get_mut(&session)
                    .unwrap()
                    .sdk
                    .publish(prepared)
                    .map(JobValue::Published)
            }
            Output::Open(prepared) => {
                let receipt = self
                    .sessions
                    .get_mut(&session)
                    .unwrap()
                    .sdk
                    .publish_loaded(prepared)?;
                self.paths.retain(|_, owner| *owner != session);
                self.paths.insert(entry.target.clone().unwrap(), session);
                self.sessions
                    .get_mut(&session)
                    .unwrap()
                    .protected_paths
                    .clear();
                Ok(JobValue::Opened(receipt))
            }
            Output::Import(prepared) => {
                let hosted = self.sessions.get_mut(&session).unwrap();
                let publication = hosted.sdk.replace(identity.base(), prepared.snapshot)?;
                hosted.protected_paths = prepared.protected_paths;
                self.paths.retain(|_, owner| *owner != session);
                Ok(JobValue::Imported(ImportReceipt {
                    publication,
                    warnings: prepared.warnings,
                    diagnostics: prepared.diagnostics,
                    name: prepared.name,
                }))
            }
            Output::Read(_) | Output::Save(_) | Output::Export(_) | Output::Yield(_) => {
                unreachable!()
            }
        }
    }
}
trait SourceSession {
    fn session(self) -> SessionKey;
}
impl SourceSession for QuerySource {
    fn session(self) -> SessionKey {
        match self {
            Self::Published(v) => v.session,
            Self::Draft(d) => d.base.session,
        }
    }
}

#[cfg(test)]
#[path = "resident_tests.rs"]
mod tests;
