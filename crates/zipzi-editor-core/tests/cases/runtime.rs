use zipzi_core::{Canvas, Vec2};
use zipzi_editor_core::HostedDocument;
use zipzi_runtime::Runtime;
use zipzi_sdk::AuthoringSession;

#[test]
fn hosted_view_borrows_original_sdk_instance_and_observes_shared_publication() {
    let mut runtime = Runtime::default();
    let editor = runtime.register_client().unwrap();
    let agent = runtime.register_client().unwrap();
    let original = AuthoringSession::new(
        "60000000-0000-4000-8000-000000000001",
        Canvas::new(100., 100., Vec2::default(), 1.),
    )
    .unwrap();
    let version = original.version();
    let id = runtime.adopt_session(editor, original).unwrap();
    let view = HostedDocument::new(id);
    assert!(std::ptr::eq(
        view.read(&runtime).unwrap(),
        runtime.session(id).unwrap()
    ));
    assert_eq!(view.read(&runtime).unwrap().version(), version);
    let edit = runtime
        .begin_edit(agent, id, "resize", Some(version))
        .unwrap();
    let changed = Canvas::new(200., 100., Vec2::default(), 1.);
    runtime
        .apply_edit(agent, edit, |e| e.replace_model_setup(changed))
        .unwrap();
    runtime.commit_edit(agent, edit).unwrap();
    assert_eq!(view.read(&runtime).unwrap().source_canvas(), Some(changed));
    assert_eq!(view.read(&runtime).unwrap().history_lengths(), (1, 0));
    runtime.detach_session(editor, id).unwrap();
    assert_eq!(view.read(&runtime).unwrap().history_lengths(), (1, 0));
    runtime.undo(agent, id).unwrap();
    assert_eq!(
        view.read(&runtime).unwrap().version().session_id,
        version.session_id
    );
    assert_eq!(view.read(&runtime).unwrap().history_lengths(), (0, 1));
}

use std::{path::Path, sync::Arc};
use zipzi_editor_core::{Editor, EditorCommand, RuntimeHost};
use zipzi_runtime::{JobState, RuntimeError, SessionId, SessionState};

fn canvas(width: f32) -> Canvas {
    Canvas::new(width, 100., Vec2::default(), 1.)
}
fn resize(host: &RuntimeHost, client: zipzi_runtime::ClientId, session: SessionId, width: f32) {
    let edit = host
        .write()
        .begin_edit(client, session, "External resize", None)
        .unwrap();
    host.write()
        .apply_edit(client, edit, |e| e.replace_model_setup(canvas(width)))
        .unwrap();
    host.write().commit_edit(client, edit).unwrap();
}

#[test]
fn r2_external_publication_updates_projection_and_shares_editor_history_once() -> anyhow::Result<()>
{
    let mut editor = Editor::new()?;
    let host = editor.runtime_host();
    let session = editor.document().unwrap().runtime_session_id();
    let agent = host.write().register_client()?;
    resize(&host, agent, session, 200.);
    assert!(editor.sync_runtime()?);
    assert_eq!(
        editor.document().unwrap().source_canvas(),
        Some(canvas(200.))
    );
    let revision = editor.document().unwrap().properties_revision();
    assert!(!editor.sync_runtime()?);
    assert_eq!(editor.document().unwrap().properties_revision(), revision);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(host.read().session(session)?.history_lengths(), (0, 1));
    assert!(!editor.sync_runtime()?); // local receipt already reached its projection
    editor.execute(EditorCommand::Redo)?;
    assert_eq!(
        host.read().session(session)?.source_canvas(),
        Some(canvas(200.))
    );
    Ok(())
}

