use zipzi_core::{Canvas, Vec2};
use zipzi_editor_core::HostedDocument;
use zipzi_runtime::Runtime;
use zipzi_sdk::AuthoringSession;

#[test]
fn hosted_view_borrows_original_sdk_instance_and_observes_shared_publication() {
    let mut runtime = Runtime::default();
    let editor = runtime.register_client().unwrap();
    let agent = runtime.register_client().unwrap();
    let snapshot = zipzi_sdk::Snapshot::new(
        zipzi_core::state::DocumentState::new(
            "60000000-0000-4000-8000-000000000001",
            zipzi_core::ModelSetup::default(),
        )
        .unwrap(),
        Default::default(),
    )
    .unwrap();
    let original = AuthoringSession::new(snapshot);
    let version = original.version();
    let id = runtime.adopt_session(editor, original).unwrap();
    let view = HostedDocument::new(editor, id);
    assert!(std::ptr::eq(
        view.read(&runtime).unwrap(),
        runtime.session_read(editor, id).unwrap()
    ));
    assert_eq!(view.read(&runtime).unwrap().version(), version);
    runtime.attach(agent, id).unwrap();
    let draft = runtime.begin_draft(agent, version).unwrap();
    let changed = zipzi_core::ExportSettings {
        padding_pixels: 200.,
        ..Default::default()
    };
    let (draft, _) = runtime
        .batch(agent, draft, |c| {
            c.edit_document(zipzi_contract::document::DocumentAuthoringEdit::Patch {
                expected: zipzi_contract::document::DocumentValue::ExportSettings(
                    Default::default(),
                ),
                value: zipzi_contract::document::DocumentValue::ExportSettings(changed),
            })
        })
        .unwrap();
    let publication = runtime.publish(agent, draft, "resize").unwrap();
    assert_eq!(
        view.read(&runtime)
            .unwrap()
            .snapshot()
            .document()
            .setup()
            .export,
        changed
    );
    assert_eq!(view.read(&runtime).unwrap().history_state().undo_steps, 1);
    runtime.detach(editor, id).unwrap();
    assert_eq!(view.read(&runtime).unwrap_err().code, "NOT_OWNER");
    runtime.undo(agent, publication.version).unwrap();
    runtime.attach(editor, id).unwrap();
    assert_eq!(
        view.read(&runtime).unwrap().version().session,
        version.session
    );
    let history = view.read(&runtime).unwrap().history_state();
    assert_eq!((history.undo_steps, history.redo_steps), (0, 1));
}

use std::{path::Path, sync::Arc};
use zipzi_contract::document::{DocumentAuthoringEdit, DocumentValue};
use zipzi_core::Status;
use zipzi_editor_core::{Editor, EditorCommand, RuntimeHost};
use zipzi_runtime::{ClientId, JobState, SessionKey, Version};

fn canvas(width: f32) -> Canvas {
    Canvas::new(width, 100., Vec2::default(), 1.)
}
fn version(host: &RuntimeHost, session: SessionKey) -> Version {
    host.read()
        .session_read(host.client_id(), session)
        .unwrap()
        .version()
}

fn resize_draft(
    host: &RuntimeHost,
    client: ClientId,
    session: SessionKey,
    padding: f32,
) -> anyhow::Result<zipzi_sdk::DraftSource> {
    let expected = version(host, session);
    let old = host
        .read()
        .session_read(host.client_id(), session)?
        .snapshot()
        .document()
        .setup()
        .export;
    let mut runtime = host.write();
    runtime.attach(client, session)?;
    let draft = runtime.begin_draft(client, expected)?;
    let (draft, ()) = runtime.batch(client, draft, |commands| {
        commands
            .edit_document(DocumentAuthoringEdit::Patch {
                expected: DocumentValue::ExportSettings(old),
                value: DocumentValue::ExportSettings(zipzi_core::ExportSettings {
                    padding_pixels: padding,
                    ..old
                }),
            })
            .map(|_| ())
    })?;
    Ok(draft)
}

fn resize(host: &RuntimeHost, client: ClientId, session: SessionKey, width: f32) {
    let draft = resize_draft(host, client, session, width).unwrap();
    host.write()
        .publish(client, draft, "External resize")
        .unwrap();
}

fn request_save(
    host: &RuntimeHost,
    client: ClientId,
    session: SessionKey,
    path: &Path,
) -> anyhow::Result<zipzi_runtime::JobId> {
    let expected = version(host, session);
    let io = host.read().project_io().clone();
    let mut runtime = host.write();
    runtime.attach(client, session)?;
    Ok(runtime.request_save_as(client, expected, &io, path, None, Default::default())?)
}

