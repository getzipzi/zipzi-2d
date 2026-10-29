//! Application-owned Runtime and lightweight view adapters. Locks cover short
//! typed calls only; workers carry Work and never retain a Runtime lock.
use std::{
    ops::Deref,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};
use zipzi_core::{Document as Model, PreviewValues, preview::PreviewState};
use zipzi_runtime::{ClientId, EditId, EventCursor, EventKind, Runtime, SessionId};
use zipzi_sdk::{AuthoringSession, EditReceipt, EditSession, SaveReceipt, SdkError, Version};

#[derive(Clone)]
pub struct RuntimeHost {
    pub(crate) runtime: Arc<RwLock<Runtime>>,
    pristine_views: Arc<std::sync::Mutex<std::collections::HashMap<SessionId, Version>>>,
    pub(crate) client: ClientId,
    notifiers: Arc<std::sync::Mutex<Vec<async_channel::Sender<()>>>>,
    prepared_views: Arc<
        std::sync::Mutex<
            std::collections::HashMap<SessionId, Result<crate::document::PreparedDocument, String>>,
        >,
    >,
}
impl Default for RuntimeHost {
    fn default() -> Self {
        Self::new()
    }
}
pub(crate) struct HostedInteraction {
    host: RuntimeHost,
    session: SessionId,
}
impl Drop for HostedInteraction {
    fn drop(&mut self) {
        let _ = self
            .host
            .write()
            .end_interaction(self.host.client_id(), self.session);
    }
}
impl RuntimeHost {
    pub(crate) fn interaction(
        &self,
        session: SessionId,
    ) -> zipzi_runtime::Result<HostedInteraction> {
        self.write().begin_interaction(self.client_id(), session)?;
        Ok(HostedInteraction {
            host: self.clone(),
            session,
        })
    }
    pub(crate) fn dispatch_job(
        &self,
        job: zipzi_runtime::JobId,
    ) -> zipzi_runtime::Result<zipzi_runtime::Work> {
        let mut runtime = self.write();
        match runtime.take_work(job) {
            Ok(work) => Ok(work),
            Err(error) => {
                runtime.cancel_queued_job(self.client_id(), job)?;
                runtime.release_job(self.client_id(), job)?;
                Err(error)
            }
        }
    }
    pub fn new() -> Self {
        let mut runtime = Runtime::default();
        let client = runtime.register_client().expect("initial Editor client");
        Self {
            runtime: Arc::new(RwLock::new(runtime)),
            pristine_views: Default::default(),
            client,
            notifiers: Default::default(),
            prepared_views: Default::default(),
        }
    }
    pub fn read(&self) -> RwLockReadGuard<'_, Runtime> {
        self.runtime.read().unwrap_or_else(|p| p.into_inner())
    }
    pub fn write(&self) -> RuntimeWrite<'_> {
        let guard = self
            .runtime
            .try_write()
            .unwrap_or_else(|error| match error {
                std::sync::TryLockError::Poisoned(p) => p.into_inner(),
                std::sync::TryLockError::WouldBlock => {
                    panic!("Runtime write overlapped a read: finish the read before mutation")
                }
            });
        let stamp = guard.host_event_stamp();
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
        session: SessionId,
        prepared: anyhow::Result<crate::document::PreparedDocument>,
    ) {
        self.prepared_views
            .lock()
            .unwrap()
            .insert(session, prepared.map_err(|error| error.to_string()));
    }
    pub(crate) fn take_prepared_view(
        &self,
        session: SessionId,
    ) -> anyhow::Result<Option<crate::document::PreparedDocument>> {
        self.prepared_views
            .lock()
            .unwrap()
            .remove(&session)
            .transpose()
            .map_err(anyhow::Error::msg)
    }
    pub(crate) fn mark_pristine_view(&self, session: SessionId, version: Version) {
        let live: std::collections::HashSet<_> = self
            .read()
            .list_sessions()
            .into_iter()
            .map(|info| info.id)
            .collect();
        let mut pristine = self.pristine_views.lock().unwrap();
        pristine.retain(|id, _| live.contains(id));
        pristine.insert(session, version);
    }
    pub(crate) fn pristine_view(&self, session: SessionId) -> Option<Version> {
        self.pristine_views.lock().unwrap().get(&session).copied()
    }
    pub(crate) fn forget_pristine_view(&self, session: SessionId) {
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
    stamp: (zipzi_runtime::EventCursor, u64),
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
        let changed = self.guard.as_ref().unwrap().host_event_stamp() != self.stamp;
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
    pub(crate) id: SessionId,
    client: ClientId,
    path: Option<std::path::PathBuf>,
    cursor: EventCursor,
    needs_resync: bool,
    management_changed: bool,
    preview: PreviewState,
}
pub(crate) struct SessionRead<'a> {
    guard: RwLockReadGuard<'a, Runtime>,
    id: SessionId,
}
impl Deref for SessionRead<'_> {
    type Target = AuthoringSession;
    fn deref(&self) -> &Self::Target {
        self.guard
            .session(self.id)
            .expect("editable hosted document")
    }
}
pub(crate) struct EditRead<'a> {
    guard: RwLockReadGuard<'a, Runtime>,
    client: ClientId,
    id: EditId,
}
impl Deref for EditRead<'_> {
    type Target = EditSession<'static>;
    fn deref(&self) -> &Self::Target {
        self.guard
            .candidate(self.client, self.id)
            .expect("retained candidate")
    }
}
pub(crate) enum ModelRead<'a> {
    Formal(SessionRead<'a>),
    Candidate(EditRead<'a>),
    Frozen(Arc<zipzi_sdk::AuthoringSnapshot>),
}
impl Deref for ModelRead<'_> {
    type Target = Model;
    fn deref(&self) -> &Model {
        match self {
            Self::Formal(s) => s.document(),
            Self::Candidate(e) => e.candidate_document(),
            Self::Frozen(snapshot) => snapshot.document(),
        }
    }
}

