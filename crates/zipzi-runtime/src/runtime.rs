mod editor;
pub use editor::CandidateWork;
mod authoring;
mod binary_authoring;
mod blobs;
mod collections;
mod mesh_read;
pub use binary_authoring::AUTHORING_MAGIC;
pub use collections::{
    CollectionDomain, CollectionPage, CollectionQuery, ObjectSummary, RuntimeCapabilities,
    Selection,
};
mod modeling;
mod modeling_read;
mod read_jobs;
pub use modeling_read::{ModelingRecordBatch, ModelingRecordBlob, ModelingRecordResponse};
mod addresses;
mod references;
pub(crate) mod resources;
pub use references::{LabelAddressInfo, ObjectRefInfo};
pub(crate) mod previews;
mod sampling;
mod typed;
use crate::events::EventLog;
use crate::jobs::Task;
use crate::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use zipzi_core::{Canvas, Document, ModelSetup, Vec2};
use zipzi_sdk::{AuthoringSession, AuthoringSnapshot, EditReceipt, EditSession, Version};

struct SessionEntry {
    info: SessionInfo,
    sdk: Option<AuthoringSession>,
    frozen: Option<Arc<AuthoringSnapshot>>,
    users: HashSet<ClientId>,
    events: EventLog,
    provisional: bool,
    interaction: Option<ClientId>,
    /// Charged while the session belongs to a worker as well as while Ready.
    content_bytes: usize,
    history_bytes: usize,
}
struct EditEntry {
    pending_upload: Option<BlobId>,
    session: SessionId,
    client: ClientId,
    base: Version,
    sequence: u64,
    state: EditState,
    sdk: Option<EditSession<'static>>,
    bytes: usize,
}
struct JobEntry {
    info: JobInfo,
    base: Version,
    cancel: Arc<AtomicBool>,
    target: Option<PathBuf>,
    /// Pending commit candidates remain budgeted after leaving the edit table.
    candidate_bytes: usize,
    publication_bytes: usize,
    read_source: Option<SourceSnapshot>,
    read_bytes: usize,
    read_only: bool,
    plan_result: bool,
}

pub struct SessionSubscription {
    pub info: SessionInfo,
    pub snapshot: SourceSnapshot,
    pub cursor: EventCursor,
}
pub struct SessionEventSubscription {
    pub info: SessionInfo,
    pub cursor: EventCursor,
}
pub struct RegistrySubscription {
    pub sessions: Vec<SessionInfo>,
    pub cursor: EventCursor,
}

