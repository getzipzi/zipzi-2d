//! Application-owned Runtime and lightweight view adapters. Locks cover short
//! typed calls only; workers carry Work and never retain a Runtime lock.
use std::{
    ops::Deref,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};
use zipzi_core::Status;
use zipzi_core::{PreviewValues, preview::PreviewState, state::DocumentState as Model};
use zipzi_runtime::{ClientId, Runtime, SessionKey};
use zipzi_sdk::Publication;
use zipzi_sdk::{AuthoringSession, Commands, SaveReceipt, Version};
use zipzi_sdk::{DraftSource, QuerySource, SourcedSnapshot};

#[derive(Clone)]
pub struct RuntimeHost {
    pub(crate) runtime: Arc<RwLock<Runtime>>,
    pristine_views: Arc<std::sync::Mutex<std::collections::HashMap<SessionKey, Version>>>,
    pub(crate) client: ClientId,
    interactions: Arc<std::sync::Mutex<std::collections::HashSet<SessionKey>>>,
    notifiers: Arc<std::sync::Mutex<Vec<async_channel::Sender<()>>>>,
    prepared_views: Arc<
        std::sync::Mutex<
            std::collections::HashMap<
                SessionKey,
                Result<crate::document::PreparedDocument, String>,
            >,
        >,
    >,
}
impl Default for RuntimeHost {
    fn default() -> Self {
        Self::new()
    }
}
pub(crate) struct HostedInteraction {
    active: Arc<std::sync::Mutex<std::collections::HashSet<SessionKey>>>,
    session: SessionKey,
}
impl Drop for HostedInteraction {
    fn drop(&mut self) {
        self.active.lock().unwrap().remove(&self.session);
    }
}
impl RuntimeHost {
    /// UI gesture coordination is local to the Editor; other Runtime clients
    /// remain free to publish and make this gesture's captured source stale.
    pub(crate) fn interaction(
        &self,
        session: SessionKey,
    ) -> zipzi_runtime::Result<HostedInteraction> {
        self.read().session_read(self.client, session)?;
        if !self.interactions.lock().unwrap().insert(session) {
            return Err(Status::error(
                "EDITOR_BUSY",
                "Finish the active Editor gesture",
            ));
        }
        Ok(HostedInteraction {
            active: self.interactions.clone(),
            session,
        })
    }
    pub(crate) fn dispatch_job(
        &self,
        job: zipzi_runtime::JobId,
    ) -> zipzi_runtime::Result<zipzi_runtime::Work> {
        self.write().take_work(job)
    }
    pub fn new() -> Self {
        Self::from_runtime(Runtime::default())
    }
    pub fn from_runtime(mut runtime: Runtime) -> Self {
        let client = runtime.register_client().expect("initial Editor client");
        Self {
            runtime: Arc::new(RwLock::new(runtime)),
            pristine_views: Default::default(),
            client,
            interactions: Default::default(),
            notifiers: Default::default(),
            prepared_views: Default::default(),
        }
    }
    /// Desktop resources use the platform cache, with one shared Runtime disk budget.
    pub fn configure_project_cache(&self, root: std::path::PathBuf) -> anyhow::Result<()> {
        let io = zipzi_project::ProjectIo::new(root, self.read().resources().clone());
        self.write().configure_project_io(io);
        Ok(())
    }
    pub fn read(&self) -> RwLockReadGuard<'_, Runtime> {
        self.runtime.read().unwrap_or_else(|p| p.into_inner())
    }
    pub fn write(&self) -> RuntimeWrite<'_> {
        // Background preparation can finish while the UI captures a read.
        // Serialize these legitimate owner calls instead of treating overlap
        // between threads as a programming error.
        let guard = self.runtime.write().unwrap_or_else(|p| p.into_inner());
        let stamp = guard.host_state_stamp();
        RuntimeWrite {
            stamp,
            guard: Some(guard),
            notifiers: &self.notifiers,
        }
    }
    pub fn subscribe_changes(&self) -> async_channel::Receiver<()> {
        let (tx, rx) = async_channel::bounded(1);
        let mut notifiers = self.notifiers.lock().unwrap();
        notifiers.retain(|sender| !sender.is_closed());
        notifiers.push(tx);
        rx
    }
    pub(crate) fn prepare_view(
        &self,
        session: SessionKey,
        prepared: anyhow::Result<crate::document::PreparedDocument>,
    ) {
        self.prepared_views
            .lock()
            .unwrap()
            .insert(session, prepared.map_err(|error| error.to_string()));
    }
    pub(crate) fn take_prepared_view(
        &self,
        session: SessionKey,
    ) -> anyhow::Result<Option<crate::document::PreparedDocument>> {
        self.prepared_views
            .lock()
            .unwrap()
            .remove(&session)
            .transpose()
            .map_err(anyhow::Error::msg)
    }
    pub(crate) fn mark_pristine_view(&self, session: SessionKey, version: Version) {
        let live: std::collections::HashSet<_> = self
            .read()
            .session_keys(self.client)
            .expect("Editor client")
            .into_iter()
            .collect();
        let mut pristine = self.pristine_views.lock().unwrap();
        pristine.retain(|id, _| live.contains(id));
        pristine.insert(session, version);
    }
    pub(crate) fn pristine_view(&self, session: SessionKey) -> Option<Version> {
        self.pristine_views.lock().unwrap().get(&session).copied()
    }
    pub(crate) fn forget_pristine_view(&self, session: SessionKey) {
        self.pristine_views.lock().unwrap().remove(&session);
    }
    pub fn client_id(&self) -> ClientId {
        self.client
    }
    pub fn same_host(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.runtime, &other.runtime)
    }
}