#[test]
fn r2_human_interaction_blocks_another_clients_commit_without_consuming_it() -> anyhow::Result<()> {
    use zipzi_editor_core::{PropertyPatch, PropertyValue, property as schema};
    let mut editor = Editor::new()?;
    let host = editor.runtime_host();
    let session = editor.document().unwrap().runtime_session_id();
    let agent = host.write().register_client()?;
    let candidate = host.write().begin_edit(agent, session, "agent", None)?;
    host.write()
        .apply_edit(agent, candidate, |e| e.replace_model_setup(canvas(200.)))?;
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
    assert_eq!(
        host.write().commit_edit(agent, candidate),
        Err(RuntimeError::Busy)
    );
    assert_eq!(
        host.read().edit_state(agent, candidate)?,
        zipzi_runtime::EditState::Editing
    );
    editor.cancel_property_edit(&token);
    host.write().commit_edit(agent, candidate)?;
    editor.sync_runtime()?;
    assert_eq!(
        editor.document().unwrap().source_canvas(),
        Some(canvas(200.))
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
    let version = host.read().session(session)?.version();
    resize(&host, host.client_id(), session, 200.);
    editor.sync_runtime()?;
    let directory = tempfile::tempdir()?;
    let task = editor.prepare_save(id, directory.path(), false)?;
    editor.suspend_window()?;
    drop(editor);
    let mut completion = task.run();
    completion.publish(&host); // no Editor, Entity, WeakEntity or Window exists
    assert_eq!(
        host.read().session_info(session)?.state,
        SessionState::Ready
    );
    assert!(!host.read().session(session)?.modified());
    let mut reopened = Editor::with_runtime(host.clone());
    reopened.attach_resident_sessions()?;
    assert_eq!(reopened.document_count(), 1);
    let document = reopened.document().unwrap();
    assert_eq!(document.runtime_session_id(), session);
    assert_eq!(document.source_canvas(), Some(canvas(200.)));
    assert_eq!(document.history_lengths(), (1, 0));
    assert_eq!(
        host.read().session(session)?.version().session_id,
        version.session_id
    );
    reopened.execute(EditorCommand::Undo)?;
    assert!(reopened.document().unwrap().source_canvas().is_none());
    Ok(())
}

struct BrokenFilesystem {
    panic: bool,
}
impl zipzi_sdk::FileSystem for BrokenFilesystem {
    fn write_new(&self, _: &Path, _: &[u8]) -> std::io::Result<()> {
        if self.panic {
            panic!("R2 injected save panic");
        }
        Err(std::io::Error::other("R2 injected save failure"))
    }
    fn sync_directory(&self, _: &Path) -> std::io::Result<()> {
        Ok(())
    }
    fn rename(&self, _: &Path, _: &Path) -> std::io::Result<()> {
        Ok(())
    }
}
fn broken_editor(panic: bool) -> anyhow::Result<(Editor, RuntimeHost, SessionId)> {
    let host = RuntimeHost::new();
    let mut sdk = AuthoringSession::with_filesystem(
        "62000000-0000-4000-8000-000000000001",
        canvas(100.),
        Arc::new(BrokenFilesystem { panic }),
    )?;
    sdk.edit("unsaved", None, |e| e.replace_model_setup(canvas(200.)))?;
    let session = host.write().adopt_session(host.client_id(), sdk)?;
    let mut editor = Editor::with_runtime(host.clone());
    editor.attach_resident_sessions()?;
    Ok((editor, host, session))
}

#[test]
fn r2_failed_save_without_window_preserves_dirty_history_and_queryable_failure()
-> anyhow::Result<()> {
    let (mut editor, host, session) = broken_editor(false)?;
    let directory = tempfile::tempdir()?;
    let task = editor.prepare_save(editor.active_id().unwrap(), directory.path(), false)?;
    editor.suspend_window()?;
    drop(editor);
    let mut completion = task.run();
    let job = completion.job_id();
    completion.publish(&host);
    assert_eq!(
        host.read().job_info(host.client_id(), job)?.state,
        JobState::Failed
    );
    assert!(
        host.read()
            .job_info(host.client_id(), job)?
            .result
            .unwrap()
            .is_err()
    );
    assert_eq!(
        host.read().session_info(session)?.state,
        SessionState::Ready
    );
    let mut reopened = Editor::with_runtime(host.clone());
    reopened.attach_resident_sessions()?;
    assert!(reopened.document().unwrap().modified());
    assert_eq!(reopened.document().unwrap().history_lengths(), (1, 0));
    assert!(reopened.document().unwrap().path().is_none());
    reopened.execute(EditorCommand::Undo)?;
    assert_eq!(
        reopened.document().unwrap().source_canvas(),
        Some(canvas(100.))
    );
    Ok(())
}

#[test]
fn r2_worker_panic_keeps_frozen_view_and_never_leaves_editor_busy() -> anyhow::Result<()> {
    let (mut editor, host, session) = broken_editor(true)?;
    let id = editor.active_id().unwrap();
    let directory = tempfile::tempdir()?;
    let task = editor.prepare_save(id, directory.path(), false)?;
    editor.suspend_window()?;
    let mut completion = task.run();
    completion.publish(&host);
    assert!(editor.finish_save(&mut completion).is_err());
    assert!(!editor.is_busy());
    assert_eq!(editor.document_count(), 1);
    assert!(editor.document().is_none()); // frozen view only; no fake Ready SDK
    assert!(matches!(
        host.read().session_info(session)?.state,
        SessionState::Faulted(_)
    ));
    assert_eq!(
        host.read()
            .source_snapshot(session)?
            .snapshot
            .document()
            .source_canvas(),
        Some(canvas(200.))
    );
    assert!(host.read().session_info(session)?.fault.is_some());
    editor.close_document(id, true)?;
    assert!(host.read().list_sessions().is_empty());
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
    let job = host.write().request_save(agent, session, target.path())?;

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
    assert!(matches!(
        error.downcast_ref::<zipzi_runtime::RuntimeError>(),
        Some(zipzi_runtime::RuntimeError::Busy)
    ));

    host.write().cancel_queued_job(agent, job)?;
    host.write().release_job(agent, job)?;
    assert!(!editor.is_busy());
    assert!(editor.command_state(command).enabled);
    assert_eq!(
        editor.authoring_command_capability(CommandId::CreateParameter)?,
        zipzi_sdk::CapabilityState::Available
    );
    assert!(editor.sync_runtime()?);
    assert!(editor.document().is_some());
    Ok(())
}

#[test]
fn r2_external_busy_operation_freezes_then_restores_the_editor_projection() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let host = editor.runtime_host();
    let session = editor.document().unwrap().runtime_session_id();
    let agent = host.write().register_client()?;
    resize(&host, agent, session, 200.);
    editor.sync_runtime()?;
    let target = tempfile::tempdir()?;
    let job = host.write().request_save(agent, session, target.path())?;
    let work = host.write().take_work(job)?;
    assert!(editor.sync_runtime()?);
    assert!(editor.is_busy());
    assert!(editor.document().is_none());
    assert!(host.write().complete(work.run()));
    assert!(editor.sync_runtime()?);
    assert!(!editor.is_busy());
    assert_eq!(
        editor.document().unwrap().source_canvas(),
        Some(canvas(200.))
    );
    assert!(!editor.document().unwrap().modified());
    assert_eq!(host.read().job_info(agent, job)?.state, JobState::Succeeded);
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
    assert_eq!(host.read().list_sessions().len(), 1);
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
    source.save(directory.path())?;
    let mut editor = Editor::empty();
    let host = editor.runtime_host();
    let task = editor
        .prepare_load(zipzi_editor_core::LoadRequest::Open(
            directory.path().into(),
        ))?
        .unwrap();
    drop(editor);
    let mut completion = task.run();
    completion.publish(&host);
    assert_eq!(host.read().list_sessions().len(), 1);
    let session = host.read().list_sessions()[0].id;
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
    let first = editor.prepare_load(request("first"))?.unwrap();
    let second = editor.prepare_load(request("second"))?.unwrap();
    assert!(editor.prepare_load(request("third")).is_err());
    assert_eq!(host.read().list_jobs(host.client_id())?.len(), 2);
    assert!(
        host.read()
            .find_project(&directory.path().join("third"))?
            .is_none()
    );
    assert!(editor.finish_load(first.run()).is_err());
    assert!(editor.finish_load(second.run()).is_err());
    assert_eq!(editor.pending_io_count(), 0);
    assert!(host.read().list_jobs(host.client_id())?.is_empty());
    assert!(host.read().list_sessions().is_empty());
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
fn r2_fast_external_save_still_requests_a_redraw_after_metadata_changes() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let host = editor.runtime_host();
    let session = editor.document().unwrap().runtime_session_id();
    let client = host.write().register_client()?;
    resize(&host, client, session, 150.);
    editor.sync_runtime()?;
    let directory = tempfile::tempdir()?;
    let job = host
        .write()
        .request_save(client, session, directory.path())?;
    let work = host.write().take_work(job)?;
    host.write().complete(work.run()); // Busy starts and ends between UI polls.
    assert!(editor.sync_runtime()?);
    assert!(!editor.document().unwrap().modified());
    assert!(editor.document().unwrap().path().is_some());
    assert!(!editor.sync_runtime()?);
    Ok(())
}

