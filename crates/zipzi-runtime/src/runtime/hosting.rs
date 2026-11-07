//! Native host coordination. These operations never hand Session ownership to a Worker.
use super::*;

/// Cheap view metadata; no model payload or history entries are retained here.
#[derive(Debug, PartialEq, Eq)]
pub struct HostStateStamp {
    sessions: Vec<(
        SessionKey,
        Version,
        Option<zipzi_sdk::ContentId>,
        Option<PathBuf>,
    )>,
    jobs: Vec<(JobId, JobState)>,
}
impl Runtime {
    pub fn session_catalog(&self, client: ClientId) -> Result<Vec<SessionOverview>> {
        self.client(client)?;
        let mut entries: Vec<_> = self
            .sessions
            .iter()
            .map(|(session, entry)| SessionOverview {
                session: *session,
                version: entry.sdk.version(),
                document_id: entry.sdk.snapshot().document().id().into(),
                path: entry.sdk.project().map(|project| project.path().to_owned()),
                dirty: entry.sdk.is_dirty(),
                saving: entry.saving.is_some(),
            })
            .collect();
        entries.sort_by_key(|entry| entry.session);
        Ok(entries)
    }
    /// Freeze export content, resource bytes and source paths together. Reading
    /// never transfers Session ownership and remains available during save.
    pub fn transfer_snapshot(
        &self,
        client: ClientId,
        source: QuerySource,
    ) -> Result<zipzi_project::TransferSnapshot> {
        let captured = self.capture(client, source)?;
        let entry = self.session(client, source.session())?;
        let mut protected = entry.protected_paths.clone();
        if let Some(project) = entry.sdk.project() {
            protected.push(project.path().to_owned());
        }
        protected.sort();
        protected.dedup();
        zipzi_project::TransferSnapshot::new(
            captured.snapshot.document().clone(),
            captured.snapshot.resources().clone(),
            protected,
        )
    }