pub struct RuntimeWrite<'a> {
    stamp: zipzi_runtime::HostStateStamp,
    guard: Option<RwLockWriteGuard<'a, Runtime>>,
    notifiers: &'a std::sync::Mutex<Vec<async_channel::Sender<()>>>,
}
impl Deref for RuntimeWrite<'_> {
    type Target = Runtime;
    fn deref(&self) -> &Runtime {
        self.guard.as_ref().unwrap()
    }
}
impl std::ops::DerefMut for RuntimeWrite<'_> {
    fn deref_mut(&mut self) -> &mut Runtime {
        self.guard.as_mut().unwrap()
    }
}
impl Drop for RuntimeWrite<'_> {
    fn drop(&mut self) {
        let changed = self.guard.as_ref().unwrap().host_state_stamp() != self.stamp;
        drop(self.guard.take());
        if !changed {
            return;
        }
        self.notifiers
            .lock()
            .unwrap()
            .retain(|tx| !matches!(tx.try_send(()), Err(async_channel::TrySendError::Closed(_))));
    }
}

pub(crate) struct HostedSession {
    pub(crate) host: RuntimeHost,
    pub(crate) id: SessionKey,
    client: ClientId,
    path: Option<std::path::PathBuf>,
    cursor: Version,
    last_local: Option<Version>,
    saved: Option<zipzi_sdk::ContentId>,
    needs_resync: bool,
    management_changed: bool,
    preview: PreviewState,
}
pub(crate) struct SessionRead<'a> {
    guard: RwLockReadGuard<'a, Runtime>,
    client: ClientId,
    id: SessionKey,
}
impl Deref for SessionRead<'_> {
    type Target = AuthoringSession;
    fn deref(&self) -> &Self::Target {
        self.guard
            .session_read(self.client, self.id)
            .expect("resident hosted document")
    }
}
/// A view owns a cheap immutable snapshot, never a Runtime read lock. It can
/// outlive a publication or Session close without freezing unrelated UI work.
pub(crate) struct ModelRead(pub(crate) zipzi_sdk::Snapshot);
impl Deref for ModelRead {
    type Target = Model;
    fn deref(&self) -> &Model {
        self.0.document()
    }
}