fn history(host: &RuntimeHost, session: SessionKey) -> (usize, usize) {
    let snapshot = host.read().subscribe(host.client_id(), session).unwrap();
    (snapshot.history.undo_steps, snapshot.history.redo_steps)
}

#[test]
fn r2_external_publication_updates_projection_and_shares_editor_history_once() -> anyhow::Result<()>
{
    let mut editor = Editor::new()?;
    let inspection = editor.inspection_context_token().unwrap();
    let host = editor.runtime_host();
    let session = editor.document().unwrap().runtime_session_id();
    let agent = host.write().register_client()?;
    resize(&host, agent, session, 200.);
    assert!(editor.sync_runtime()?);
    assert_eq!(editor.inspection_context_token(), Some(inspection));
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        200.
    );
    let revision = editor.document().unwrap().properties_revision();
    assert!(!editor.sync_runtime()?);
    assert_eq!(editor.document().unwrap().properties_revision(), revision);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(history(&host, session), (0, 1));
    assert!(!editor.sync_runtime()?); // local receipt already reached its projection
    editor.execute(EditorCommand::Redo)?;
    assert_eq!(
        host.read()
            .session_read(host.client_id(), session)?
            .snapshot()
            .document()
            .setup()
            .export
            .padding_pixels,
        200.
    );
    Ok(())
}

#[test]
fn external_publication_invalidates_a_local_property_gesture() -> anyhow::Result<()> {
    use zipzi_editor_core::{PropertyPatch, PropertyValue, property as schema};
    let mut editor = Editor::new()?;
    let host = editor.runtime_host();
    let session = editor.document().unwrap().runtime_session_id();
    let agent = host.write().register_client()?;
    let candidate = resize_draft(&host, agent, session, 200.)?;
    let document = editor.document().unwrap();
    let object = document.object_index().objects[0].reference.clone();
    let values = document.property_values(&object)?;
    let key = schema::document::PADDING_PIXELS.key;
    let patch = PropertyPatch {
        scope: document.property_scope(&object),
        expected: [(key, values[&key].clone())].into(),
        changes: [(key, PropertyValue::Float(300.))].into(),
    };
    let context = editor.editing_context_token().unwrap();
    let token = editor.begin_property_edit(context, object, &patch)?;
    editor.update_property_edit(&token, &patch)?;
    host.write().publish(agent, candidate, "External resize")?;
    assert!(editor.sync_runtime()?);
    assert!(editor.update_property_edit(&token, &patch).is_err());
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        200.
    );
    Ok(())
}

#[test]
fn r2_save_completes_after_all_views_drop_then_same_session_history_reopens() -> anyhow::Result<()>
{
    let mut editor = Editor::new()?;
    let host = editor.runtime_host();
    let id = editor.active_id().unwrap();
    let session = editor.document().unwrap().runtime_session_id();
    let version = host
        .read()
        .session_read(host.client_id(), session)?
        .version();
    resize(&host, host.client_id(), session, 200.);
    editor.sync_runtime()?;
    let directory = tempfile::tempdir()?;
    let task = editor.prepare_save(id, &directory.path().join("project.zipzi"), false)?;
    let progress = task.progress();
    assert_eq!(
        progress.state().stage,
        zipzi_editor_core::ProjectSaveStage::Queued
    );
    editor.suspend_window()?;
    drop(editor);
    let mut completion = task.run();
    let state = progress.state();
    assert_eq!(state.stage, zipzi_editor_core::ProjectSaveStage::Published);
    assert_eq!(state.completed_bytes, state.total_bytes);
    assert!(state.total_bytes > 0);
    completion.publish(&host); // no Editor, Entity, WeakEntity or Window exists
    assert!(host.read().saving_job(host.client_id(), session)?.is_none());
    assert!(
        !host
            .read()
            .session_read(host.client_id(), session)?
            .is_dirty()
    );
    let mut reopened = Editor::with_runtime(host.clone());
    reopened.attach_resident_sessions()?;
    assert_eq!(reopened.document_count(), 1);
    let document = reopened.document().unwrap();
    assert_eq!(document.runtime_session_id(), session);
    assert_eq!(document.export_settings().padding_pixels, 200.);
    assert_eq!(document.history_lengths(), (1, 0));
    assert_eq!(
        host.read()
            .session_read(host.client_id(), session)?
            .version()
            .session,
        version.session
    );
    reopened.execute(EditorCommand::Undo)?;
    assert_eq!(
        reopened.document().unwrap().export_settings(),
        Default::default()
    );
    Ok(())
}

