//! Short typed calls for in-process hosts. Mutable formal SDK access stays here.
use super::*;

impl Runtime {
    /// Bounded SDK preflight workspace. It is promoted only after the view has
    /// resolved its previous interaction, preserving the one writable-edit rule.
    pub fn prepare_edit(
        &mut self,
        client: ClientId,
        session: SessionId,
        label: impl Into<String>,
        expected: Option<Version>,
    ) -> Result<EditId> {
        self.client(client)?;
        if self.edits.len() >= self.limits.edits_total
            || self.edits.values().filter(|e| e.session == session).count()
                >= self.limits.edits_per_session
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
                state: EditState::Prepared,
                sdk: Some(sdk),
                bytes,
            },
        );
        Ok(id)
    }
    pub fn promote_edit(&mut self, client: ClientId, edit: EditId) -> Result<()> {
        let entry = self.edit(client, edit)?;
        if entry.state == EditState::Editing {
            return Ok(());
        }
        if entry.state != EditState::Prepared {
            return Err(RuntimeError::Aborted);
        }
        let session = entry.session;
        self.ready(session)?;
        self.check_interaction(client, session)?;
        if self
            .edits
            .values()
            .filter(|e| e.session == session && e.client == client && e.state == EditState::Editing)
            .count()
            >= self.limits.edits_per_client_session
        {
            return Err(RuntimeError::Budget("writable edits"));
        }
        self.edits.get_mut(&edit).unwrap().state = EditState::Editing;
        Ok(())
    }
    pub fn request_import(
        &mut self,
        client: ClientId,
        source: crate::ImportSource,
    ) -> Result<JobId> {
        self.client(client)?;
        self.check_job_slot(client)?;
        let sdk = AuthoringSession::new(&uuid::Uuid::new_v4().to_string(), ModelSetup::default())?;
        let session = self.insert_session(client, sdk, true)?;
        match self.queue_task(client, session, Task::Import(source), None, 0) {
            Ok(job) => Ok(job),
            Err(error) => {
                self.sessions.remove(&session);
                Err(error)
            }
        }
    }
    /// Host-wide pending work, including provisional sessions and other clients.
    pub fn pending_jobs(&self) -> Vec<JobInfo> {
        self.jobs
            .values()
            .filter(|job| !job.info.state.is_terminal())
            .map(|job| job.info.clone())
            .collect()
    }
    pub fn list_jobs(&self, client: ClientId) -> Result<Vec<JobInfo>> {
        self.client(client)?;
        let mut jobs: Vec<_> = self
            .jobs
            .values()
            .filter(|job| job.info.client == client)
            .map(|job| job.info.clone())
            .collect();
        jobs.sort_by_key(|job| job.id);
        Ok(jobs)
    }
    pub fn event_cursor(&self, session: SessionId) -> Result<EventCursor> {
        Ok(self.entry(session)?.events.cursor())
    }
    pub fn candidate(&self, client: ClientId, edit: EditId) -> Result<&EditSession<'static>> {
        self.edit(client, edit)?
            .sdk
            .as_ref()
            .ok_or(RuntimeError::Aborted)
    }
    pub fn register_owned_edit(
        &mut self,
        client: ClientId,
        session: SessionId,
        mut sdk: EditSession<'static>,
    ) -> Result<EditId> {
        self.client(client)?;
        self.ready(session)?;
        self.check_owned_edit_slot(client, session)?;
        if sdk.base_version() != self.ready(session)?.version() {
            return Err(RuntimeError::InvalidReference);
        }
        sdk.set_image_memory_budget(self.image_budget.clone())?;
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
    pub(super) fn publish_plan(
        &mut self,
        client: ClientId,
        session: SessionId,
        label: &str,
        expected: Version,
        apply: impl FnOnce(&mut EditSession<'static>) -> std::result::Result<(), zipzi_sdk::SdkError>,
    ) -> Result<EditReceipt> {
        self.check_interaction(client, session)?;
        let edit = self.begin_edit(client, session, label, Some(expected))?;
        if let Err(error) = self.apply_edit(client, edit, apply) {
            self.release_edit(client, edit)?;
            return Err(error);
        }
        let result = self.commit_edit(client, edit);
        if result.is_err() {
            self.release_edit(client, edit)?;
        }
        result
    }
    fn check_owned_edit_slot(&self, client: ClientId, session: SessionId) -> Result<()> {
        let own = self
            .edits
            .values()
            .filter(|e| e.session == session && e.client == client)
            .count();
        let count = self.edits.values().filter(|e| e.session == session).count();
        if own >= self.limits.edits_per_client_session
            || count >= self.limits.edits_per_session
            || self.edits.len() >= self.limits.edits_total
        {
            return Err(RuntimeError::Budget("edits"));
        }
        Ok(())
    }
    pub(super) fn typed_write(
        &mut self,
        client: ClientId,
        session: SessionId,
        prepare: impl FnOnce(
            &AuthoringSession,
        ) -> std::result::Result<EditSession<'static>, zipzi_sdk::SdkError>,
    ) -> Result<EditReceipt> {
        self.client(client)?;
        self.check_interaction(client, session)?;
        self.check_owned_edit_slot(client, session)?;
        let candidate = prepare(self.ready(session)?)?;
        let edit = self.register_owned_edit(client, session, candidate)?;
        let result = self.commit_edit(client, edit);
        if result.is_err() {
            self.release_edit(client, edit)?;
        }
        result
    }
    pub fn begin_interaction(&mut self, client: ClientId, session: SessionId) -> Result<()> {
        self.client(client)?;
        self.ready(session)?;
        self.check_interaction(client, session)?;
        self.entry_mut(session)?.interaction = Some(client);
        Ok(())
    }
    pub fn end_interaction(&mut self, client: ClientId, session: SessionId) -> Result<()> {
        self.client(client)?;
        let entry = self.entry_mut(session)?;
        if entry.interaction == Some(client) {
            entry.interaction = None;
        }
        Ok(())
    }
    pub(super) fn check_interaction(&self, client: ClientId, session: SessionId) -> Result<()> {
        if self
            .entry(session)?
            .interaction
            .is_some_and(|owner| owner != client)
        {
            return Err(RuntimeError::Busy);
        }
        Ok(())
    }
    pub fn take_work(&mut self, job: JobId) -> Result<Work> {
        let position = self
            .queue
            .iter()
            .position(|work| work.job == job)
            .ok_or(RuntimeError::InvalidReference)?;
        let running = self
            .jobs
            .values()
            .filter(|j| {
                matches!(j.info.state, JobState::Running | JobState::Cancelling)
                    && !self.queue.iter().any(|w| w.job == j.info.id)
            })
            .count();
        if running >= self.limits.running_jobs {
            return Err(RuntimeError::Busy);
        }
        let read = self.jobs[&job].read_only;
        if read {
            let reads = self
                .jobs
                .values()
                .filter(|j| {
                    j.read_only
                        && matches!(j.info.state, JobState::Running | JobState::Cancelling)
                        && !self.queue.iter().any(|w| w.job == j.info.id)
                })
                .count();
            if reads >= self.limits.running_jobs.saturating_sub(1).max(1)
                || self.queue.iter().any(|w| !self.jobs[&w.job].read_only)
                || self.next_read_index() != Some(position)
            {
                return Err(RuntimeError::Busy);
            }
        }
        let work = self.queue.remove(position).unwrap();
        if read {
            self.jobs.get_mut(&job).unwrap().gpu_resident = self.jobs[&job].gpu;
            self.read_turns.pop_front();
            self.read_turns.push_back(work.client);
        }
        if self.jobs[&job].info.state != JobState::Cancelling {
            self.jobs.get_mut(&job).unwrap().info.state = JobState::Running;
        }
        Ok(work)
    }
    /// Cancel an unentered host Job without borrowing an executor slot. Return
    /// ownership and release reservations before the host reports admission
    /// failure. An entered Job must use ordinary cooperative cancellation.
    pub fn cancel_queued_job(&mut self, client: ClientId, job: JobId) -> Result<()> {
        self.job_info(client, job)?;
        let position = self
            .queue
            .iter()
            .position(|work| work.job == job)
            .ok_or(RuntimeError::Busy)?;
        if matches!(
            self.queue[position].task,
            Task::Read(crate::jobs::ReadTask::Batch(_))
        ) {
            // Entered continuations require worker-side cleanup, even when queued.
            return Err(RuntimeError::Busy);
        }
        let Work {
            job,
            client,
            session_id,
            base,
            session,
            transfer,
            task,
            cancel,
        } = self.queue.remove(position).unwrap();
        cancel.store(true, Ordering::Release);
        // Drop candidate/prepared roots before completion collects resources.
        drop(task);
        self.complete(Completion {
            job,
            client,
            session_id,
            base,
            session,
            transfer,
            outcome: crate::jobs::WorkOutcome::Complete(Err(RuntimeError::Cancelled)),
        });
        Ok(())
    }
    pub fn find_project(&self, path: &Path) -> Result<Option<SessionId>> {
        Ok(self.paths.get(&identity(path)?).copied())
    }
    /// Move a bootstrap standalone host into the application registry without
    /// cloning its SDK state/history. Active edits and jobs prevent transfer.
    pub fn transfer_session_to(
        &mut self,
        client: ClientId,
        session: SessionId,
        target: &mut Runtime,
        target_client: ClientId,
    ) -> Result<SessionId> {
        self.client(client)?;
        self.exclusive(session)?;
        target.client(target_client)?;
        let entry = self.entry(session)?;
        if entry.users.iter().any(|id| *id != client) {
            return Err(RuntimeError::Busy);
        }
        if target.sessions.len() >= target.limits.sessions {
            return Err(RuntimeError::Budget("sessions"));
        }
        if entry
            .info
            .path
            .as_ref()
            .is_some_and(|p| target.paths.contains_key(p) || target.reservations.contains_key(p))
        {
            return Err(RuntimeError::Busy);
        }
        target.check_bytes(
            entry.content_bytes.saturating_add(entry.history_bytes),
            0,
            0,
        )?;
        if target.history_bytes().saturating_add(entry.history_bytes) > target.limits.history_bytes
        {
            return Err(RuntimeError::Budget("history"));
        }
        let mut entry = self.sessions.remove(&session).unwrap();
        if let Some(path) = &entry.info.path {
            self.paths.remove(path);
        }
        let operation = OperationId {
            runtime: self.instance,
            serial: self.serial(),
        };
        let mut closed = entry.info.clone();
        closed.state = SessionState::Closed;
        self.registry
            .push(Some(client), None, operation, EventKind::Closed(closed));
        // All fallible admission checks precede ownership removal. Re-key the
        // already validated entry rather than re-running filesystem admission
        // after moving its sole SDK instance out of the source host.
        let id = SessionId {
            runtime: target.instance,
            serial: target.serial(),
        };
        entry.info.id = id;
        entry.users = HashSet::from([target_client]);
        entry.events = EventLog::new(
            target.instance,
            Some(id),
            target.limits.session_events,
            target.limits.session_event_bytes,
        );
        entry.provisional = false;
        if let Some(path) = &entry.info.path {
            target.paths.insert(path.clone(), id);
        }
        target.registry.push(
            Some(target_client),
            None,
            OperationId {
                runtime: id.runtime,
                serial: id.serial,
            },
            EventKind::Created(entry.info.clone()),
        );
        target.sessions.insert(id, entry);
        self.trim_events();
        target.trim_events();
        Ok(id)
    }
}

/// A temporary candidate borrow, never a formal AuthoringSession. Hosts must
/// return it even on errors/panics, just as an executor must return Work.
pub struct CandidateWork {
    client: ClientId,
    id: EditId,
    sdk: EditSession<'static>,
}
impl CandidateWork {
    pub fn apply<T, E>(
        &mut self,
        apply: impl FnOnce(&mut EditSession<'static>) -> std::result::Result<T, E>,
    ) -> std::result::Result<T, E> {
        apply(&mut self.sdk)
    }
}
impl Runtime {
    pub fn lease_candidate(&mut self, client: ClientId, id: EditId) -> Result<CandidateWork> {
        let entry = self.edit(client, id)?;
        if !matches!(entry.state, EditState::Editing | EditState::Prepared) {
            return Err(RuntimeError::Aborted);
        }
        let sdk = self
            .edits
            .get_mut(&id)
            .unwrap()
            .sdk
            .take()
            .ok_or(RuntimeError::Busy)?;
        Ok(CandidateWork { client, id, sdk })
    }
    pub fn return_candidate(
        &mut self,
        work: CandidateWork,
        failed: bool,
        recoverable: bool,
    ) -> Result<()> {
        let CandidateWork { client, id, sdk } = work;
        let entry = self.edit(client, id)?;
        if entry.sdk.is_some() {
            return Err(RuntimeError::InvalidReference);
        }
        let old = entry.bytes;
        let bytes = sdk.estimated_bytes();
        let budget = self.check_bytes(bytes.saturating_sub(old), bytes.saturating_sub(old), 0);
        let entry = self.edits.get_mut(&id).unwrap();
        entry.sequence += 1;
        if budget.is_err() {
            entry.state = EditState::Aborted;
            entry.bytes = 0;
        } else {
            if sdk.is_aborted() || (failed && !recoverable) {
                entry.state = EditState::Aborted;
            }
            entry.sdk = Some(sdk);
            entry.bytes = bytes;
        }
        budget
    }
}

impl Runtime {
    pub fn request_project(
        &mut self,
        client: ClientId,
        session: SessionId,
        request: crate::project_io::ProjectRequest,
    ) -> Result<JobId> {
        use crate::project_io::ProjectRequest;
        self.client(client)?;
        self.exclusive(session)?;
        let target = match &request {
            ProjectRequest::Open { path } | ProjectRequest::Export { path, .. } => {
                Some(identity(path)?)
            }
            _ => None,
        };
        if let Some(path) = &target {
            if self.reservations.contains_key(path)
                || self.paths.get(path).is_some_and(|s| *s != session)
            {
                return Err(RuntimeError::Busy);
            }
        }
        // Reserve both codec workspace and all live image payload expansion
        // before giving the worker ownership of the SDK session.
        let sdk = self.ready(session)?;
        let bytes = sdk
            .estimated_content_bytes()
            .saturating_mul(16)
            .saturating_add(
                sdk.document()
                    .asset_order()
                    .iter()
                    .filter_map(|id| sdk.document().get_asset(id)?.content.as_ref())
                    .fold(0usize, |n, c| {
                        n.saturating_add(
                            (c.width as usize)
                                .saturating_mul(c.height as usize)
                                .saturating_mul(16),
                        )
                    }),
            )
            .saturating_add(8 << 20);
        let io = match &request {
            ProjectRequest::Import { .. } => 256 << 20,
            ProjectRequest::Open { path } => std::fs::metadata(path)
                .map_or(0, |m| m.len() as usize)
                .saturating_mul(128),
            _ => 0,
        };
        self.refresh_memory_budget();
        let lease = Arc::new(self.reserve_transfer(bytes.saturating_add(io))?);
        self.queue_task(
            client,
            session,
            Task::Project {
                request,
                _lease: lease,
            },
            target,
            0,
        )
    }
}

impl Runtime {
    pub fn request_project_target(
        &mut self,
        client: ClientId,
        session: Option<SessionId>,
        request: crate::project_io::ProjectRequest,
    ) -> Result<JobId> {
        if let Some(session) = session {
            return self.request_project(client, session, request);
        }
        self.client(client)?;
        self.check_job_slot(client)?;
        if matches!(&request, crate::project_io::ProjectRequest::Export { .. }) {
            return Err(RuntimeError::InvalidReference);
        }
        let sdk = AuthoringSession::new(&uuid::Uuid::new_v4().to_string(), ModelSetup::default())?;
        let session = self.insert_session(client, sdk, true)?;
        match self.request_project(client, session, request) {
            Ok(job) => Ok(job),
            Err(error) => {
                self.sessions.remove(&session);
                Err(error)
            }
        }
    }
}

impl Runtime {
    pub fn set_editor_context(
        &mut self,
        session: SessionId,
        context: Option<crate::editor_context::EditorContext>,
    ) -> Result<()> {
        let version = self.session_info(session)?.version;
        if let Some(value) = context {
            if value.version != version {
                return Err(RuntimeError::InvalidReference);
            }
            if value.selected_ids.len() > 256
                || value.view_id.len() > 256
                || value.active_tool.len() > 256
                || value.workspace.len() > 256
                || value.editing_context.len() > 256
                || value.selected_ids.iter().any(|s| s.len() > 256)
            {
                return Err(RuntimeError::Budget("Editor context"));
            }
            self.check_bytes(128 << 10, 0, 0)?;
            self.editor_contexts.insert(session, value);
        } else {
            self.editor_contexts.remove(&session);
        }
        self.refresh_memory_budget();
        Ok(())
    }
    pub fn editor_context(
        &self,
        session: SessionId,
    ) -> Result<Option<&crate::editor_context::EditorContext>> {
        let version = self.session_info(session)?.version;
        Ok(self
            .editor_contexts
            .get(&session)
            .filter(|c| c.version == version))
    }
}