impl HostedSession {
    pub(crate) fn capture_authoring(&self) -> HostedSource {
        HostedSource {
            host: self.host.clone(),
            client: self.client,
            captured: self.sourced_snapshot(),
        }
    }
    pub(crate) fn from_session(session: AuthoringSession) -> anyhow::Result<Self> {
        let host = RuntimeHost::new();
        let id = host.write().adopt_session(host.client, session)?;
        Self::attach(host, id)
    }
    pub(crate) fn attach(host: RuntimeHost, id: SessionKey) -> anyhow::Result<Self> {
        host.write().attach(host.client, id)?;
        let (cursor, path, saved) = {
            let runtime = host.read();
            let sdk = runtime.session_read(host.client, id)?;
            (
                sdk.version(),
                sdk.project().map(|p| p.path().to_owned()),
                sdk.saved_marker().map(|m| m.content),
            )
        };
        Ok(Self {
            client: host.client,
            host,
            id,
            path,
            cursor,
            last_local: None,
            saved,
            needs_resync: false,
            management_changed: false,
            preview: Default::default(),
        })
    }
    pub(crate) fn rehost(&mut self, target: RuntimeHost) -> anyhow::Result<()> {
        if self.host.same_host(&target) {
            return Ok(());
        }
        let id = self.host.write().transfer_session_to(
            self.client,
            self.id,
            &mut target.write(),
            target.client,
        )?;
        if let Some(version) = self.host.pristine_view(self.id) {
            self.host.forget_pristine_view(self.id);
            target.mark_pristine_view(id, version);
        }
        self.id = id;
        self.client = target.client;
        self.host = target;
        self.refresh_metadata()?;
        let version = self.read().version();
        self.cursor = version;
        Ok(())
    }
    pub(crate) fn read(&self) -> SessionRead<'_> {
        SessionRead {
            guard: self.host.read(),
            client: self.client,
            id: self.id,
        }
    }
    pub(crate) fn try_read(&self) -> Result<SessionRead<'_>, Status> {
        let read = self.read();
        read.guard.session_read(self.client, self.id)?;
        Ok(read)
    }
    pub(crate) fn document(&self) -> ModelRead {
        ModelRead(self.sourced_snapshot().snapshot)
    }
    pub(crate) fn observed_version(&self) -> Version {
        self.cursor
    }
    pub(crate) fn sourced_snapshot(&self) -> SourcedSnapshot {
        self.host
            .read()
            .read_snapshot(self.client, self.id)
            .expect("resident hosted source")
    }
    pub(crate) fn refresh_metadata(&mut self) -> anyhow::Result<()> {
        let (path, saved) = {
            let sdk = self.try_read()?;
            (
                sdk.project().map(|p| p.path().to_owned()),
                sdk.saved_marker().map(|m| m.content),
            )
        };
        self.management_changed |= self.path != path || self.saved != saved;
        self.path = path;
        self.saved = saved;
        Ok(())
    }
    pub(crate) fn project_path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }
    pub(crate) fn receipts(&mut self) -> zipzi_runtime::Result<Vec<Publication>> {
        self.poll_receipts(false)
    }
    pub(crate) fn take_management_changed(&mut self) -> bool {
        std::mem::take(&mut self.management_changed)
    }
    fn poll_receipts(
        &mut self,
        acknowledge_local: bool,
    ) -> zipzi_runtime::Result<Vec<Publication>> {
        if !acknowledge_local && std::mem::take(&mut self.needs_resync) {
            return Err(Status::error(
                "EVENT_GAP",
                "Resynchronize Editor projections",
            ));
        }
        let runtime = self.host.read();
        let events = runtime.publications_since(self.client, self.cursor);
        self.cursor = runtime.session_read(self.client, self.id)?.version();
        let events = events?;
        if acknowledge_local
            && events
                .iter()
                .any(|receipt| Some(receipt.version) != self.last_local)
        {
            self.needs_resync = true;
        }
        Ok(events)
    }
    pub(crate) fn acknowledge_receipts(&mut self) -> Vec<Publication> {
        match self.poll_receipts(true) {
            Ok(receipts) => receipts,
            Err(_) => {
                self.needs_resync = true;
                vec![]
            }
        }
    }
    pub(crate) fn begin_draft(
        &self,
        label: impl Into<String>,
        expected: Option<Version>,
    ) -> zipzi_runtime::Result<HostedDraft> {
        let expected = expected.unwrap_or_else(|| self.read().version());
        let source = self.host.write().begin_draft(self.client, expected)?;
        Ok(HostedDraft {
            host: self.host.clone(),
            client: self.client,
            source,
            description: label.into(),
            ended: false,
            interaction: None,
        })
    }
    pub(crate) fn edit<T>(
        &mut self,
        label: impl Into<String>,
        expected: Option<Version>,
        apply: impl FnOnce(&mut Commands<'_>) -> Result<T, Status>,
    ) -> zipzi_runtime::Result<(T, Publication)> {
        let mut edit = self.begin_draft(label, expected)?;
        let value = edit.batch(apply)?;
        let receipt = edit.commit_to(self)?;
        Ok((value, receipt))
    }
    pub(crate) fn undo(&mut self) -> zipzi_runtime::Result<Publication> {
        let version = self.read().version();
        let publication = self.host.write().undo(self.client, version)?;
        self.last_local = Some(publication.version);
        Ok(publication)
    }
    pub(crate) fn redo(&mut self) -> zipzi_runtime::Result<Publication> {
        let version = self.read().version();
        let publication = self.host.write().redo(self.client, version)?;
        self.last_local = Some(publication.version);
        Ok(publication)
    }
    pub(crate) fn save_project(
        &mut self,
        path: &std::path::Path,
        expected: Option<Version>,
    ) -> anyhow::Result<SaveReceipt> {
        let version = expected.unwrap_or_else(|| self.read().version());
        let io = self.host.read().project_io().clone();
        let (job, work) = self.host.write().start_save_as(
            self.client,
            version,
            &io,
            path,
            None,
            Default::default(),
        )?;
        let completion = work.run();
        self.host
            .write()
            .complete(completion)
            .expect("Editor save completion belongs to its host");
        let result = self
            .host
            .read()
            .job(self.client, job)?
            .result
            .clone()
            .expect("completed save");
        self.host.write().release_job(self.client, job)?;
        self.refresh_metadata()?;
        match result? {
            zipzi_runtime::JobValue::Saved(receipt) => Ok(receipt),
            _ => unreachable!("save returned a different job value"),
        }
    }
    pub(crate) fn preview_values(&self) -> &PreviewValues {
        self.preview.values()
    }
    pub(crate) fn set_preview_values(&mut self, values: PreviewValues) -> anyhow::Result<bool> {
        self.set_preview_values_at(values, None)
    }

    pub(crate) fn set_preview_values_at(
        &mut self,
        values: PreviewValues,
        expected_revision: Option<u64>,
    ) -> anyhow::Result<bool> {
        let runtime = self.host.read();
        let sdk = runtime.session_read(self.client, self.id)?;
        anyhow::ensure!(
            expected_revision.is_none_or(|revision| revision == sdk.version().revision),
            "Parameter preview source changed"
        );
        self.preview
            .replace(sdk.snapshot().document(), sdk.version().generation, values)
            .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))
    }
    pub(crate) fn preview_frame(&mut self) -> anyhow::Result<Arc<zipzi_core::DrawableFrame>> {
        let runtime = self.host.read();
        let sdk = runtime.session_read(self.client, self.id)?;
        self.preview
            .frame(sdk.snapshot().document(), sdk.version().generation)
            .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))
    }
}