impl HostedSession {
    pub(crate) fn from_session(session: AuthoringSession) -> anyhow::Result<Self> {
        let host = RuntimeHost::new();
        let id = host.write().adopt_session(host.client, session)?;
        Self::attach(host, id)
    }
    pub(crate) fn attach(host: RuntimeHost, id: SessionId) -> anyhow::Result<Self> {
        let info = host.read().session_info(id)?;
        let cursor = host.read().event_cursor(id)?;
        Ok(Self {
            client: host.client,
            host,
            id,
            path: info.path,
            cursor,
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
        self.cursor = self.host.read().event_cursor(id)?;
        Ok(())
    }
    pub(crate) fn read(&self) -> SessionRead<'_> {
        SessionRead {
            guard: self.host.read(),
            id: self.id,
        }
    }
    pub(crate) fn document(&self) -> ModelRead<'_> {
        let guard = self.host.read();
        if guard.session(self.id).is_ok() {
            ModelRead::Formal(SessionRead { guard, id: self.id })
        } else {
            ModelRead::Frozen(
                guard
                    .source_snapshot(self.id)
                    .expect("frozen hosted source")
                    .snapshot,
            )
        }
    }
    pub(crate) fn refresh_metadata(&mut self) -> anyhow::Result<()> {
        self.path = self.host.read().session_info(self.id)?.path;
        Ok(())
    }
    pub(crate) fn project_path(&self) -> Option<&std::path::Path> {
        self.path.as_deref()
    }
    pub(crate) fn receipts(&mut self) -> zipzi_runtime::Result<Vec<EditReceipt>> {
        self.poll_receipts(false)
    }
    pub(crate) fn take_management_changed(&mut self) -> bool {
        std::mem::take(&mut self.management_changed)
    }
    fn poll_receipts(
        &mut self,
        acknowledge_local: bool,
    ) -> zipzi_runtime::Result<Vec<EditReceipt>> {
        if !acknowledge_local && std::mem::take(&mut self.needs_resync) {
            return Err(zipzi_runtime::RuntimeError::ResyncRequired);
        }
        let runtime = self.host.read();
        let events = match runtime.events_since(self.cursor) {
            Ok(events) => events,
            Err(error) => {
                self.cursor = runtime.event_cursor(self.id)?;
                return Err(error);
            }
        };
        self.cursor = runtime.event_cursor(self.id)?;
        self.management_changed |= events
            .iter()
            .any(|event| matches!(event.kind, EventKind::Saved(_) | EventKind::StateChanged(_)));
        // Local fast paths apply their own receipt. Unseen foreign changes must
        // still rebuild affected projections, even if this acknowledgment moves
        // past them or the bounded Runtime stream already has a gap.
        if acknowledge_local
            && events.iter().any(|event| {
                event.client != Some(self.client) && matches!(event.kind, EventKind::Published(_))
            })
        {
            self.needs_resync = true;
        }
        Ok(events
            .iter()
            .filter_map(|e| {
                if let EventKind::Published(r) = &e.kind {
                    Some(r.as_ref().clone())
                } else {
                    None
                }
            })
            .collect())
    }
    pub(crate) fn acknowledge_receipts(&mut self) -> Vec<EditReceipt> {
        match self.poll_receipts(true) {
            Ok(receipts) => receipts,
            Err(_) => {
                self.needs_resync = true;
                vec![]
            }
        }
    }
    pub(crate) fn begin_owned_edit(
        &self,
        label: impl Into<String>,
        expected: Option<Version>,
    ) -> zipzi_runtime::Result<HostedEdit> {
        let id = self
            .host
            .write()
            .prepare_edit(self.client, self.id, label, expected)?;
        Ok(HostedEdit {
            host: self.host.clone(),
            session: self.id,
            client: self.client,
            id: Some(id),
            interactive: false,
        })
    }
    pub(crate) fn begin_image_content_edit(
        &self,
        token: &zipzi_sdk::ImageLoadToken,
        content: Option<zipzi_core::ImageContent>,
    ) -> zipzi_runtime::Result<HostedEdit> {
        let edit = self.read().begin_image_content_edit(token, content)?;
        let id = self
            .host
            .write()
            .register_owned_edit(self.client, self.id, edit)?;
        Ok(HostedEdit {
            host: self.host.clone(),
            session: self.id,
            client: self.client,
            id: Some(id),
            interactive: false,
        })
    }
    pub(crate) fn edit<T>(
        &mut self,
        label: impl Into<String>,
        expected: Option<Version>,
        apply: impl FnOnce(&mut EditSession<'static>) -> Result<T, SdkError>,
    ) -> zipzi_runtime::Result<(T, EditReceipt)> {
        let mut edit = self.begin_owned_edit(label, expected)?;
        let value = edit.apply_sdk(false, apply)?;
        let receipt = edit.commit_to(self)?;
        Ok((value, receipt))
    }
    pub(crate) fn execute_modeling_plan(
        &mut self,
        plan: zipzi_runtime::PlanId,
        expected: Version,
    ) -> zipzi_runtime::Result<EditReceipt> {
        let edit = self.begin_owned_edit("Delete objects and clean dependencies", Some(expected))?;
        self.host
            .write()
            .promote_edit(self.client, edit.id.unwrap())?;
        self.host
            .write()
            .apply_modeling_plan(self.client, edit.id.unwrap(), plan)?;
        edit.commit_to(self)
    }
    pub(crate) fn undo(&mut self) -> zipzi_runtime::Result<EditReceipt> {
        self.host.write().undo(self.client, self.id)
    }
    pub(crate) fn redo(&mut self) -> zipzi_runtime::Result<EditReceipt> {
        self.host.write().redo(self.client, self.id)
    }
    pub(crate) fn save_project(
        &mut self,
        path: &std::path::Path,
        _: Option<Version>,
    ) -> anyhow::Result<SaveReceipt> {
        let job = self.host.write().request_save(self.client, self.id, path)?;
        let work = self.host.dispatch_job(job)?;
        let completion = work.run();
        self.host.write().complete(completion);
        let result = self.host.read().job_info(self.client, job)?.result.unwrap();
        self.host.write().release_job(self.client, job)?;
        self.refresh_metadata()?;
        match result? {
            zipzi_runtime::JobValue::Saved(receipt) => Ok(receipt),
            _ => unreachable!(),
        }
    }
    pub(crate) fn preview_values(&self) -> &PreviewValues {
        self.preview.values()
    }
    pub(crate) fn set_preview_values(&mut self, values: PreviewValues) -> anyhow::Result<bool> {
        let runtime = self.host.read();
        let sdk = runtime.session(self.id)?;
        self.preview
            .replace(sdk.document(), sdk.version().generation, values)
            .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))
    }
    pub(crate) fn preview_frame(&mut self) -> anyhow::Result<Arc<zipzi_core::DrawableFrame>> {
        let runtime = self.host.read();
        let sdk = runtime.session(self.id)?;
        self.preview
            .frame(sdk.document(), sdk.version().generation)
            .map_err(|s| anyhow::anyhow!("{}: {}", s.code, s.message))
    }
}