struct BrokenFilesystem {
    panic: bool,
}
impl zipzi_project::archive::ArchiveFileSystem for BrokenFilesystem {
    fn create_staging(&self, _: &Path) -> std::io::Result<zipzi_project::archive::ArchiveStaging> {
        if self.panic {
            panic!("R2 injected save panic");
        }
        Err(std::io::Error::other("R2 injected save failure"))
    }
    fn sync_file(&self, _: &std::fs::File) -> std::io::Result<()> {
        unreachable!()
    }
    fn publish_no_replace(&self, _: &Path, _: &Path) -> std::io::Result<()> {
        unreachable!()
    }
    fn publish_replace(&self, _: &Path, _: &Path) -> std::io::Result<()> {
        unreachable!()
    }
    fn sync_directory(&self, _: &Path) -> std::io::Result<()> {
        unreachable!()
    }
    fn acquire_lock(&self, path: &Path) -> std::io::Result<zipzi_project::archive::ArchiveLock> {
        zipzi_project::archive::NativeArchiveFileSystem.acquire_lock(path)
    }
}
fn broken_editor(panic: bool) -> anyhow::Result<(Editor, RuntimeHost, SessionKey)> {
    let host = RuntimeHost::new();
    let io = host.read().project_io().clone();
    host.write()
        .configure_project_io(io.with_filesystem(Arc::new(BrokenFilesystem { panic })));
    let session =
        host.write()
            .create_session(host.client_id(), &budget_id(1), canvas(100.).into())?;
    resize(&host, host.client_id(), session, 200.);
    let mut editor = Editor::with_runtime(host.clone());
    editor.attach_resident_sessions()?;
    Ok((editor, host, session))
}

#[test]
fn r2_failed_save_without_window_preserves_dirty_history_and_queryable_failure()
-> anyhow::Result<()> {
    let (mut editor, host, session) = broken_editor(false)?;
    let directory = tempfile::tempdir()?;
    let task = editor.prepare_save(
        editor.active_id().unwrap(),
        &directory.path().join("project.zipzi"),
        false,
    )?;
    editor.suspend_window()?;
    drop(editor);
    let mut completion = task.run();
    let job = completion.job_id();
    completion.publish(&host);
    assert_eq!(
        host.read().job(host.client_id(), job)?.state,
        JobState::Failed
    );
    assert!(
        host.read()
            .job(host.client_id(), job)?
            .result
            .as_ref()
            .unwrap()
            .is_err()
    );
    assert!(host.read().saving_job(host.client_id(), session)?.is_none());
    let mut reopened = Editor::with_runtime(host.clone());
    reopened.attach_resident_sessions()?;
    assert!(reopened.document().unwrap().modified());
    assert_eq!(reopened.document().unwrap().history_lengths(), (1, 0));
    assert!(reopened.document().unwrap().path().is_none());
    reopened.execute(EditorCommand::Undo)?;
    assert_eq!(
        reopened
            .document()
            .unwrap()
            .export_settings()
            .padding_pixels,
        zipzi_core::ExportSettings::default().padding_pixels
    );
    Ok(())
}

#[test]
fn r2_worker_panic_keeps_frozen_view_and_never_leaves_editor_busy() -> anyhow::Result<()> {
    let (mut editor, host, session) = broken_editor(true)?;
    let id = editor.active_id().unwrap();
    let directory = tempfile::tempdir()?;
    let task = editor.prepare_save(id, &directory.path().join("project.zipzi"), false)?;
    editor.suspend_window()?;
    let mut completion = task.run();
    completion.publish(&host);
    assert_eq!(
        host.read()
            .job(host.client_id(), completion.job_id())?
            .result
            .as_ref()
            .unwrap()
            .as_ref()
            .unwrap_err()
            .code,
        "WORKER_PANIC"
    );
    assert!(editor.finish_save(&mut completion).is_err());
    assert!(!editor.is_busy());
    assert_eq!(editor.document_count(), 1);
    assert!(host.read().saving_job(host.client_id(), session)?.is_none());
    editor.attach_resident_sessions()?;
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        200.
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.close_document(id, true)?;
    assert!(host.read().session_catalog(host.client_id())?.is_empty());
    Ok(())
}