    pub fn saving_job(&self, client: ClientId, session: SessionKey) -> Result<Option<JobId>> {
        Ok(self.session(client, session)?.saving)
    }
    pub fn project_session(&self, client: ClientId, path: &Path) -> Result<Option<SessionKey>> {
        self.client(client)?;
        Ok(self
            .paths
            .get(&self.project_io.normalize_path(path)?)
            .copied())
    }
    pub fn client_jobs(&self, client: ClientId) -> Result<Vec<JobInfo>> {
        self.client(client)?;
        Ok(self
            .jobs
            .values()
            .filter(|job| job.info.client == client)
            .map(|job| job.info.clone())
            .collect())
    }
    pub fn outstanding_job_ids(&self) -> Vec<JobId> {
        self.jobs
            .values()
            .filter(|job| !job.info.state.is_terminal())
            .map(|job| job.info.id)
            .collect()
    }
    fn admit_immediate_worker(&self) -> Result<()> {
        if self
            .jobs
            .values()
            .filter(|j| j.entered && !j.info.state.is_terminal())
            .count()
            >= self.limits.running_jobs
        {
            return Err(error("JOB_BUSY", "Worker concurrency is full"));
        }
        Ok(())
    }
    pub fn start_open(
        &mut self,
        client: ClientId,
        expected: Version,
        io: &ProjectIo,
        path: &Path,
        budget: TaskBudget,
    ) -> Result<(JobId, Work)> {
        self.source(client, expected)?;
        self.admit_immediate_worker()?;
        let job = self.request_open(client, expected, io, path, budget)?;
        Ok((job, self.take_work(job).expect("newly admitted open")))
    }
    pub fn start_import(
        &mut self,
        client: ClientId,
        expected: Version,
        path: &Path,
        format: zipzi_contract::kernel_wire::ImportFormat,
        budget: TaskBudget,
    ) -> Result<(JobId, Work)> {
        self.source(client, expected)?;
        self.admit_immediate_worker()?;
        let job = self.request_import(client, expected, path, format, budget)?;
        Ok((job, self.take_work(job).expect("newly admitted import")))
    }
    pub fn host_state_stamp(&self) -> HostStateStamp {
        let mut sessions: Vec<_> = self
            .sessions
            .iter()
            .map(|(key, entry)| {
                (
                    *key,
                    entry.sdk.version(),
                    entry.sdk.saved_marker().map(|s| s.content),
                    entry.sdk.project().map(|p| p.path().to_owned()),
                )
            })
            .collect();
        sessions.sort_by_key(|entry| entry.0);
        let mut jobs: Vec<_> = self
            .jobs
            .iter()
            .map(|(key, entry)| (*key, entry.info.state))
            .collect();
        jobs.sort_by_key(|entry| entry.0);
        HostStateStamp { sessions, jobs }
    }
    pub fn session_keys(&self, client: ClientId) -> Result<Vec<SessionKey>> {
        self.client(client)?;
        let mut keys: Vec<_> = self.sessions.keys().copied().collect();
        keys.sort();
        Ok(keys)
    }
    /// Install an initially constructed SDK Session on its host owner. Preserve
    /// its saved marker and history; this is not a worker completion path.
    pub fn adopt_session(
        &mut self,
        client: ClientId,
        mut sdk: AuthoringSession,
    ) -> Result<SessionKey> {
        self.client(client)?;
        if self.sessions.len() >= self.limits.sessions {
            return Err(error("RESOURCE_LIMIT", "Session limit"));
        }
        let key = sdk.version().session;
        if self.sessions.contains_key(&key) {
            return Err(error("SESSION_EXISTS", "Session already hosted"));
        }
        if let Some(project) = sdk.project() {
            self.target(project.path(), key)?;
        }
        let event_floor = sdk.version();
        sdk.take_events();
        if let Some(project) = sdk.project() {
            self.paths.insert(project.path().to_owned(), key);
        }
        self.sessions.insert(
            key,
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
        Ok(key)
    }
    /// Rehome an idle native view before attaching it to an application host.
    /// Both hosts are borrowed exclusively; no history/snapshot clone is needed.
    pub fn transfer_session_to(
        &mut self,
        client: ClientId,
        key: SessionKey,
        target: &mut Runtime,
        target_client: ClientId,
    ) -> Result<SessionKey> {
        let entry = self.session(client, key)?;
        target.client(target_client)?;
        if entry.users.len() != 1
            || self.drafts.values().any(|d| d.source.base.session == key)
            || self
                .jobs
                .values()
                .any(|j| j.info.source.session() == key && !j.info.state.is_terminal())
        {
            return Err(error(
                "SESSION_BUSY",
                "Rehome requires an idle, exclusively attached Session",
            ));
        }
        if target.sessions.len() >= target.limits.sessions {
            return Err(error("RESOURCE_LIMIT", "Target Session limit"));
        }
        if target.sessions.contains_key(&key) {
            return Err(error("SESSION_EXISTS", "Session already hosted"));
        }
        if let Some(project) = entry.sdk.project() {
            target.target(project.path(), key)?;
        }
        let mut entry = self.sessions.remove(&key).unwrap();
        entry.users = HashSet::from([target_client]);
        self.paths.retain(|_, owner| *owner != key);
        if let Some(project) = entry.sdk.project() {
            target.paths.insert(project.path().to_owned(), key);
        }
        target.sessions.insert(key, entry);
        Ok(key)
    }
    /// Dispatch one admitted job for a native task owner, keeping queued work
    /// untouched when concurrency admission refuses entry.
    pub fn take_work(&mut self, job: JobId) -> Result<Work> {
        self.admit_immediate_worker()?;
        let index = self
            .queue
            .iter()
            .position(|work| work.lock().unwrap_or_else(|p| p.into_inner()).identity.job == job)
            .ok_or_else(|| error("JOB_NOT_QUEUED", "Job is not queued"))?;
        let work = self
            .queue
            .remove(index)
            .unwrap()
            .into_inner()
            .unwrap_or_else(|p| p.into_inner());
        let entry = self.jobs.get_mut(&job).unwrap();
        entry.entered = true;
        if !entry.cancel.is_cancelled() {
            entry.info.state = JobState::Running;
        }
        Ok(work)
    }

    /// Admit and dispatch a native save in one owner call. A full executor must
    /// not leave a save permit or queued job behind when the caller has no task
    /// handle to retry later. The returned Work still runs outside the owner.
    pub fn start_save_as(
        &mut self,
        client: ClientId,
        expected: Version,
        io: &ProjectIo,
        path: &Path,
        fingerprint: Option<&ProjectFingerprint>,
        budget: TaskBudget,
    ) -> Result<(JobId, Work)> {
        self.source(client, expected)?;
        self.admit_immediate_worker()?;
        let job = self.request_save_as(client, expected, io, path, fingerprint, budget)?;
        let work = self
            .take_work(job)
            .expect("new save is queued and concurrency was admitted");
        Ok((job, work))
    }
}

impl Runtime {
    pub fn set_editor_context(
        &mut self,
        session: SessionKey,
        context: Option<crate::editor_context::EditorContext>,
    ) -> Result<()> {
        let entry = self
            .sessions
            .get_mut(&session)
            .ok_or_else(|| error("SESSION_NOT_FOUND", "Unknown session"))?;
        if context
            .as_ref()
            .is_some_and(|context| context.version != entry.sdk.version())
        {
            return Err(error("STALE_SOURCE", "UI context source changed"));
        }
        entry.editor_context = context;
        Ok(())
    }
    pub fn editor_context(
        &self,
        client: ClientId,
        source: Version,
    ) -> Result<Option<crate::editor_context::EditorContext>> {
        let entry = self.session(client, source.session)?;
        if entry.sdk.version() != source {
            return Err(error("STALE_SOURCE", "UI context source changed"));
        }
        Ok(entry
            .editor_context
            .as_ref()
            .filter(|context| context.version == source)
            .cloned())
    }
}