use zipzi_editor_core::{EditingContext, TimelineCommand};
struct PanicFs;
impl zipzi_sdk::FileSystem for PanicFs {
    fn write_new(&self, _: &Path, _: &[u8]) -> std::io::Result<()> {
        panic!("review injected worker panic")
    }
    fn sync_directory(&self, _: &Path) -> std::io::Result<()> {
        Ok(())
    }
    fn rename(&self, _: &Path, _: &Path) -> std::io::Result<()> {
        Ok(())
    }
}
#[test]
fn r2_timeline_interaction_blocks_foreign_commit() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let host = editor.runtime_host();
    let id = editor.active_id().unwrap();
    let session = editor.document().unwrap().runtime_session_id();
    editor.set_editing_context(EditingContext::Animation)?;
    editor.create_timeline_motion(id)?;
    let agent = host.write().register_client()?;
    let external = host.write().begin_edit(agent, session, "foreign", None)?;
    host.write()
        .apply_edit(agent, external, |e| e.replace_model_setup(canvas(200.)))?;
    let token = editor.begin_timeline_edit(id, "drag")?;
    editor.update_timeline_edit(id, token, TimelineCommand::ScaleTime(2.))?;
    let result = host.write().commit_edit(agent, external);
    editor.sync_runtime()?;
    assert_eq!(result, Err(RuntimeError::Busy));
    assert_eq!(
        host.write().request_undo(agent, session),
        Err(RuntimeError::Busy)
    );
    assert!(editor.timeline_edit().is_some());
    editor.cancel_timeline_edit();
    host.write().commit_edit(agent, external)?;
    editor.sync_runtime()?;
    let external = host
        .write()
        .begin_edit(agent, session, "stale after gesture", None)?;
    let token = editor.begin_timeline_edit(id, "commit drag")?;
    editor.update_timeline_edit(id, token, TimelineCommand::ScaleTime(3.))?;
    editor.finish_timeline_edit(id, token)?;
    let stale = host.write().commit_edit(agent, external);
    assert!(matches!(stale, Err(RuntimeError::Sdk(_))), "{stale:?}");
    host.write().begin_interaction(agent, session)?;
    host.write().end_interaction(agent, session)?;
    Ok(())
}
#[test]
fn r2_external_fault_before_first_poll_is_safe() -> anyhow::Result<()> {
    let host = RuntimeHost::new();
    let sdk = AuthoringSession::with_filesystem(
        "65000000-0000-4000-8000-000000000001",
        canvas(100.),
        Arc::new(PanicFs),
    )?;
    let session = host.write().adopt_session(host.client_id(), sdk)?;
    let mut editor = Editor::with_runtime(host.clone());
    editor.attach_resident_sessions()?;
    let agent = host.write().register_client()?;
    let dir = tempfile::tempdir()?;
    let job = host.write().request_save(agent, session, dir.path())?;
    let work = host.write().take_work(job)?;
    host.write().complete(work.run());
    assert!(matches!(
        host.read().session_info(session)?.state,
        SessionState::Faulted(_)
    ));
    assert!(editor.sync_runtime().is_err());
    assert!(editor.document().is_none());
    assert!(!editor.sync_runtime()?);
    assert_eq!(editor.documents().len(), 1);
    editor.close_document(editor.active_id().unwrap(), true)?;
    Ok(())
}
#[test]
fn r2_quit_cannot_finish_with_detached_save_running() -> anyhow::Result<()> {
    let mut editor = Editor::new()?;
    let host = editor.runtime_host();
    let id = editor.active_id().unwrap();
    let session = editor.document().unwrap().runtime_session_id();
    let dir = tempfile::tempdir()?;
    let _work = editor.prepare_save(id, dir.path(), false)?;
    editor.detach_document(id)?;
    let exit = editor.request_exit();
    eprintln!(
        "request_exit={exit:?}, next_close={:?}, state={:?}",
        editor.next_close(),
        host.read().session_info(session)?.state
    );
    assert!(
        exit.is_err() || editor.next_close().is_some(),
        "Exit would call cx.quit while save is still running"
    );
    Ok(())
}