pub struct Runtime {
    previews: HashMap<PreviewId, previews::PreviewEntry>,
    editor_contexts: HashMap<SessionId, crate::editor_context::EditorContext>,
    #[cfg(feature = "test-support")]
    control: Arc<crate::test_support::Control>,
    instance: RuntimeInstanceId,
    next_id: u64,
    limits: RuntimeLimits,
    clients: HashSet<ClientId>,
    sessions: HashMap<SessionId, SessionEntry>,
    paths: HashMap<PathBuf, SessionId>,
    reservations: HashMap<PathBuf, JobId>,
    edits: HashMap<EditId, EditEntry>,
    jobs: HashMap<JobId, JobEntry>,
    queue: VecDeque<Work>,
    registry: EventLog,
    image_budget: zipzi_project::ImageMemoryBudget,
    plans: HashMap<PlanId, modeling::PlanEntry>,
    prepared_images: HashMap<ImageId, resources::ImageEntry>,
    object_refs: HashMap<ObjectRefId, references::RefEntry>,
    field_addresses: HashMap<FieldAddressId, references::AddressEntry>,
    blobs: HashMap<BlobId, blobs::BlobEntry>,
    external_bytes: usize,
}
impl Default for Runtime {
    fn default() -> Self {
        Self::new(RuntimeLimits::default())
    }
}
impl Runtime {
    pub fn new(limits: RuntimeLimits) -> Self {
        let instance = RuntimeInstanceId(uuid::Uuid::new_v4());
        let registry = EventLog::new(
            instance,
            None,
            limits.registry_events,
            limits.registry_event_bytes,
        );
        let image_budget = zipzi_project::ImageMemoryBudget::new(limits.image_bytes);
        image_budget.bind_host_budget(0, limits.accounted_bytes);
        Self {
            previews: HashMap::new(),
            #[cfg(feature = "test-support")]
            control: Arc::default(),
            image_budget,
            external_bytes: 0,
            editor_contexts: HashMap::new(),
            plans: HashMap::new(),
            prepared_images: HashMap::new(),
            object_refs: HashMap::new(),
            field_addresses: HashMap::new(),
            blobs: HashMap::new(),
            instance,
            next_id: 0,
            limits,
            clients: HashSet::new(),
            sessions: HashMap::new(),
            paths: HashMap::new(),
            reservations: HashMap::new(),
            edits: HashMap::new(),
            jobs: HashMap::new(),
            queue: VecDeque::new(),
            registry,
        }
    }
    /// Coalesced wake hint for short in-process host calls. This is not an event
    /// cursor or a content Version; consumers still poll the bounded streams.
    pub fn host_event_stamp(&self) -> (EventCursor, u64) {
        (
            self.registry.cursor(),
            self.sessions.values().fold(0u64, |total, entry| {
                total.wrapping_add(entry.events.cursor().sequence)
            }),
        )
    }
    /// Live table counts for host diagnostics; byte values are CPU estimates.
    pub fn live_counts(&self) -> (usize, usize, usize, usize) {
        (
            self.clients.len(),
            self.sessions.len(),
            self.edits.len(),
            self.jobs.len(),
        )
    }
    #[cfg(feature = "test-support")]
    pub fn test_controller(&self) -> Arc<crate::test_support::Control> {
        self.control.clone()
    }
    pub fn image_memory_usage(&self) -> zipzi_project::ImageMemoryUsage {
        self.image_budget.usage()
    }
    pub fn refresh_memory_budget(&self) {
        self.image_budget
            .bind_host_budget(self.non_resource_bytes(), self.limits.accounted_bytes);
    }
    pub fn transport_budget(&self) -> zipzi_project::ImageMemoryBudget {
        self.refresh_memory_budget();
        self.image_budget.clone()
    }
    pub fn set_external_bytes(&mut self, bytes: usize) {
        self.external_bytes = bytes;
        self.refresh_memory_budget();
    }
    pub fn external_bytes_fit(&self, bytes: usize) -> bool {
        self.accounted_bytes()
            .saturating_sub(self.external_bytes)
            .saturating_add(bytes)
            <= self.limits.accounted_bytes
    }
    pub fn collect_image_resources(&self) -> usize {
        self.sessions
            .values()
            .filter_map(|s| s.sdk.as_ref())
            .map(|sdk| sdk.collect_image_resources())
            .sum()
    }
    pub fn image_memory_budget(&self) -> &zipzi_project::ImageMemoryBudget {
        &self.image_budget
    }
    pub fn instance_id(&self) -> RuntimeInstanceId {
        self.instance
    }
    pub fn limits(&self) -> &RuntimeLimits {
        &self.limits
    }
    fn serial(&mut self) -> u64 {
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("runtime identity exhausted");
        self.next_id
    }
    pub fn register_client(&mut self) -> Result<ClientId> {
        if self.clients.len() >= self.limits.clients {
            return Err(RuntimeError::Budget("clients"));
        }
        self.check_bytes(512, 0, 0)?;
        let id = ClientId {
            runtime: self.instance,
            serial: self.serial(),
        };
        self.clients.insert(id);
        Ok(id)
    }
    fn client(&self, client: ClientId) -> Result<()> {
        self.clients
            .contains(&client)
            .then_some(())
            .ok_or(RuntimeError::InvalidReference)
    }
    /// Explicit client expiry; transport grace periods belong to R3. Committing
    /// jobs and their results survive, ordinary unsubmitted candidates do not.
    pub fn release_client(&mut self, client: ClientId) {
        for entry in self.previews.values().filter(|p| p.client == client) {
            if let Some(job) = entry.busy {
                if let Some(job) = self.jobs.get(&job) {
                    job.cancel.store(true, Ordering::Release);
                }
            }
        }
        self.previews.retain(|_, p| p.client != client);
        self.clients.remove(&client);
        self.object_refs.retain(|_, r| r.client != client);
        self.field_addresses.retain(|_, r| r.client != client);
        self.plans.retain(|_, p| p.client != client);
        self.prepared_images
            .retain(|_, image| image.client != client);
        self.blobs.retain(|_, b| b.client != client);
        self.edits.retain(|_, edit| edit.client != client);
        self.jobs
            .retain(|_, job| job.info.client != client || !job.info.state.is_terminal());
        for session in self.sessions.values_mut() {
            if let Some(sdk) = &session.sdk {
                sdk.collect_image_resources();
            }
            session.users.remove(&client);
            if session.interaction == Some(client) {
                session.interaction = None;
            }
        }
    }
    pub fn attach_session(&mut self, client: ClientId, session: SessionId) -> Result<()> {
        self.client(client)?;
        self.entry_mut(session)?.users.insert(client);
        Ok(())
    }
    pub fn detach_session(&mut self, client: ClientId, session: SessionId) -> Result<()> {
        self.client(client)?;
        let entry = self.entry(session)?;
        if !entry.users.contains(&client) {
            return Err(RuntimeError::WrongOwner);
        }
        if self
            .edits
            .values()
            .any(|e| e.session == session && e.client == client)
            || self.jobs.values().any(|j| {
                j.info.session == session && j.info.client == client && !j.info.state.is_terminal()
            })
        {
            return Err(RuntimeError::Busy);
        }
        self.object_refs
            .retain(|_, r| r.session != session || r.client != client);
        self.previews
            .retain(|_, p| p.source.source.session() != session || p.client != client);
        self.field_addresses
            .retain(|_, r| r.session != session || r.client != client);
        self.plans
            .retain(|_, p| p.source.source.session() != session || p.client != client);
        self.entry_mut(session)?.users.remove(&client);
        Ok(())
    }
    fn entry(&self, id: SessionId) -> Result<&SessionEntry> {
        self.sessions.get(&id).ok_or(RuntimeError::InvalidReference)
    }
    fn entry_mut(&mut self, id: SessionId) -> Result<&mut SessionEntry> {
        self.sessions
            .get_mut(&id)
            .ok_or(RuntimeError::InvalidReference)
    }
    pub fn session_info(&self, id: SessionId) -> Result<SessionInfo> {
        Ok(self.entry(id)?.info.clone())
    }
    pub fn list_sessions(&self) -> Vec<SessionInfo> {
        let mut sessions: Vec<_> = self
            .sessions
            .values()
            .filter(|e| !e.provisional)
            .map(|e| e.info.clone())
            .collect();
        sessions.sort_by_key(|e| e.id);
        sessions
    }
    fn ready(&self, id: SessionId) -> Result<&AuthoringSession> {
        let entry = self.entry(id)?;
        match entry.info.state {
            SessionState::Ready => Ok(entry.sdk.as_ref().expect("ready session")),
            SessionState::Faulted(_) => Err(RuntimeError::Faulted),
            _ => Err(RuntimeError::Busy),
        }
    }
    /// No mutable SDK session is exposed. This borrow reads the sole formal
    /// instance directly and can serve Editor projections without a full clone.
    pub fn session(&self, id: SessionId) -> Result<&AuthoringSession> {
        self.ready(id)
    }
    pub fn document(&self, id: SessionId) -> Result<&Document> {
        Ok(self.ready(id)?.document())
    }
    pub fn create_session(
        &mut self,
        client: ClientId,
        document_id: &str,
        setup: impl Into<ModelSetup>,
    ) -> Result<SessionId> {
        self.adopt_session(client, AuthoringSession::new(document_id, setup)?)
    }
    /// Moves an existing standalone instance into the host; ownership cannot be
    /// duplicated. Initial queued SDK receipts are consumed only by Runtime.
    pub fn adopt_session(&mut self, client: ClientId, sdk: AuthoringSession) -> Result<SessionId> {
        self.insert_session(client, sdk, false)
    }
    fn insert_session(
        &mut self,
        client: ClientId,
        mut sdk: AuthoringSession,
        provisional: bool,
    ) -> Result<SessionId> {
        self.client(client)?;
        if self.sessions.len() >= self.limits.sessions {
            return Err(RuntimeError::Budget("sessions"));
        }
        let path = sdk.project_path().map(identity).transpose()?;
        if path
            .as_ref()
            .is_some_and(|p| self.paths.contains_key(p) || self.reservations.contains_key(p))
        {
            return Err(RuntimeError::Busy);
        }
        let bytes = sdk
            .estimated_content_bytes()
            .saturating_add(sdk.history_state().estimated_bytes);
        let image_growth = if sdk.image_memory_budget().same_pool(&self.image_budget) {
            0
        } else {
            sdk.image_memory_budget().usage().total_bytes()
        };
        self.check_bytes(bytes.saturating_add(image_growth), 0, 0)?;
        if self
            .history_bytes()
            .saturating_add(sdk.history_state().estimated_bytes)
            > self.limits.history_bytes
        {
            return Err(RuntimeError::Budget("history"));
        }
        sdk.set_image_memory_budget(self.image_budget.clone())?;
        let id = SessionId {
            runtime: self.instance,
            serial: self.serial(),
        };
        let info = SessionInfo {
            id,
            state: SessionState::Ready,
            version: sdk.version(),
            modified: sdk.modified(),
            history_lengths: sdk.history_lengths(),
            path: path.clone(),
            fault: None,
        };
        // Pre-host events cannot be assigned a Runtime operation/client origin.
        sdk.drain_events();
        let entry = SessionEntry {
            content_bytes: sdk.estimated_content_bytes(),
            history_bytes: sdk.history_state().estimated_bytes,
            info: info.clone(),
            sdk: Some(sdk),
            frozen: None,
            users: HashSet::from([client]),
            events: EventLog::new(
                self.instance,
                Some(id),
                self.limits.session_events,
                self.limits.session_event_bytes,
            ),
            provisional,
            interaction: None,
        };
        self.sessions.insert(id, entry);
        if let Some(path) = path {
            self.paths.insert(path, id);
        }
        if !provisional {
            self.registry.push(
                Some(client),
                None,
                OperationId {
                    runtime: id.runtime,
                    serial: id.serial,
                },
                EventKind::Created(info),
            );
        }
        self.trim_events();
        Ok(id)
    }
    pub fn source_snapshot(&self, session: SessionId) -> Result<SourceSnapshot> {
        let entry = self.entry(session)?;
        self.refresh_memory_budget();
        let lease = Arc::new(self.reserve_transfer(entry.content_bytes.saturating_add(1024))?);
        let snapshot = if let Some(sdk) = &entry.sdk {
            Arc::new(sdk.read_snapshot())
        } else {
            entry.frozen.as_ref().ok_or(RuntimeError::Busy)?.clone()
        };
        Ok(SourceSnapshot {
            _lease: lease,
            source: SourceRef::Formal {
                session,
                version: snapshot.version(),
            },
            snapshot,
            state: entry.info.state,
        })
    }
    pub fn subscribe(&self, session: SessionId) -> Result<SessionSubscription> {
        // One owner and a shared borrow make snapshot + cursor atomic with
        // respect to publication; no callback or executor runs in this call.
        Ok(SessionSubscription {
            info: self.session_info(session)?,
            snapshot: self.source_snapshot(session)?,
            cursor: self.entry(session)?.events.cursor(),
        })
    }
    /// Capture metadata and its event boundary atomically, without cloning a model.
    pub fn subscribe_info(&self, session: SessionId) -> Result<SessionEventSubscription> {
        let entry = self.entry(session)?;
        Ok(SessionEventSubscription {
            info: entry.info.clone(),
            cursor: entry.events.cursor(),
        })
    }
    pub fn subscribe_registry(&self) -> RegistrySubscription {
        RegistrySubscription {
            sessions: self.list_sessions(),
            cursor: self.registry.cursor(),
        }
    }
    pub fn events_since(&self, cursor: EventCursor) -> Result<Vec<Arc<RuntimeEvent>>> {
        if cursor.runtime != self.instance {
            return Err(RuntimeError::ResyncRequired);
        }
        match cursor.session {
            Some(session) => self.entry(session)?.events.since(cursor),
            None => self.registry.since(cursor),
        }
    }
    /// Bound adapter expansion before constructing Python values or JSON trees.
    pub fn events_page(&self, cursor: EventCursor, limit: usize) -> Result<Vec<Arc<RuntimeEvent>>> {
        if !(1..=256).contains(&limit) {
            return Err(RuntimeError::Budget("event page count"));
        }
        let mut events = self.events_since(cursor)?;
        events.truncate(limit);
        let bytes = events
            .iter()
            .fold(0usize, |bytes, e| bytes.saturating_add(e.estimated_bytes()));
        if bytes > 1 << 20 {
            return Err(RuntimeError::Budget("event page bytes"));
        }
        self.check_bytes(bytes.saturating_mul(128), 0, 0)?;
        Ok(events)
    }
    fn edit(&self, client: ClientId, edit: EditId) -> Result<&EditEntry> {
        self.client(client)?;
        let entry = self
            .edits
            .get(&edit)
            .ok_or(RuntimeError::InvalidReference)?;
        if entry.client != client {
            return Err(RuntimeError::WrongOwner);
        }
        Ok(entry)
    }
    pub fn begin_edit(
        &mut self,
        client: ClientId,
        session: SessionId,
        label: impl Into<String>,
        expected: Option<Version>,
    ) -> Result<EditId> {
        self.client(client)?;
        let count = self.edits.values().filter(|e| e.session == session).count();
        let own = self
            .edits
            .values()
            .filter(|e| {
                e.session == session && e.client == client && e.state != EditState::Prepared
            })
            .count();
        if self.edits.len() >= self.limits.edits_total
            || count >= self.limits.edits_per_session
            || own >= self.limits.edits_per_client_session
        {
            return Err(RuntimeError::Budget("edits"));
        }
        let estimate = self
            .ready(session)?
            .estimated_content_bytes()
            .saturating_mul(2)
            .saturating_add(4096);
        self.check_bytes(estimate, 0, 0)?;
        let sdk = self.ready(session)?.begin_owned_edit(label, expected)?;
        let bytes = sdk.estimated_bytes();
        self.check_bytes(bytes, bytes, 0)?;
        let id = EditId {
            runtime: self.instance,
            serial: self.serial(),
        };
        self.edits.insert(
            id,
            EditEntry {
                pending_upload: None,
                session,
                client,
                base: sdk.base_version(),
                sequence: 0,
                state: EditState::Editing,
                sdk: Some(sdk),
                bytes,
            },
        );
        Ok(id)
    }
    pub fn edit_state(&self, client: ClientId, edit: EditId) -> Result<EditState> {
        Ok(self.edit(client, edit)?.state)
    }
    pub fn edit_sequence(&self, client: ClientId, edit: EditId) -> Result<u64> {
        Ok(self.edit(client, edit)?.sequence)
    }
    pub fn edit_document(&self, client: ClientId, edit: EditId) -> Result<&Document> {
        self.edit(client, edit)?
            .sdk
            .as_ref()
            .map(|s| s.candidate_document())
            .ok_or(RuntimeError::Aborted)
    }
    pub fn edit_snapshot(&self, client: ClientId, edit: EditId) -> Result<SourceSnapshot> {
        let entry = self.edit(client, edit)?;
        let sdk = entry.sdk.as_ref().ok_or(RuntimeError::Aborted)?;
        self.refresh_memory_budget();
        let lease = Arc::new(
            self.reserve_transfer(
                sdk.candidate_document()
                    .estimated_content_bytes()
                    .saturating_add(1024),
            )?,
        );
        let snapshot = Arc::new(self.ready(entry.session)?.candidate_snapshot(sdk)?);
        Ok(SourceSnapshot {
            _lease: lease,
            source: SourceRef::Edit {
                session: entry.session,
                edit,
                base: entry.base,
                sequence: entry.sequence,
            },
            snapshot,
            state: self.entry(entry.session)?.info.state,
        })
    }
    /// Any reported ordinary failure aborts the whole transaction. Even if the
    /// closure catches an SDK error, SDK's own aborted flag prevents commit.
    /// Budget overflow discards this candidate; formal content/history is intact.
    pub fn apply_edit<T>(
        &mut self,
        client: ClientId,
        edit: EditId,
        apply: impl FnOnce(&mut EditSession<'static>) -> std::result::Result<T, zipzi_sdk::SdkError>,
    ) -> Result<T> {
        let entry = self.edit(client, edit)?;
        if entry.state != EditState::Editing {
            return Err(RuntimeError::Aborted);
        }
        if matches!(
            self.entry(entry.session)?.info.state,
            SessionState::Faulted(_)
        ) {
            return Err(RuntimeError::Faulted);
        }
        if entry.pending_upload.is_some() {
            return Err(RuntimeError::Busy);
        }
        let old_bytes = entry.bytes;
        let entry = self.edits.get_mut(&edit).unwrap();
        let sdk = entry.sdk.as_mut().ok_or(RuntimeError::Aborted)?;
        let value = match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| apply(sdk))) {
            Ok(value) => value,
            Err(panic) => {
                entry.state = EditState::Aborted;
                entry.sdk = None;
                entry.bytes = 0;
                self.collect_image_resources();
                std::panic::resume_unwind(panic);
            }
        };
        entry.sequence = entry
            .sequence
            .checked_add(1)
            .expect("candidate sequence exhausted");
        let failed = value.is_err() || sdk.is_aborted();
        let bytes = sdk.estimated_bytes();
        let budget = self.check_bytes(
            bytes.saturating_sub(old_bytes),
            bytes.saturating_sub(old_bytes),
            0,
        );
        let entry = self.edits.get_mut(&edit).unwrap();
        if failed || budget.is_err() {
            entry.state = EditState::Aborted;
            entry.sdk = None;
            entry.bytes = 0;
        } else {
            entry.bytes = bytes;
        }
        if failed || budget.is_err() {
            self.collect_image_resources();
        }
        self.refresh_memory_budget();
        let value = value.map_err(RuntimeError::from)?;
        budget?;
        if failed {
            return Err(RuntimeError::Aborted);
        }
        Ok(value)
    }
    /// Idempotent for released tokens; a token owned by another client is denied.
    pub fn release_edit(&mut self, client: ClientId, edit: EditId) -> Result<()> {
        self.client(client)?;
        if self.edits.contains_key(&edit) {
            self.edit(client, edit)?;
            let entry = self.edits.remove(&edit).unwrap();
            if let Some(blob) = entry.pending_upload {
                self.blobs.remove(&blob);
            }
            let session = entry.session;
            if let Some(sdk) = &self.entry(session)?.sdk {
                sdk.collect_image_resources();
            }
        }
        Ok(())
    }
    fn abort_edit(&mut self, client: ClientId, edit: EditId) -> Result<()> {
        self.edit(client, edit)?;
        let entry = self.edits.get_mut(&edit).unwrap();
        entry.state = EditState::Aborted;
        entry.sdk = None;
        entry.bytes = 0;
        if let Some(blob) = entry.pending_upload.take() {
            self.blobs.remove(&blob);
        }
        self.collect_image_resources();
        Ok(())
    }
    fn take_edit(
        &mut self,
        client: ClientId,
        edit: EditId,
    ) -> Result<(SessionId, EditSession<'static>)> {
        let entry = self.edit(client, edit)?;
        self.ready(entry.session)?; // Busy never consumes the candidate.
        self.check_interaction(client, entry.session)?;
        if entry.state != EditState::Editing {
            return Err(RuntimeError::Aborted);
        }
        if entry.pending_upload.is_some() {
            return Err(RuntimeError::Busy);
        }
        let session = entry.session;
        self.check_publication_budget(session, entry.bytes)?;
        let mut entry = self.edits.remove(&edit).unwrap();
        Ok((session, entry.sdk.take().unwrap()))
    }
    pub fn commit_edit(&mut self, client: ClientId, edit: EditId) -> Result<EditReceipt> {
        let operation = edit.into();
        let (session, edit) = self.take_edit(client, edit)?;
        let result = edit.commit_to(self.entry_mut(session)?.sdk.as_mut().unwrap());
        self.refresh(session, Some(client), None, operation)?;
        Ok(result?)
    }
    pub fn undo(&mut self, client: ClientId, session: SessionId) -> Result<EditReceipt> {
        self.client(client)?;
        self.ready(session)?;
        self.check_interaction(client, session)?;
        let operation = OperationId {
            runtime: self.instance,
            serial: self.serial(),
        };
        let result = self.entry_mut(session)?.sdk.as_mut().unwrap().undo();
        self.refresh(session, Some(client), None, operation)?;
        Ok(result?)
    }
    pub fn redo(&mut self, client: ClientId, session: SessionId) -> Result<EditReceipt> {
        self.client(client)?;
        self.ready(session)?;
        self.check_interaction(client, session)?;
        let operation = OperationId {
            runtime: self.instance,
            serial: self.serial(),
        };
        let result = self.entry_mut(session)?.sdk.as_mut().unwrap().redo();
        self.refresh(session, Some(client), None, operation)?;
        Ok(result?)
    }
    fn refresh(
        &mut self,
        session: SessionId,
        client: Option<ClientId>,
        job: Option<JobId>,
        operation: OperationId,
    ) -> Result<()> {
        let entry = self.entry_mut(session)?;
        let sdk = entry.sdk.as_mut().expect("returned session");
        entry.info.version = sdk.version();
        entry.info.modified = sdk.modified();
        entry.info.history_lengths = sdk.history_lengths();
        entry.content_bytes = sdk.estimated_content_bytes();
        entry.history_bytes = sdk.history_state().estimated_bytes;
        for receipt in sdk.drain_events() {
            entry.events.push(
                client,
                job,
                operation,
                EventKind::Published(Arc::new(receipt)),
            );
        }
        self.trim_events();
        Ok(())
    }
    fn has_edits(&self, session: SessionId) -> bool {
        self.edits.values().any(|e| e.session == session)
    }
    fn exclusive(&self, session: SessionId) -> Result<()> {
        self.ready(session)?;
        if self.has_edits(session) || self.entry(session)?.interaction.is_some() {
            return Err(RuntimeError::Busy);
        }
        Ok(())
    }
    pub fn close_session(
        &mut self,
        client: ClientId,
        session: SessionId,
        discard_modified: bool,
    ) -> Result<SessionInfo> {
        self.client(client)?;
        let entry = self.entry(session)?;
        if matches!(
            entry.info.state,
            SessionState::Busy(_) | SessionState::Closing
        ) || self.has_edits(session)
            || entry.users.iter().any(|id| *id != client)
        {
            return Err(RuntimeError::Busy);
        }
        if entry.info.modified && !discard_modified {
            return Err(RuntimeError::Modified);
        }
        let pending = self
            .jobs
            .values()
            .filter(|j| j.read_only && j.info.session == session && !j.info.state.is_terminal())
            .map(|j| (j.info.id, j.info.client))
            .collect::<Vec<_>>();
        for (job, owner) in pending {
            if self.clients.contains(&owner) {
                self.cancel_job(owner, job)?;
            } else if let Some(record) = self.jobs.get_mut(&job) {
                record.cancel.store(true, Ordering::Release);
                record.info.state = JobState::Cancelling;
            }
        }
        let operation = OperationId {
            runtime: self.instance,
            serial: self.serial(),
        };
        let entry = self.entry_mut(session)?;
        entry.info.state = SessionState::Closing;
        entry.events.push(
            Some(client),
            None,
            operation,
            EventKind::StateChanged(entry.info.clone()),
        );
        self.plans
            .retain(|_, p| p.source.source.session() != session);
        self.object_refs.retain(|_, r| r.session != session);
        self.field_addresses.retain(|_, r| r.session != session);
        self.editor_contexts.remove(&session);
        self.previews
            .retain(|_, p| p.source.source.session() != session);
        let mut entry = self.sessions.remove(&session).unwrap();
        if let Some(path) = &entry.info.path {
            self.paths.remove(path);
        }
        entry.info.state = SessionState::Closed;
        self.registry.push(
            Some(client),
            None,
            operation,
            EventKind::Closed(entry.info.clone()),
        );
        Ok(entry.info)
    }
    pub fn request_open(&mut self, client: ClientId, path: &Path) -> Result<OpenRequest> {
        self.client(client)?;
        let path = identity(path)?;
        if self.reservations.contains_key(&path) {
            return Err(RuntimeError::Busy);
        }
        if let Some(&session) = self.paths.get(&path) {
            self.attach_session(client, session)?;
            return Ok(OpenRequest::Existing(session));
        }
        self.check_job_slot(client)?;
        let sdk = AuthoringSession::new(
            &uuid::Uuid::new_v4().to_string(),
            Canvas::new(1.0, 1.0, Vec2::new(0.0, 0.0), 1.0),
        )?;
        let session = self.insert_session(client, sdk, true)?;
        match self.queue_task(client, session, Task::Open(path.clone()), Some(path), 0) {
            Ok(job) => Ok(OpenRequest::Pending(job)),
            Err(error) => {
                self.sessions.remove(&session);
                Err(error)
            }
        }
    }
    pub fn request_save(
        &mut self,
        client: ClientId,
        session: SessionId,
        path: &Path,
    ) -> Result<JobId> {
        self.client(client)?;
        self.exclusive(session)?;
        let path = identity(path)?;
        if self.reservations.contains_key(&path)
            || self.paths.get(&path).is_some_and(|owner| *owner != session)
        {
            return Err(RuntimeError::Busy);
        }
        self.queue_task(client, session, Task::Save(path.clone()), Some(path), 0)
    }
    pub fn request_commit(&mut self, client: ClientId, edit: EditId) -> Result<JobId> {
        let entry = self.edit(client, edit)?;
        if entry.pending_upload.is_some() {
            return Err(RuntimeError::Busy);
        }
        let session = entry.session;
        if entry.state != EditState::Editing {
            return Err(RuntimeError::Aborted);
        }
        self.ready(session)?;
        self.check_interaction(client, session)?;
        let growth = self.check_publication_budget(session, entry.bytes)?;
        self.check_job_slot(client)?;
        let frozen = self.ready(session)?.estimated_content_bytes();
        self.check_bytes(growth.saturating_add(frozen), 0, frozen)?;
        // All admission checks precede consuming the edit. No user callback or
        // competing publication can interleave on this single-owner interface.
        let entry = self.edits.remove(&edit).unwrap();
        self.queue_task(
            client,
            session,
            Task::Commit(Box::new(entry.sdk.unwrap())),
            None,
            entry.bytes,
        )
    }
    pub fn request_undo(&mut self, client: ClientId, session: SessionId) -> Result<JobId> {
        self.client(client)?;
        self.queue_task(client, session, Task::Undo, None, 0)
    }
    pub fn request_redo(&mut self, client: ClientId, session: SessionId) -> Result<JobId> {
        self.client(client)?;
        self.queue_task(client, session, Task::Redo, None, 0)
    }
    fn check_job_slot(&self, client: ClientId) -> Result<()> {
        if self.jobs.len() >= self.limits.jobs_total
            || self
                .jobs
                .values()
                .filter(|j| j.info.client == client)
                .count()
                >= self.limits.jobs_per_client
        {
            return Err(RuntimeError::Budget("jobs and retained results"));
        }
        Ok(())
    }
    fn queue_task(
        &mut self,
        client: ClientId,
        session: SessionId,
        task: Task,
        target: Option<PathBuf>,
        candidate_bytes: usize,
    ) -> Result<JobId> {
        let transfer = match &task {
            Task::Project { _lease, .. } => Some(_lease.clone()),
            Task::Open(path) => {
                let length = std::fs::metadata(path).map_or(0, |m| m.len() as usize);
                self.refresh_memory_budget();
                Some(Arc::new(self.reserve_transfer(
                    length.saturating_mul(128).saturating_add(4096),
                )?))
            }
            Task::Import(_) => {
                self.refresh_memory_budget();
                Some(Arc::new(self.reserve_transfer(256 << 20)?))
            }
            Task::Save(_) => {
                self.refresh_memory_budget();
                Some(Arc::new(
                    self.reserve_transfer(
                        self.ready(session)?
                            .estimated_content_bytes()
                            .saturating_mul(64)
                            .saturating_add(4096),
                    )?,
                ))
            }
            _ => None,
        };
        self.check_interaction(client, session)?;
        self.check_job_slot(client)?;
        let publication_bytes = if matches!(&task, Task::Commit(_)) {
            self.check_publication_budget(session, candidate_bytes)?
        } else {
            0
        };
        let freeze_bytes = self.ready(session)?.estimated_content_bytes();
        self.check_bytes(
            freeze_bytes
                .saturating_add(publication_bytes)
                .saturating_add(candidate_bytes),
            candidate_bytes,
            freeze_bytes,
        )?;
        let frozen = Arc::new(self.ready(session)?.read_snapshot());
        let job = JobId {
            runtime: self.instance,
            serial: self.serial(),
        };
        let cancel = Arc::new(AtomicBool::new(false));
        let base = self.ready(session)?.version();
        self.jobs.insert(
            job,
            JobEntry {
                info: JobInfo {
                    id: job,
                    client,
                    session,
                    state: JobState::Queued,
                    result: None,
                },
                base,
                cancel: cancel.clone(),
                target: target.clone(),
                candidate_bytes,
                publication_bytes,
                read_source: None,
                read_bytes: 0,
                read_only: false,
                plan_result: false,
            },
        );
        if let Some(target) = target {
            self.reservations.insert(target, job);
        }
        let entry = self.entry_mut(session)?;
        entry.info.state = SessionState::Busy(job);
        entry.frozen = Some(frozen);
        let sdk = entry.sdk.take().unwrap();
        entry.events.push(
            Some(client),
            Some(job),
            job.into(),
            EventKind::StateChanged(entry.info.clone()),
        );
        self.queue.push_back(Work {
            job,
            client,
            session_id: session,
            base,
            session: Some(sdk),
            transfer,
            task,
            cancel,
        });
        self.trim_events();
        Ok(job)
    }
    /// FIFO dispatch. Executors and completion delivery are host-owned, with no
    /// dependency on windows. Rejected submissions retain ownership in the queue.
    pub fn dispatch(&mut self, executor: &mut impl Executor) {
        let mut running = self
            .jobs
            .values()
            .filter(|j| {
                matches!(j.info.state, JobState::Running | JobState::Cancelling)
                    && !self.queue.iter().any(|w| w.job == j.info.id)
            })
            .count();
        while running < self.limits.running_jobs {
            // Keep an executor slot available for interactive publication. Read
            // batches are FIFO, and cannot occupy every worker of a multi-worker host.
            let running_reads = self
                .jobs
                .values()
                .filter(|j| {
                    j.read_only
                        && matches!(j.info.state, JobState::Running | JobState::Cancelling)
                        && !self.queue.iter().any(|w| w.job == j.info.id)
                })
                .count();
            let index = self
                .queue
                .iter()
                .position(|w| !self.jobs[&w.job].read_only)
                .or_else(|| {
                    (running_reads < self.limits.running_jobs.saturating_sub(1).max(1)).then_some(0)
                });
            let Some(work) = index.and_then(|i| self.queue.remove(i)) else {
                break;
            };
            let job = work.job;
            match executor.submit(work) {
                Ok(()) => {
                    let entry = self.jobs.get_mut(&job).unwrap();
                    if entry.info.state != JobState::Cancelling {
                        entry.info.state = JobState::Running;
                    }
                    running += 1;
                }
                Err(work) => {
                    self.queue.push_front(*work);
                    break;
                }
            }
        }
    }
    pub fn job_info(&self, client: ClientId, job: JobId) -> Result<JobInfo> {
        self.client(client)?;
        let entry = self.jobs.get(&job).ok_or(RuntimeError::InvalidReference)?;
        if entry.info.client != client {
            return Err(RuntimeError::WrongOwner);
        }
        Ok(entry.info.clone())
    }
    pub fn cancel_job(&mut self, client: ClientId, job: JobId) -> Result<()> {
        self.job_info(client, job)?;
        let entry = self.jobs.get_mut(&job).unwrap();
        if !entry.info.state.is_terminal() {
            entry.cancel.store(true, Ordering::Release);
            entry.info.state = JobState::Cancelling;
        }
        let queued = self.jobs[&job]
            .read_only
            .then(|| self.queue.iter().position(|w| w.job == job))
            .flatten();
        if let Some(index) = queued {
            let work = self.queue.remove(index).unwrap();
            self.complete(Completion {
                job: work.job,
                client: work.client,
                session_id: work.session_id,
                base: work.base,
                session: None,
                transfer: work.transfer,
                result: Err(RuntimeError::Cancelled),
            });
        }
        Ok(())
    }
    pub(crate) fn cancel_host_jobs(&mut self) {
        for entry in self.jobs.values_mut() {
            if !entry.info.state.is_terminal() {
                entry.cancel.store(true, Ordering::Release);
                entry.info.state = JobState::Cancelling;
            }
        }
    }
    pub fn release_job(&mut self, client: ClientId, job: JobId) -> Result<()> {
        self.client(client)?;
        if !self.jobs.contains_key(&job) {
            return Ok(());
        }
        if !self.job_info(client, job)?.state.is_terminal() {
            return Err(RuntimeError::Busy);
        }
        self.jobs.remove(&job);
        Ok(())
    }
    /// Consume once. Invalid, duplicate or late completions cannot restore a
    /// closed/faulted session. Returns false without publishing anything.
    pub fn complete(&mut self, completion: Completion) -> bool {
        let Completion {
            job,
            client,
            session_id: session,
            base,
            session: returned,
            transfer: _transfer,
            mut result,
        } = completion;
        let Some(record) = self.jobs.get(&job) else {
            return false;
        };
        if record.read_only {
            return self.complete_read(job, client, session, base, result);
        }
        let Some(entry) = self.sessions.get(&session) else {
            return false;
        };
        if job.runtime != self.instance
            || record.info.client != client
            || record.info.session != session
            || record.base != base
            || record.info.state.is_terminal()
            || entry.info.state != SessionState::Busy(job)
            || entry.info.version != base
        {
            return false;
        }
        let target = record.target.clone();
        let provisional = entry.provisional;
        if let Some(sdk) = returned {
            // Opening has no write side effects. Refuse an oversized loaded
            // document before making it a registered, usable formal session.
            if provisional {
                let bytes = sdk
                    .estimated_content_bytes()
                    .saturating_add(sdk.history_state().estimated_bytes);
                if let Err(error) = self.check_bytes(bytes, 0, 0) {
                    result = Err(error);
                }
            }
            let entry = self.sessions.get_mut(&session).unwrap();
            entry.sdk = Some(sdk);
            entry.info.state = SessionState::Ready;
            entry.frozen = None;
            // Drain and register receipts before Ready is externally observable.
            self.refresh(session, Some(client), Some(job), job.into())
                .expect("active session");
            if result.is_ok() {
                if matches!(&result,Ok(JobValue::Project{report,..}) if report.before!=report.after)
                    && target.is_none()
                {
                    if let Some(old) = self.sessions.get_mut(&session).unwrap().info.path.take() {
                        self.paths.remove(&old);
                    }
                }
                if let Some(path) = target.as_ref().filter(|_|!matches!(&result,Ok(JobValue::Project{report,..}) if report.before==report.after)) {
                    let old = self.sessions[&session].info.path.clone();
                    if let Some(old) = old {
                        self.paths.remove(&old);
                    }
                    self.paths.insert(path.clone(), session);
                    self.sessions.get_mut(&session).unwrap().info.path = Some(path.clone());
                }
                if let Ok(JobValue::Saved(receipt)) = &result {
                    self.sessions.get_mut(&session).unwrap().events.push(
                        Some(client),
                        Some(job),
                        job.into(),
                        EventKind::Saved(receipt.clone()),
                    );
                }
            }
        } else {
            let entry = self.sessions.get_mut(&session).unwrap();
            entry.info.state = SessionState::Faulted(job);
            entry.info.fault = result.as_ref().err().cloned();
            // frozen remains available and explicitly marked Faulted.
        }
        if let Some(path) = target {
            if self.reservations.get(&path) == Some(&job) {
                self.reservations.remove(&path);
            }
        }
        let record = self.jobs.get_mut(&job).unwrap();
        record.candidate_bytes = 0;
        record.publication_bytes = 0;
        record.info.state = match &result {
            Ok(_) => JobState::Succeeded,
            Err(RuntimeError::Cancelled) => JobState::Cancelled,
            Err(_) => JobState::Failed,
        };
        record.read_bytes = result
            .as_ref()
            .map_or_else(|e| e.retained_bytes(), JobValue::retained_bytes);
        record.info.result = Some(result.clone());
        if provisional && result.is_err() {
            self.sessions.remove(&session);
        } else {
            let entry = self.sessions.get_mut(&session).unwrap();
            entry.events.push(
                Some(client),
                Some(job),
                job.into(),
                EventKind::StateChanged(entry.info.clone()),
            );
            if provisional {
                entry.provisional = false;
                self.registry.push(
                    Some(client),
                    Some(job),
                    job.into(),
                    EventKind::Created(entry.info.clone()),
                );
            }
        }
        if !self.clients.contains(&client) {
            self.jobs.remove(&job);
        }
        self.trim_events();
        self.refresh_memory_budget();
        true
    }
    fn trim_events(&mut self) {
        if self.accounted_bytes() <= self.limits.accounted_bytes {
            return;
        }
        self.registry.clear();
        for entry in self.sessions.values_mut() {
            entry.events.clear();
        }
    }
    fn candidate_bytes(&self) -> usize {
        self.edits
            .values()
            .map(|e| e.bytes)
            .chain(self.jobs.values().map(|j| j.candidate_bytes))
            .fold(0, usize::saturating_add)
    }
    fn frozen_bytes(&self) -> usize {
        self.sessions
            .values()
            .filter_map(|e| e.frozen.as_ref())
            .map(|s| s.document().estimated_content_bytes())
            .chain(
                self.jobs
                    .values()
                    .filter_map(|j| j.read_source.as_ref())
                    .map(|s| s.snapshot.document().estimated_content_bytes()),
            )
            .chain(
                self.previews
                    .values()
                    .map(|p| p.source.snapshot.document().estimated_content_bytes()),
            )
            .fold(0, usize::saturating_add)
    }
    fn history_bytes(&self) -> usize {
        self.sessions
            .values()
            .map(|e| e.history_bytes)
            .chain(self.jobs.values().map(|j| j.publication_bytes))
            .fold(0, usize::saturating_add)
    }
    pub fn accounted_bytes(&self) -> usize {
        self.non_resource_bytes()
            .saturating_add(self.image_budget.usage().total_bytes())
    }
    fn non_resource_bytes(&self) -> usize {
        self.sessions
            .values()
            .map(|e| {
                e.content_bytes
                    .saturating_add(e.history_bytes)
                    .saturating_add(e.events.bytes())
            })
            .fold(
                self.registry
                    .bytes()
                    .saturating_add(self.external_bytes)
                    .saturating_add(self.clients.len().saturating_mul(512))
                    .saturating_add(self.plans.values().map(|p| p.bytes).sum::<usize>())
                    .saturating_add(self.prepared_images.len().saturating_mul(1024))
                    .saturating_add(self.editor_contexts.len().saturating_mul(128 << 10))
                    .saturating_add(self.reference_bytes())
                    .saturating_add(self.preview_bytes())
                    .saturating_add(self.candidate_bytes())
                    .saturating_add(self.frozen_bytes())
                    .saturating_add(
                        self.jobs
                            .values()
                            .map(|j| j.read_bytes.saturating_add(512))
                            .sum::<usize>(),
                    )
                    .saturating_add(
                        self.jobs
                            .values()
                            .map(|j| j.publication_bytes)
                            .fold(0, usize::saturating_add),
                    ),
                usize::saturating_add,
            )
    }
    fn check_bytes(&self, extra: usize, candidates: usize, frozen: usize) -> Result<()> {
        if self.candidate_bytes().saturating_add(candidates) > self.limits.candidate_bytes {
            return Err(RuntimeError::Budget("candidate bytes"));
        }
        if self.frozen_bytes().saturating_add(frozen) > self.limits.frozen_bytes {
            return Err(RuntimeError::Budget("frozen bytes"));
        }
        self.image_budget
            .admit_host_budget(
                self.non_resource_bytes().saturating_add(extra),
                self.limits.accounted_bytes,
            )
            .map_err(|_| RuntimeError::Budget("accounted CPU bytes"))?;
        Ok(())
    }
    fn check_publication_budget(&self, session: SessionId, candidate: usize) -> Result<usize> {
        let entry = self.entry(session)?;
        // Conservative reservation for the new checkpoint/change report; SDK
        // still applies its own exact per-session history checks on publication.
        let growth = entry
            .content_bytes
            .saturating_add(candidate)
            .saturating_mul(2);
        if self.history_bytes().saturating_add(growth) > self.limits.history_bytes {
            return Err(RuntimeError::Budget("history bytes"));
        }
        self.check_bytes(growth, 0, 0)?;
        Ok(growth)
    }
}
fn identity(path: &Path) -> Result<PathBuf> {
    zipzi_project::project_identity(path).map_err(|error| RuntimeError::Path(error.message))
}