pub(crate) struct HostedEdit {
    host: RuntimeHost,
    session: SessionId,
    client: ClientId,
    id: Option<EditId>,
    interactive: bool,
}
impl HostedEdit {
    pub(crate) fn apply_manual_mesh(
        &mut self,
        mesh_id: &str,
        operation: zipzi_sdk::ManualMeshOperation,
    ) -> zipzi_runtime::Result<zipzi_sdk::ManualMeshTransferReport> {
        let expected = self.source_ref();
        let result = self.host.write().apply_authoring(
            self.client,
            self.id.expect("active candidate"),
            vec![zipzi_runtime::authoring::EditOperation::Modeling {
                operation: Box::new(zipzi_runtime::modeling::ModelingCommand::ManualMesh {
                    mesh_id: mesh_id.into(),
                    expected,
                    operation,
                }),
            }],
        )?;
        Ok(result.mesh_transfer.expect("manual transfer report"))
    }

    pub(crate) fn source_ref(&self) -> zipzi_runtime::SourceRef {
        let edit = self.id.expect("active candidate");
        let sequence = self
            .host
            .read()
            .edit_sequence(self.client, edit)
            .expect("active candidate sequence");
        zipzi_runtime::SourceRef::Edit {
            session: self.session,
            edit,
            base: self.base_version(),
            sequence,
        }
    }
    pub(crate) fn read(&self) -> EditRead<'_> {
        EditRead {
            guard: self.host.read(),
            client: self.client,
            id: self.id.expect("active candidate"),
        }
    }
    pub(crate) fn candidate_document(&self) -> ModelRead<'_> {
        ModelRead::Candidate(self.read())
    }
    pub(crate) fn base_version(&self) -> Version {
        self.read().base_version()
    }
    pub(crate) fn claim_interaction(&mut self) -> zipzi_runtime::Result<()> {
        self.host
            .write()
            .promote_edit(self.client, self.id.unwrap())?;
        self.host
            .write()
            .begin_interaction(self.client, self.session)?;
        self.interactive = true;
        Ok(())
    }
    pub(crate) fn interactive(mut self) -> zipzi_runtime::Result<Self> {
        self.claim_interaction()?;
        Ok(self)
    }
    pub(crate) fn apply<T>(
        &mut self,
        apply: impl FnOnce(&mut EditSession<'static>) -> anyhow::Result<T>,
    ) -> anyhow::Result<T> {
        Ok(self.apply_inner(false, apply)??)
    }
    fn apply_inner<T, E>(
        &mut self,
        recoverable: bool,
        apply: impl FnOnce(&mut EditSession<'static>) -> Result<T, E>,
    ) -> zipzi_runtime::Result<Result<T, E>> {
        let mut work = self
            .host
            .write()
            .lease_candidate(self.client, self.id.unwrap())?;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work.apply(apply)));
        let failed = result.as_ref().map_or(true, |value| value.is_err());
        let returned = self
            .host
            .write()
            .return_candidate(work, failed, recoverable);
        match result {
            Ok(value) => {
                returned?;
                Ok(value)
            }
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }
    pub(crate) fn apply_sdk<T>(
        &mut self,
        recoverable: bool,
        apply: impl FnOnce(&mut EditSession<'static>) -> Result<T, SdkError>,
    ) -> zipzi_runtime::Result<T> {
        self.apply_inner(recoverable, apply)?.map_err(Into::into)
    }
    pub(crate) fn commit_to(
        mut self,
        session: &mut HostedSession,
    ) -> zipzi_runtime::Result<EditReceipt> {
        if self.session != session.id || !self.host.same_host(&session.host) {
            return Err(zipzi_runtime::RuntimeError::InvalidReference);
        }
        let id = self.id.unwrap();
        self.host.write().promote_edit(self.client, id)?;
        let result = self.host.write().commit_edit(self.client, id);
        if result.is_ok() {
            self.id = None;
        }
        result
    }
}
impl Drop for HostedEdit {
    fn drop(&mut self) {
        if self.id.is_none() && !self.interactive {
            return;
        }
        let mut runtime = self.host.write();
        if let Some(id) = self.id {
            let _ = runtime.release_edit(self.client, id);
        }
        if self.interactive {
            let _ = runtime.end_interaction(self.client, self.session);
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
        let publish = |width: f32| -> anyhow::Result<()> {
            let edit = host.write().begin_edit(agent, session, "resize", None)?;
            host.write().apply_edit(agent, edit, |edit| {
                edit.replace_model_setup(zipzi_core::Canvas::new(
                    width,
                    100.,
                    Default::default(),
                    1.,
                ))
            })?;
            host.write().commit_edit(agent, edit)?;
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
        let edit = host
            .write()
            .prepare_edit(client, session, "preview", None)?;
        host.write().promote_edit(client, edit)?;
        let mut candidate = host.write().lease_candidate(client, edit)?;
        candidate.apply(|edit| {
            edit.replace_model_setup(zipzi_core::Canvas::new(200., 100., Default::default(), 1.))
        })?;
        host.write().return_candidate(candidate, false, false)?;
        assert!(
            receiver.try_recv().is_err(),
            "candidate bookkeeping woke formal projections"
        );
        host.write().commit_edit(client, edit)?;
        receiver.try_recv()?;
        let directory = tempfile::tempdir()?;
        let job = host
            .write()
            .request_save(client, session, directory.path())?;
        receiver.try_recv()?;
        let work = host.write().take_work(job)?;
        assert!(receiver.try_recv().is_err());
        host.write().complete(work.run());
        receiver.try_recv()?;
        host.write().release_job(client, job)?;
        assert!(receiver.try_recv().is_err());
        host.write().close_session(client, session, false)?;
        receiver.try_recv()?;
        Ok(())
    }
}

impl zipzi_runtime::editor_context::EditorContextProvider for crate::Editor {
    fn editor_context(
        &self,
        session: SessionId,
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