#[test]
fn external_save_scene_creation_queries_before_workspace_sync_return_busy() -> anyhow::Result<()> {
    use zipzi_editor_core::{Document, PaletteProjection, SceneObjectType};

    let mut document = Document::new_blank()?;
    let mut selections = Vec::new();
    for kind in [
        SceneObjectType::Part,
        SceneObjectType::Mesh,
        SceneObjectType::Warp,
    ] {
        let plan = document.plan_scene_object_creation(kind, "Selected".into(), &[])?;
        selections.push(
            document
                .execute_scene_object_creation(&plan)?
                .suggested_selection
                .into_iter()
                .filter(|object| object.kind == kind.object_kind())
                .collect::<Vec<_>>(),
        );
    }
    selections.push(Vec::new());
    let host = document.runtime_host();
    let session = document.runtime_session_id();
    let agent = host.write().register_client()?;
    let target = tempfile::tempdir()?;
    let job = request_save(&host, agent, session, &target.path().join("project.zipzi"))?;

    // Retain the UI's Document after external admission, without syncing it.
    // A selection reaches handle(); an empty selection reaches collection capabilities.
    for selection in &selections {
        for (projection, kind) in [
            (PaletteProjection::Organization, SceneObjectType::Part),
            (PaletteProjection::Deformation, SceneObjectType::Mesh),
        ] {
            if projection == PaletteProjection::Deformation
                && selection
                    .first()
                    .is_some_and(|object| object.kind == zipzi_editor_core::ObjectKind::Part)
            {
                continue;
            }
            let error = document
                .validate_scene_object_creation_in(projection, kind, selection)
                .unwrap_err();
            assert_eq!(
                error.downcast_ref::<Status>().unwrap().code.as_str(),
                "PROJECT_BUSY"
            );
        }
    }
    let work = host.write().take_work(job)?;
    host.write().complete(work.run()).unwrap();
    host.write().release_job(agent, job)?;
    for selection in &selections {
        document.validate_scene_object_creation_context(SceneObjectType::Part, selection)?;
    }
    assert!(target.path().join("project.zipzi").is_file());
    Ok(())
}

#[test]
fn external_busy_command_queries_do_not_require_a_workspace_sync() -> anyhow::Result<()> {
    use zipzi_editor_core::{CommandDisabledReason, CommandId};

    let mut editor = Editor::new()?;
    let host = editor.runtime_host();
    let session = editor.document().unwrap().runtime_session_id();
    let agent = host.write().register_client()?;
    let command = editor.command(CommandId::CreateParameter);
    assert!(editor.command_state(command).enabled);
    let target = tempfile::tempdir()?;
    let job = request_save(&host, agent, session, &target.path().join("project.zipzi"))?;

    // Keep the Job queued and skip sync_runtime: this is the interval between
    // socket admission and the workspace's asynchronous change notification.
    assert!(editor.is_busy());
    assert_eq!(
        editor.command_state(command).disabled_reason,
        Some(CommandDisabledReason::Busy)
    );
    let error = editor
        .authoring_command_capability(CommandId::CreateParameter)
        .unwrap_err();
    assert_eq!(error.downcast_ref::<Status>().unwrap().code, "PROJECT_BUSY");

    host.write().cancel_job(agent, job)?;
    let work = host.write().take_work(job)?;
    host.write().complete(work.run()).unwrap();
    host.write().release_job(agent, job)?;
    assert!(!editor.is_busy());
    assert!(editor.command_state(command).enabled);
    assert_eq!(
        editor.authoring_command_capability(CommandId::CreateParameter)?,
        zipzi_contract::authoring::CapabilityState::Available
    );
    assert!(!editor.sync_runtime()?);
    assert!(editor.document().is_some());
    Ok(())
}

#[test]
fn r2_external_busy_operation_freezes_then_restores_the_editor_projection() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let inspection = editor.inspection_context_token().unwrap();
    let host = editor.runtime_host();
    let session = editor.document().unwrap().runtime_session_id();
    let agent = host.write().register_client()?;
    resize(&host, agent, session, 200.);
    editor.sync_runtime()?;
    let target = tempfile::tempdir()?;
    let job = request_save(&host, agent, session, &target.path().join("project.zipzi"))?;
    let work = host.write().take_work(job)?;
    assert!(editor.sync_runtime()?);
    assert!(editor.is_busy());
    assert!(editor.document().is_none());
    assert_eq!(editor.inspection_context_token(), Some(inspection));
    host.write().complete(work.run()).unwrap();
    assert!(editor.sync_runtime()?);
    assert!(!editor.is_busy());
    assert_eq!(editor.inspection_context_token(), Some(inspection));
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        200.
    );
    assert!(!editor.document().unwrap().modified());
    assert_eq!(host.read().job(agent, job)?.state, JobState::Succeeded);
    Ok(())
}