pub(crate) fn serialized_bytes(value: &impl serde::Serialize) -> Result<usize> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self.0.saturating_add(bytes.len());
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    crate::metrics::admission_encode();
    let mut counter = Counter(0);
    serde_json::to_writer(&mut counter, value).map_err(|_| RuntimeError::InvalidReference)?;
    Ok(counter.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn duplicate_wrong_session_and_late_results_cannot_revive_faulted_or_closed_session() {
        let mut runtime = Runtime::default();
        let client = runtime.register_client().unwrap();
        let s = runtime
            .create_session(
                client,
                &uuid::Uuid::new_v4().to_string(),
                Canvas::new(100., 100., Vec2::default(), 1.),
            )
            .unwrap();
        let base = runtime.session(s).unwrap().version();
        let dir = tempfile::tempdir().unwrap();
        let job = runtime.request_save(client, s, dir.path()).unwrap();
        let completion = |session_id, returned| Completion {
            job,
            client,
            session_id,
            base,
            session: returned,
            transfer: None,
            result: Err(RuntimeError::WorkerPanic("injected".into())),
        };
        let wrong = SessionId {
            runtime: runtime.instance_id(),
            serial: u64::MAX,
        };
        assert!(!runtime.complete(completion(wrong, None)));
        assert_eq!(
            runtime.session_info(s).unwrap().state,
            SessionState::Busy(job)
        );
        assert!(runtime.complete(completion(s, None)));
        assert_eq!(
            runtime.session_info(s).unwrap().state,
            SessionState::Faulted(job)
        );
        assert!(!runtime.complete(completion(s, None)));
        assert_eq!(
            runtime.session_info(s).unwrap().state,
            SessionState::Faulted(job)
        );
        let replacement = AuthoringSession::new(
            &uuid::Uuid::new_v4().to_string(),
            Canvas::new(100., 100., Vec2::default(), 1.),
        )
        .unwrap();
        assert!(!runtime.complete(completion(s, Some(replacement))));
        runtime.close_session(client, s, true).unwrap();
        assert!(!runtime.complete(completion(s, None)));
        assert!(runtime.list_sessions().is_empty());
    }
}