use zipzi_editor_core::Document;
use zipzi_runtime::RuntimeLimits;
use zipzi_sdk::{prepare_png_asset, rectangle_mesh};
fn budget_id(n: u32) -> String {
    format!("66000000-0000-4000-8000-{n:012x}")
}
#[test]
fn r2_typed_remesh_respects_runtime_history_budget() -> anyhow::Result<()> {
    let mut sdk = AuthoringSession::new(&budget_id(1), canvas(100.))?;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../zipzi-sdk/tests/fixtures/asymmetric-2x2.png");
    let asset = prepare_png_asset(&budget_id(2), "fixture", &path)?;
    let mesh = rectangle_mesh(
        &budget_id(3),
        "mesh",
        &budget_id(2),
        Vec2::default(),
        Vec2::new(10., 10.),
    )?;
    sdk.edit("setup", None, |edit| {
        edit.create_asset(asset)?;
        edit.create_mesh(mesh)
    })?;
    let limit = sdk.history_state().estimated_bytes + 1;
    let mut runtime = Runtime::new(RuntimeLimits {
        history_bytes: limit,
        ..Default::default()
    });
    let client = runtime.register_client()?;
    let session = runtime.adopt_session(client, sdk)?;
    let version = runtime.session(session)?.version();
    let result = runtime.remesh_rectangle_grid(client, session, &budget_id(3), 32, 32, None);
    let history = runtime.session(session)?.history_state().estimated_bytes;
    eprintln!("remesh result={result:?}; runtime history budget={limit}; actual history={history}");
    assert!(
        matches!(result, Err(RuntimeError::Budget(_))),
        "typed write bypassed Runtime budget"
    );
    assert_eq!(runtime.session(session)?.version(), version);
    assert_eq!(
        runtime.session(session)?.history_state().estimated_bytes,
        limit - 1
    );
    // Rejection must release its temporary candidate slot.
    let edit = runtime.begin_edit(client, session, "retry", None)?;
    runtime.release_edit(client, edit)?;
    Ok(())
}