#[test]
fn r2_tab_detach_retains_unsaved_session_and_history_as_a_resident() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let host = editor.runtime_host();
    let session = editor.document().unwrap().runtime_session_id();
    resize(&host, host.client_id(), session, 200.);
    editor.sync_runtime()?;
    let id = editor.active_id().unwrap();
    editor.detach_document(id)?;
    assert_eq!(editor.document_count(), 0);
    assert_eq!(host.read().session_catalog(host.client_id())?.len(), 1);
    editor.attach_resident_sessions()?;
    assert_eq!(editor.document().unwrap().runtime_session_id(), session);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert!(editor.document().unwrap().modified());
    Ok(())
}

#[test]
fn r2_load_without_window_registers_resident_and_does_not_require_the_initiating_view()
-> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let mut source = zipzi_editor_core::Document::new_blank()?;
    source.save(&directory.path().join("project.zipzi"))?;
    let mut editor = Editor::empty();
    let host = editor.runtime_host();
    let task = editor
        .prepare_load(zipzi_editor_core::LoadRequest::Open(
            directory.path().join("project.zipzi"),
        ))?
        .unwrap();
    drop(editor);
    let mut completion = task.run();
    completion.publish(&host);
    assert_eq!(host.read().session_catalog(host.client_id())?.len(), 1);
    let session = host.read().session_catalog(host.client_id())?[0].session;
    let mut reopened = Editor::with_runtime(host);
    reopened.attach_resident_sessions()?;
    assert_eq!(reopened.document().unwrap().runtime_session_id(), session);
    assert!(reopened.document().unwrap().path().is_some());
    Ok(())
}

#[test]
fn r2_load_dispatch_rejection_releases_provisional_job_and_path() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let mut editor = Editor::empty();
    let host = editor.runtime_host();
    let request = |name: &str| zipzi_editor_core::LoadRequest::Open(directory.path().join(name));
    let mut loads = Vec::new();
    for index in 0..zipzi_runtime::RuntimeLimits::default().running_jobs {
        loads.push(
            editor
                .prepare_load(request(&format!("loading{index}")))?
                .unwrap(),
        );
    }
    assert!(editor.prepare_load(request("third")).is_err());
    assert_eq!(
        host.read().client_jobs(host.client_id())?.len(),
        loads.len()
    );
    assert!(
        host.read()
            .project_session(host.client_id(), &directory.path().join("third"))?
            .is_none()
    );
    for load in loads {
        assert!(editor.finish_load(load.run()).is_err());
    }
    assert_eq!(editor.pending_io_count(), 0);
    assert!(host.read().client_jobs(host.client_id())?.is_empty());
    assert!(host.read().session_catalog(host.client_id())?.is_empty());
    // The rejected target can be requested again, rather than staying reserved.
    let retry = editor.prepare_load(request("third"))?.unwrap();
    assert!(editor.finish_load(retry.run()).is_err());
    Ok(())
}

#[test]
fn r2_pristine_blank_survives_detach_but_external_writes_cannot_hide_behind_it()
-> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let host = editor.runtime_host();
    let session = editor.document().unwrap().runtime_session_id();
    assert!(!editor.document().unwrap().modified());
    editor.detach_document(editor.active_id().unwrap())?;
    editor.attach_resident_sessions()?;
    assert!(!editor.document().unwrap().modified());
    // An external publication is visible even before a view polls its receipt.
    let agent = host.write().register_client()?;
    resize(&host, agent, session, 200.);
    assert!(editor.document().unwrap().modified());
    assert!(
        editor
            .close_document(editor.active_id().unwrap(), false)
            .is_err()
    );
    editor.sync_runtime()?;
    assert!(editor.document().unwrap().modified());
    Ok(())
}