#[derive(Clone)]
pub(crate) struct HostedSource {
    host: RuntimeHost,
    client: ClientId,
    pub captured: SourcedSnapshot,
}
impl std::fmt::Debug for HostedSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostedSource")
            .field("source", &self.captured.source)
            .finish()
    }
}
impl HostedSource {
    pub fn resource_store(&self) -> zipzi_project::ResourceStore {
        self.host.read().resources().clone()
    }
    pub fn version(&self) -> Version {
        match self.captured.source {
            QuerySource::Published(version) => version,
            _ => unreachable!("captured published source"),
        }
    }
    pub fn begin_draft(&self, description: &str) -> Result<HostedDraft, Status> {
        let source = self.host.write().begin_draft(self.client, self.version())?;
        Ok(HostedDraft {
            host: self.host.clone(),
            client: self.client,
            source,
            description: description.into(),
            ended: false,
            interaction: None,
        })
    }
}

pub(crate) struct HostedDraft {
    host: RuntimeHost,
    client: ClientId,
    source: DraftSource,
    description: String,
    ended: bool,
    interaction: Option<HostedInteraction>,
}
impl HostedDraft {
    pub(crate) fn request_batch(
        &mut self,
        budget: zipzi_runtime::TaskBudget,
        run: impl FnOnce(&mut Commands<'_>, &zipzi_runtime::Cancellation) -> Result<(), Status>
        + Send
        + 'static,
    ) -> Result<zipzi_runtime::JobId, Status> {
        self.host
            .write()
            .request_batch(self.client, self.source, budget, run)
    }
    pub(crate) fn finish_batch(&mut self, job: zipzi_runtime::JobId) -> Result<(), Status> {
        let mut runtime = self.host.write();
        let result = runtime
            .job(self.client, job)?
            .result
            .clone()
            .ok_or_else(|| Status::error("JOB_PENDING", "Batch has not completed"))?;
        runtime.release_job(self.client, job)?;
        match result {
            Ok(zipzi_runtime::JobValue::DraftUpdated(source)) => {
                self.source = source;
                Ok(())
            }
            Ok(_) => Err(Status::error("JOB_RESULT", "Expected Draft update")),
            Err(error) => {
                self.ended = true;
                self.interaction.take();
                Err(error)
            }
        }
    }
    /// Run conversion work without holding the resident Runtime lock. The worker
    /// receives only its owned Draft; completion is installed under the host lock.
    pub(crate) fn worker_batch<T: Send + 'static>(
        &mut self,
        budget: zipzi_runtime::TaskBudget,
        run: impl FnOnce(&mut Commands<'_>, &zipzi_runtime::Cancellation) -> Result<T, Status>
        + Send
        + 'static,
    ) -> Result<T, Status> {
        let output = Arc::new(std::sync::Mutex::new(None));
        let worker_output = output.clone();
        let job = self.host.write().request_batch(
            self.client,
            self.source,
            budget,
            move |commands, cancel| {
                let value = run(commands, cancel)?;
                *worker_output.lock().unwrap_or_else(|p| p.into_inner()) = Some(value);
                Ok(())
            },
        )?;
        let work = match self.host.dispatch_job(job) {
            Ok(work) => work,
            Err(error) => {
                let mut runtime = self.host.write();
                let _ = runtime.cancel_job(self.client, job);
                let _ = runtime.release_job(self.client, job);
                self.ended = true;
                return Err(error);
            }
        };
        let completion = work.run_with_stack("editor-conversion", 8 * 1024 * 1024);
        let result = {
            let mut runtime = self.host.write();
            runtime.complete(completion).map_err(|_| {
                Status::error(
                    "WRONG_RUNTIME",
                    "Conversion completion belongs to another host",
                )
            })?;
            let result = runtime
                .job(self.client, job)?
                .result
                .clone()
                .ok_or_else(|| Status::error("JOB_PENDING", "Conversion did not finish"))?;
            runtime.release_job(self.client, job)?;
            result
        };
        match result {
            Ok(zipzi_runtime::JobValue::DraftUpdated(source)) => self.source = source,
            Ok(_) => return Err(Status::error("JOB_RESULT", "Expected a Draft update")),
            Err(error) => {
                self.ended = true;
                self.interaction.take();
                return Err(error);
            }
        }
        output
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
            .ok_or_else(|| Status::error("JOB_RESULT", "Conversion returned no outcome"))
    }
    pub(crate) fn read_snapshot(&self) -> zipzi_runtime::Result<SourcedSnapshot> {
        self.host.read().read_draft(self.client, self.source)
    }
    pub(crate) fn candidate_document(&self) -> ModelRead {
        ModelRead(self.read_snapshot().expect("active Editor Draft").snapshot)
    }
    pub(crate) fn base_version(&self) -> Version {
        self.source.base
    }
    pub(crate) fn claim_interaction(&mut self) -> zipzi_runtime::Result<()> {
        if self.interaction.is_none() {
            self.interaction = Some(self.host.interaction(self.source.base.session)?);
        }
        Ok(())
    }
    pub(crate) fn interactive(mut self) -> zipzi_runtime::Result<Self> {
        self.claim_interaction()?;
        Ok(self)
    }
    pub(crate) fn batch<T>(
        &mut self,
        run: impl FnOnce(&mut Commands<'_>) -> Result<T, Status>,
    ) -> zipzi_runtime::Result<T> {
        let result = self.host.write().batch(self.client, self.source, run);
        match result {
            Ok((source, value)) => {
                self.source = source;
                Ok(value)
            }
            Err(error) => {
                if !matches!(
                    error.code.as_str(),
                    "DRAFT_BUSY" | "PROJECT_BUSY" | "NOT_OWNER" | "CLIENT_NOT_FOUND"
                ) {
                    self.ended = true;
                    self.interaction.take();
                }
                Err(error)
            }
        }
    }
    pub(crate) fn preview<T>(
        &mut self,
        run: impl FnOnce(&mut Commands<'_>) -> Result<T, Status>,
    ) -> zipzi_runtime::Result<T> {
        let (source, value) = self.host.write().preview(self.client, self.source, run)?;
        self.source = source;
        Ok(value)
    }
    pub(crate) fn apply<T>(
        &mut self,
        run: impl FnOnce(&mut Commands<'_>) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        Ok(self.batch(|commands| {
            run(commands).map_err(|error| {
                error
                    .downcast_ref::<Status>()
                    .cloned()
                    .unwrap_or_else(|| Status::error("EDITOR_COMMAND", error.to_string()))
            })
        })?)
    }
    fn preview_manual<T>(
        &mut self,
        use_baseline: bool,
        run: impl FnOnce(&mut Commands<'_>) -> Result<T, Status>,
    ) -> zipzi_runtime::Result<T> {
        if !use_baseline {
            return self.preview(run);
        }
        let (source, value) =
            self.host
                .write()
                .preview_from_baseline(self.client, self.source, run)?;
        self.source = source;
        Ok(value)
    }
    pub(crate) fn apply_manual_mesh_batch(
        &mut self,
        operations: Vec<(String, zipzi_contract::authoring::ManualMeshOperation)>,
    ) -> zipzi_runtime::Result<()> {
        let gesture = operations.iter().all(|(_, operation)| {
            matches!(
                operation,
                zipzi_contract::authoring::ManualMeshOperation::MoveVertices {
                    motion_from: Some(_),
                    ..
                }
            )
        });
        self.preview_manual(gesture, |commands| {
            for (mesh, operation) in operations {
                commands.edit_manual_mesh(
                    &mesh,
                    operation,
                    Default::default(),
                    &Default::default(),
                )?;
            }
            Ok(())
        })
    }
    pub(crate) fn apply_manual_mesh(
        &mut self,
        mesh: &str,
        operation: zipzi_contract::authoring::ManualMeshOperation,
    ) -> zipzi_runtime::Result<zipzi_sdk::ManualMeshTransferReport> {
        let gesture = matches!(
            &operation,
            zipzi_contract::authoring::ManualMeshOperation::MoveVertices {
                motion_from: Some(_),
                ..
            }
        );
        self.preview_manual(gesture, |commands| {
            commands
                .edit_manual_mesh(mesh, operation, Default::default(), &Default::default())
                .map(|outcome| outcome.report)
        })
    }
    pub(crate) fn is_active(&self) -> bool {
        !self.ended
    }
    pub(crate) fn commit_to(
        &mut self,
        session: &mut HostedSession,
    ) -> zipzi_runtime::Result<Publication> {
        #[cfg(feature = "bench-instrumentation")]
        let _span = zipzi_core::benchmark::span("editor.commit_to");
        if session.id != self.source.base.session || !session.host.same_host(&self.host) {
            return Err(Status::error(
                "NOT_OWNER",
                "Draft belongs to another hosted document",
            ));
        }
        let result = self
            .host
            .write()
            .publish(self.client, self.source, self.description.clone());
        if result.as_ref().err().is_none_or(|error| {
            !matches!(
                error.code.as_str(),
                "DRAFT_BUSY" | "PROJECT_BUSY" | "NOT_OWNER" | "CLIENT_NOT_FOUND"
            )
        }) {
            self.ended = true;
            self.interaction.take();
        }
        if let Ok(publication) = &result {
            session.last_local = Some(publication.version);
        }
        result
    }
}
impl Drop for HostedDraft {
    fn drop(&mut self) {
        if !self.ended {
            let _ = self
                .host
                .write()
                .cancel_draft(self.client, self.source.draft);
        }
    }
}

#[path = "runtime_host_forward.rs"]
mod forwarding;

#[cfg(test)]
mod receipt_tests {
    #[test]
    fn local_ack_cannot_hide_foreign_publications_or_a_stream_gap() -> anyhow::Result<()> {
        let mut document = crate::Document::new_blank()?;
        let host = document.runtime_host();
        let session = document.runtime_session_id();
        let agent = host.write().register_client()?;
        host.write().attach(agent, session)?;
        let publish = |width: f32| -> anyhow::Result<()> {
            let version = host.read().session_read(agent, session)?.version();
            let draft = host.write().begin_draft(agent, version)?;
            let (draft, ()) = host.write().batch(agent, draft, |commands| {
                commands.create_parameter(zipzi_core::Parameter {
                    id: uuid::Uuid::new_v4().to_string(),
                    name: format!("parameter {width}"),
                    ..Default::default()
                })
            })?;
            host.write().publish(agent, draft, "create parameter")?;
            Ok(())
        };
        publish(200.)?;
        document.session.acknowledge_receipts();
        assert!(document.sync_runtime_events()?);
        assert!(!document.sync_runtime_events()?);
        let revision = document.properties_revision();
        for width in 300..900 {
            publish(width as f32)?;
        }
        document.session.acknowledge_receipts();
        assert!(document.sync_runtime_events()?);
        assert!(document.properties_revision() > revision);
        assert!(!document.sync_runtime_events()?);
        Ok(())
    }
}

#[cfg(test)]
mod notification_tests {
    use super::*;

    fn session() -> anyhow::Result<HostedSession> {
        let host = RuntimeHost::new();
        let id = host.write().create_session(
            host.client,
            &uuid::Uuid::new_v4().to_string(),
            Default::default(),
        )?;
        HostedSession::attach(host, id)
    }

    #[test]
    fn busy_publication_retains_interactive_candidate_for_retry() -> anyhow::Result<()> {
        let mut session = session()?;
        let mut draft = session.begin_draft("gesture", None)?.interactive()?;
        let id = uuid::Uuid::new_v4().to_string();
        draft.batch(|c| {
            c.create_parameter(zipzi_core::Parameter {
                id: id.clone(),
                ..Default::default()
            })
        })?;
        let captured = draft.read_snapshot()?;
        let version = session.read().version();
        let io = session.host.read().project_io().clone();
        let dir = tempfile::tempdir()?;
        let (job, work) = session.host.write().start_save_as(
            session.client,
            version,
            &io,
            &dir.path().join("view.zipzi"),
            None,
            Default::default(),
        )?;
        assert_eq!(
            draft.commit_to(&mut session).unwrap_err().code,
            "PROJECT_BUSY"
        );
        assert!(draft.is_active());
        assert_eq!(draft.read_snapshot()?.source, captured.source);
        assert!(session.host.interaction(session.id).is_err());
        session.host.write().complete(work.run()).unwrap();
        session.host.write().release_job(session.client, job)?;
        draft.commit_to(&mut session)?;
        assert!(!draft.is_active());
        assert!(session.host.interaction(session.id).is_ok());
        assert!(session.document().parameter(&id).is_some());
        assert_eq!(captured.snapshot.document().parameter(&id).unwrap().id, id);
        Ok(())
    }

    #[test]
    fn local_gesture_does_not_block_foreign_publication_or_hold_read_locks() -> anyhow::Result<()> {
        let mut session = session()?;
        let before = session.document();
        let mut gesture = session.begin_draft("gesture", None)?.interactive()?;
        let agent = session.host.write().register_client()?;
        session.host.write().attach(agent, session.id)?;
        let other = session
            .host
            .write()
            .begin_draft(agent, gesture.base_version())?;
        let id = uuid::Uuid::new_v4().to_string();
        let (other, ()) = session.host.write().batch(agent, other, |c| {
            c.create_parameter(zipzi_core::Parameter {
                id: id.clone(),
                ..Default::default()
            })
        })?;
        session.host.write().publish(agent, other, "foreign")?;
        assert!(before.parameter(&id).is_none());
        assert!(session.document().parameter(&id).is_some());
        assert_eq!(
            gesture.commit_to(&mut session).unwrap_err().code,
            "STALE_SOURCE"
        );
        assert!(!gesture.is_active());
        assert!(session.host.interaction(session.id).is_ok());
        Ok(())
    }

    #[test]
    fn candidate_table_changes_stay_quiet_but_publication_and_job_states_wake_views()
    -> anyhow::Result<()> {
        let host = RuntimeHost::new();
        let client = host.client_id();
        let session = host.write().create_session(
            client,
            "64000000-0000-4000-8000-000000000001",
            zipzi_core::ModelSetup::default(),
        )?;
        let receiver = host.subscribe_changes();
        let version = host.read().session_read(client, session)?.version();
        let draft = host.write().begin_draft(client, version)?;
        let (draft, ()) = host.write().preview(client, draft, |commands| {
            commands.create_parameter(zipzi_core::Parameter {
                id: uuid::Uuid::new_v4().to_string(),
                ..Default::default()
            })
        })?;
        assert!(
            receiver.try_recv().is_err(),
            "candidate bookkeeping woke formal projections"
        );
        let publication = host.write().publish(client, draft, "preview")?;
        receiver.try_recv()?;
        let directory = tempfile::tempdir()?;
        let io = host.read().project_io().clone();
        let job = host.write().request_save_as(
            client,
            publication.version,
            &io,
            &directory.path().join("view.zipzi"),
            None,
            Default::default(),
        )?;
        receiver.try_recv()?;
        let work = host.write().take_work(job)?;
        receiver.try_recv()?;
        host.write().complete(work.run()).unwrap();
        receiver.try_recv()?;
        host.write().release_job(client, job)?;
        receiver.try_recv()?;
        host.write().close_session(client, session, false)?;
        receiver.try_recv()?;
        Ok(())
    }
}

impl zipzi_runtime::editor_context::EditorContextProvider for crate::Editor {
    fn editor_context(
        &self,
        session: SessionKey,
    ) -> Option<zipzi_runtime::editor_context::EditorContext> {
        let document = self.document()?;
        if document.runtime_session_id() != session {
            return None;
        }
        Some(zipzi_runtime::editor_context::EditorContext {
            version: document.session.version(),
            view_id: "main".into(),
            selected_ids: document
                .context()
                .selections
                .iter()
                .map(|o| o.id.clone())
                .collect(),
            workspace: format!("{:?}", self.workspace()),
            editing_context: format!("{:?}", self.editing_context()?),
            active_tool: format!("{:?}", self.active_tool()),
            pending_interaction: document.has_pending_edit(),
        })
    }
}