#[test]
fn r2_sync_save_dispatch_failure_returns_ownership() -> anyhow::Result<()> {
    let host = RuntimeHost::new();
    let mut sessions = Vec::new();
    for n in 1..=3 {
        sessions.push(host.write().create_session(
            host.client_id(),
            &budget_id(n),
            canvas(100.),
        )?);
    }
    let mut document = Document::attach_runtime(host.clone(), sessions[2])?;
    let dir = tempfile::tempdir()?;
    let mut running = Vec::new();
    for (i, session) in sessions[..2].iter().enumerate() {
        let job = host.write().request_save(
            host.client_id(),
            *session,
            &dir.path().join(format!("running{i}")),
        )?;
        running.push(host.write().take_work(job)?);
    }
    let result = document.save(&dir.path().join("rejected"));
    let state = host.read().session_info(sessions[2])?.state;
    eprintln!("synchronous save result={result:?}; session state={state:?}");
    assert!(result.is_err());
    assert_eq!(
        state,
        SessionState::Ready,
        "failed dispatch left an unreachable queued save"
    );
    assert_eq!(host.read().list_jobs(host.client_id())?.len(), 2);
    for work in running {
        host.write().complete(work.run());
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
    let job = host.write().request_import(
        agent,
        zipzi_runtime::ImportSource::Psd(directory.path().join("missing.psd")),
    )?;
    assert_eq!(host.read().list_sessions().len(), 1); // provisional load is hidden
    assert!(editor.request_exit().is_err());
    assert!(!editor.exiting());
    host.write().cancel_job(agent, job)?;
    let work = host.write().take_work(job)?;
    host.write().complete(work.run());
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

    let edit = host
        .write()
        .begin_edit(agent, session, "Agent batch", None)?;
    host.write().apply_edit(agent, edit, |e| {
        e.rename_mesh(&mesh, "Agent draft")?;
        let mut clip = e
            .candidate_document()
            .get_motion(&motion.clip_id)
            .unwrap()
            .clone();
        Arc::make_mut(&mut clip.tracks[0].segments)[0]
            .end_mut()
            .value = 0.8;
        e.replace_motion(clip)
    })?;
    let job = host.write().request_commit(agent, edit)?;
    let completion = host.write().take_work(job)?.run();
    assert!(host.write().complete(completion));
    host.write().release_job(agent, job)?;
    assert!(editor.sync_runtime()?);
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    assert_eq!(editor.document().unwrap().layers()[0].name, "Agent draft");

    let obsolete = host
        .write()
        .begin_edit(agent, session, "Old candidate", None)?;
    host.write().apply_edit(agent, obsolete, |e| {
        e.rename_mesh(&mesh, "Must not overwrite")
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
    let error = host.write().commit_edit(agent, obsolete).unwrap_err();
    assert!(
        matches!(error, RuntimeError::Sdk(ref error) if error.code.as_ref() == "STALE_VERSION")
    );
    host.write().release_edit(agent, obsolete)?;
    assert_eq!(editor.document().unwrap().layers()[0].name, "Human polish");
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(editor.document().unwrap().history_lengths(), (2, 1));
    editor.execute(EditorCommand::Redo)?;
    let refined = host.read().session_info(session)?.version;

    let next = host
        .write()
        .begin_edit(agent, session, "Continue human work", Some(refined))?;
    host.write().apply_edit(agent, next, |e| {
        assert_eq!(
            e.candidate_document().get_mesh(&mesh).unwrap().name,
            "Human polish"
        );
        let mut clip = e
            .candidate_document()
            .get_motion(&motion.clip_id)
            .unwrap()
            .clone();
        assert_eq!(clip.tracks[0].segments[0].end().id, endpoint_key);
        assert_eq!(clip.tracks[0].segments[0].end().value, 0.45);
        e.rename_mesh(&mesh, "Agent continued")?;
        clip.tracks[0].initial.value = 0.2;
        e.replace_motion(clip)
    })?;
    host.write().commit_edit(agent, next)?;
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