#[test]
fn inspection_context_survives_local_save_and_runtime_notifications() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let id = editor.active_id().unwrap();
    let inspection = editor.inspection_context_token().unwrap();
    let directory = tempfile::tempdir()?;
    let task = editor.prepare_save(id, &directory.path().join("project.zipzi"), false)?;
    assert!(editor.document().is_none());
    assert_eq!(editor.inspection_context_token(), Some(inspection));
    editor.sync_runtime()?;
    assert_eq!(editor.inspection_context_token(), Some(inspection));

    let mut completion = task.run();
    editor.finish_save(&mut completion)?;
    assert_eq!(editor.inspection_context_token(), Some(inspection));
    assert!(editor.sync_runtime()?);
    assert_eq!(editor.inspection_context_token(), Some(inspection));
    assert!(editor.document().unwrap().path().is_some());
    assert!(!editor.sync_runtime()?);
    Ok(())
}

#[test]
fn r2_fast_external_save_still_requests_a_redraw_after_metadata_changes() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let inspection = editor.inspection_context_token().unwrap();
    let host = editor.runtime_host();
    let session = editor.document().unwrap().runtime_session_id();
    let client = host.write().register_client()?;
    resize(&host, client, session, 150.);
    editor.sync_runtime()?;
    let directory = tempfile::tempdir()?;
    let job = request_save(
        &host,
        client,
        session,
        &directory.path().join("project.zipzi"),
    )?;
    let work = host.write().take_work(job)?;
    host.write().complete(work.run()).unwrap(); // Busy starts and ends between UI polls.
    assert!(editor.sync_runtime()?);
    assert_eq!(editor.inspection_context_token(), Some(inspection));
    assert!(!editor.document().unwrap().modified());
    assert!(editor.document().unwrap().path().is_some());
    assert!(!editor.sync_runtime()?);
    Ok(())
}

use zipzi_editor_core::{EditingContext, TimelineCommand};
#[test]
fn external_publication_invalidates_a_local_timeline_gesture() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let host = editor.runtime_host();
    let id = editor.active_id().unwrap();
    let session = editor.document().unwrap().runtime_session_id();
    editor.set_editing_context(EditingContext::Animation)?;
    editor.create_timeline_motion(id)?;
    let agent = host.write().register_client()?;
    let external = resize_draft(&host, agent, session, 200.)?;
    let token = editor.begin_timeline_edit(id, "drag")?;
    editor.update_timeline_edit(id, token, TimelineCommand::ScaleTime(2.))?;
    host.write().publish(agent, external, "External resize")?;
    editor.sync_runtime()?;
    assert!(editor.timeline_edit().is_none());

    let expected = version(&host, session);
    let obsolete = host.write().begin_draft(agent, expected)?;
    let token = editor.begin_timeline_edit(id, "commit drag")?;
    editor.update_timeline_edit(id, token, TimelineCommand::ScaleTime(3.))?;
    editor.finish_timeline_edit(id, token)?;
    assert_eq!(
        host.write()
            .publish(agent, obsolete, "Stale")
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    Ok(())
}
#[test]
fn external_worker_failure_before_the_first_view_poll_preserves_the_document() -> anyhow::Result<()>
{
    let (mut editor, host, session) = broken_editor(true)?;
    let inspection = editor.inspection_context_token();
    let agent = host.write().register_client()?;
    let dir = tempfile::tempdir()?;
    let job = request_save(&host, agent, session, &dir.path().join("project.zipzi"))?;
    let work = host.write().take_work(job)?;
    host.write().complete(work.run()).unwrap();
    assert_eq!(host.read().job(agent, job)?.state, JobState::Failed);
    editor.sync_runtime()?;
    assert_eq!(editor.inspection_context_token(), inspection);
    assert_eq!(
        editor.document().unwrap().export_settings().padding_pixels,
        200.
    );
    assert!(!editor.is_busy());
    Ok(())
}
#[test]
fn r2_quit_cannot_finish_with_detached_save_running() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let host = editor.runtime_host();
    let id = editor.active_id().unwrap();
    let session = editor.document().unwrap().runtime_session_id();
    let dir = tempfile::tempdir()?;
    let _work = editor.prepare_save(id, &dir.path().join("project.zipzi"), false)?;
    editor.detach_document(id)?;
    let exit = editor.request_exit();
    eprintln!(
        "request_exit={exit:?}, next_close={:?}, state={:?}",
        editor.next_close(),
        host.read().saving_job(host.client_id(), session)?
    );
    assert!(
        exit.is_err() || editor.next_close().is_some(),
        "Exit would call cx.quit while save is still running"
    );
    Ok(())
}

use zipzi_editor_core::Document;
use zipzi_sdk::rectangle_mesh;
fn budget_id(n: u32) -> String {
    format!("66000000-0000-4000-8000-{n:012x}")
}
#[test]
fn typed_remesh_respects_the_resident_sdk_history_policy() -> anyhow::Result<()> {
    let mut sdk = crate::support::fixture_session(&budget_id(1), canvas(100.))?;
    let mesh = rectangle_mesh(
        &budget_id(3),
        "mesh",
        "",
        Vec2::default(),
        Vec2::new(10., 10.),
    )?;
    crate::support::publish_fixture(&mut sdk, "Fixture", |commands| {
        crate::support::insert_fixture_mesh(commands, mesh)
    })?;
    sdk.set_history_limits(zipzi_sdk::HistoryLimits {
        max_steps: 1,
        target_bytes: 1,
    });
    let host = RuntimeHost::new();
    let session = host.write().adopt_session(host.client_id(), sdk)?;
    let mut document = Document::attach_runtime(host.clone(), session)?;
    let mesh = zipzi_editor_core::ObjectRef::new(zipzi_editor_core::ObjectKind::Mesh, budget_id(3));
    document.generate_mesh_grid(
        &mesh,
        zipzi_editor_core::MeshGridOptions {
            columns: 8,
            rows: 8,
        },
        document.mesh_generation_revision(),
    )?;
    assert_eq!(history(&host, session), (1, 0));
    document.undo()?;
    assert_eq!(history(&host, session), (0, 1));
    Ok(())
}
#[test]
fn r2_sync_save_dispatch_failure_returns_ownership() -> anyhow::Result<()> {
    let host = RuntimeHost::new();
    let mut sessions = Vec::new();
    let workers = zipzi_runtime::RuntimeLimits::default().running_jobs;
    for n in 1..=workers + 1 {
        sessions.push(host.write().create_session(
            host.client_id(),
            &budget_id(n as u32),
            canvas(100.).into(),
        )?);
    }
    let mut document = Document::attach_runtime(host.clone(), sessions[workers])?;
    let dir = tempfile::tempdir()?;
    let mut running = Vec::new();
    for (i, session) in sessions[..workers].iter().enumerate() {
        let job = request_save(
            &host,
            host.client_id(),
            *session,
            &dir.path().join(format!("running{i}")),
        )?;
        running.push(host.write().take_work(job)?);
    }
    let result = document.save(&dir.path().join("rejected"));
    let state = host
        .read()
        .saving_job(host.client_id(), sessions[workers])?;
    eprintln!("synchronous save result={result:?}; session state={state:?}");
    assert!(result.is_err());
    assert_eq!(
        state, None,
        "failed dispatch left an unreachable queued save"
    );
    assert_eq!(host.read().client_jobs(host.client_id())?.len(), workers);
    for work in running {
        host.write().complete(work.run()).unwrap();
    }
    document.save(&dir.path().join("rejected"))?;
    Ok(())
}

#[test]
fn r2_quit_waits_for_foreign_provisional_loads() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let host = editor.runtime_host();
    let agent = host.write().register_client()?;
    let directory = tempfile::tempdir()?;
    let session = editor.document().unwrap().runtime_session_id();
    let source = version(&host, session);
    host.write().attach(agent, session)?;
    let job = host.write().request_import(
        agent,
        source,
        &directory.path().join("missing.psd"),
        zipzi_contract::kernel_wire::ImportFormat::Psd,
        Default::default(),
    )?;
    assert!(editor.request_exit().is_err());
    assert!(!editor.exiting());
    host.write().cancel_job(agent, job)?;
    let work = host.write().take_work(job)?;
    host.write().complete(work.run()).unwrap();
    host.write().release_job(agent, job)?;
    editor.request_exit()?;
    assert!(editor.next_close().is_some());
    Ok(())
}

#[test]
fn r6_direct_batches_share_human_refinement_history_and_reject_old_candidates() -> anyhow::Result<()>
{
    use zipzi_editor_core::{
        AnimationPreviewMode, MotionSelection, TimelineChannelId, TimelineCommand, TimelineKeyRef,
        WorkspaceKind,
    };
    let (document, _assets) = crate::support::animation_document()?;
    let mut editor = crate::support::editor_from_document(document);
    let host = editor.runtime_host();
    let session = editor.document().unwrap().runtime_session_id();
    let agent = host.write().register_client()?;
    host.write().attach(agent, session)?;
    let mesh = editor.document().unwrap().layers()[0].id.clone();
    let motion = editor.document().unwrap().motion_resources()[0]
        .selection
        .clone();
    let original = editor
        .document()
        .unwrap()
        .motion_definition(&motion.clip_id)
        .unwrap();
    let track = original.tracks[0].clone();
    let parameter = editor.document().unwrap().parameters()[0].id.clone();
    let endpoint_key = track.segments[0].end().id.clone();

    let expected = version(&host, session);
    let edit = host.write().begin_draft(agent, expected)?;
    let (edit, ()) = host.write().batch(agent, edit, |e| {
        e.rename(zipzi_sdk::NameTarget::Mesh(&mesh), "Agent draft".into())?;
        let mut clip = e.read_document()?.motion(&motion.clip_id).unwrap().clone();
        Arc::make_mut(&mut clip.tracks[0].segments)[0]
            .end_mut()
            .value = 0.8;
        e.replace_motion(clip)
    })?;
    let job = host
        .write()
        .request_publish(agent, edit, "Agent batch", Default::default())?;
    let completion = host.write().take_work(job)?.run();
    host.write().complete(completion).unwrap();
    host.write().release_job(agent, job)?;
    assert!(editor.sync_runtime()?);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert_eq!(editor.document().unwrap().layers()[0].name, "Agent draft");

    let expected = version(&host, session);
    let obsolete = host.write().begin_draft(agent, expected)?;
    let (obsolete, ()) = host.write().batch(agent, obsolete, |e| {
        e.rename(
            zipzi_sdk::NameTarget::Mesh(&mesh),
            "Must not overwrite".into(),
        )
    })?;
    editor.execute(EditorCommand::Select(mesh.clone()))?;
    editor.execute(EditorCommand::PreviewName("Human polish".into()))?;
    editor.execute(EditorCommand::FinishEdit)?;
    editor.set_workspace(WorkspaceKind::Animation)?;
    let document = editor.active_id().unwrap();
    editor.set_animation_preview_mode(document, AnimationPreviewMode::Editing)?;
    editor.select_motion(
        document,
        MotionSelection {
            clip_id: motion.clip_id.clone(),
            entry: motion.entry,
        },
    )?;
    editor.seek_timeline(document, 2.)?;
    editor.execute_timeline(
        document,
        TimelineCommand::WriteValue {
            channel: TimelineChannelId::Parameter(parameter),
            track_id: Some(track.id.clone()),
            selected_key: Some(TimelineKeyRef {
                clip_id: motion.clip_id.clone(),
                track_id: track.id.clone(),
                key_id: endpoint_key.clone(),
            }),
            value: 0.45,
        },
    )?;
    assert_eq!(editor.document().unwrap().history_lengths(), (3, 0));
    let error = host
        .write()
        .publish(agent, obsolete, "Old candidate")
        .unwrap_err();
    assert_eq!(error.code, "STALE_SOURCE");
    assert_eq!(editor.document().unwrap().layers()[0].name, "Human polish");
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 1));
    editor.execute(EditorCommand::Redo)?;

    let expected = version(&host, session);
    let next = host.write().begin_draft(agent, expected)?;
    let (next, ()) = host.write().batch(agent, next, |e| {
        assert_eq!(e.read_document()?.mesh(&mesh).unwrap().name, "Human polish");
        let mut clip = e.read_document()?.motion(&motion.clip_id).unwrap().clone();
        assert_eq!(clip.tracks[0].segments[0].end().id, endpoint_key);
        assert_eq!(clip.tracks[0].segments[0].end().value, 0.45);
        e.rename(zipzi_sdk::NameTarget::Mesh(&mesh), "Agent continued".into())?;
        clip.tracks[0].initial.value = 0.2;
        e.replace_motion(clip)
    })?;
    host.write().publish(agent, next, "Continue human work")?;
    host.write().release_client(agent);
    assert!(editor.sync_runtime()?);
    assert_eq!(editor.document().unwrap().history_lengths(), (4, 0));
    assert_eq!(
        editor.document().unwrap().layers()[0].name,
        "Agent continued"
    );
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(editor.document().unwrap().layers()[0].name, "Human polish");
    let restored = editor
        .document()
        .unwrap()
        .motion_definition(&motion.clip_id)
        .unwrap();
    assert_eq!(restored.tracks[0].initial.value, 0.);
    assert_eq!(restored.tracks[0].segments[0].end().value, 0.45);
    assert_eq!(restored.tracks[0].segments[0].end().id, endpoint_key);
    Ok(())
}
