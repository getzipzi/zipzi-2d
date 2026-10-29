//! Window-independent host completion and native attach/detach regressions.
use super::{Harness, setup};
use crate::host::DesktopHost;
use crate::host::external_status::ExternalStatus;
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};

#[gpui_kit::test]
fn external_status_connections_and_disconnects_stay_quiet(cx: &mut TestAppContext) {
    let (harness, host) = hosted(cx);
    let (client, connection) = connect_peer(&host);
    cx.run_until_parked();
    assert!(
        host.external_status
            .read_with(cx, |s, _| s.label())
            .is_empty()
    );
    host.protocol
        .borrow_mut()
        .disconnect(client, connection, std::time::Instant::now());
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("app-external-status").is_none());
    })
    .unwrap();
    assert!(
        host.external_status
            .read_with(cx, |s, _| s.label())
            .is_empty()
    );
}

fn external_status_rename(
    host: &DesktopHost,
    session: zipzi_runtime::SessionId,
    client: zipzi_runtime::ClientId,
    mesh: &str,
    name: &str,
) {
    let edit = host
        .runtime
        .write()
        .begin_edit(client, session, "Rename", None)
        .unwrap();
    host.runtime
        .write()
        .apply_edit(client, edit, |e| e.rename_mesh(mesh, name))
        .unwrap();
    let job = host.runtime.write().request_commit(client, edit).unwrap();
    let work = host.runtime.write().take_work(job).unwrap();
    assert!(host.runtime.write().complete(work.run()));
    host.runtime.write().release_job(client, job).unwrap();
}

#[gpui_kit::test]
fn external_status_only_reports_changed_foreign_publications_and_coalesces_them(
    cx: &mut TestAppContext,
) {
    let (harness, host) = hosted(cx);
    let status = host.external_status.clone();
    let (client, connection) = connect_peer(&host);
    let (session, mesh) = harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        (doc.runtime_session_id(), doc.layers()[0].id.clone())
    });
    // Human edits, successful no-ops and cancelled candidates stay quiet.
    external_status_rename(
        &host,
        session,
        host.runtime.client_id(),
        &mesh,
        "Human edit",
    );
    external_status_rename(&host, session, client, &mesh, "Human edit");
    let edit = host
        .runtime
        .write()
        .begin_edit(client, session, "Cancelled", None)
        .unwrap();
    host.runtime
        .write()
        .apply_edit(client, edit, |e| e.rename_mesh(&mesh, "Discard me"))
        .unwrap();
    host.runtime.write().release_edit(client, edit).unwrap();
    cx.run_until_parked();
    assert!(status.read_with(cx, |s, _| s.label()).is_empty());
    external_status_rename(&host, session, client, &mesh, "Script edit");
    cx.run_until_parked();
    assert!(!status.read_with(cx, |s, _| s.label()).is_empty());
    cx.background_executor
        .advance_clock(std::time::Duration::from_secs(2));
    external_status_rename(&host, session, client, &mesh, "Another script edit");
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    assert!(!status.read_with(cx, |s, _| s.label()).is_empty());
    cx.background_executor
        .advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    assert!(status.read_with(cx, |s, _| s.label()).is_empty());
    // A short script's disconnect must not immediately erase its successful result.
    external_status_rename(&host, session, client, &mesh, "Final script edit");
    cx.run_until_parked();
    host.protocol
        .borrow_mut()
        .disconnect(client, connection, std::time::Instant::now());
    assert!(!status.read_with(cx, |s, _| s.label()).is_empty());
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(ExternalStatus::NOTICE_DURATION);
    cx.run_until_parked();
    assert!(status.read_with(cx, |s, _| s.label()).is_empty());
}

#[gpui_kit::test]
fn external_status_keeps_operation_hints_centered_and_error_timeout_independent(
    cx: &mut TestAppContext,
) {
    use gpui_kit::{px, size};
    let (harness, host) = hosted(cx);
    let (client, _) = connect_peer(&host);
    let (session, mesh) = harness.view.read_with(cx, |view, _| {
        let doc = view.editor.document().unwrap();
        (doc.runtime_session_id(), doc.layers()[0].id.clone())
    });
    cx.run_until_parked();
    external_status_rename(&host, session, client, &mesh, "Initial background update");
    cx.run_until_parked();
    for width in [800., 1100.] {
        cx.simulate_window_resize(harness.window, size(px(width), px(740.)));
        cx.update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.set_edit_error("Keep this operation error visible", cx)
            });
            window.render_frame(cx);
            let bar = window.find("app-status-bar").bounds();
            let hint = window.find("app-status-message").bounds();
            let external = window.find("app-external-status").bounds();
            assert!((hint.center().x - bar.center().x).abs() <= px(0.5));
            assert!(hint.right() <= external.left());
            assert!(external.right() <= bar.right());
            assert!(window.try_find("app-external-connected-dot").is_none());
        })
        .unwrap();
    }
    external_status_rename(&host, session, client, &mesh, "Background update");
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let bar = window.find("app-status-bar").bounds();
        let hint = window.find("app-status-message").bounds();
        let external = window.find("app-external-status").bounds();
        assert!((hint.center().x - bar.center().x).abs() <= px(0.5));
        assert!(hint.right() <= external.left());
        assert!(window.try_find("app-external-connected-dot").is_none());
        let label = window.find("app-external-status-label").bounds();
        assert!(bar.right() - label.right() >= crate::ui::compact::scaled(12., cx));
        let view = harness.view.read(cx);
        assert_eq!(
            view.status_bar.read(cx).displayed_message(),
            Some(("Keep this operation error visible", true))
        );
    })
    .unwrap();
    cx.background_executor
        .advance_clock(ExternalStatus::NOTICE_DURATION);
    cx.run_until_parked();
    harness.view.read_with(cx, |view, cx| {
        assert_eq!(
            view.status_bar.read(cx).displayed_message(),
            Some(("Keep this operation error visible", true))
        );
    });
}

#[gpui_kit::test]
fn external_status_identifies_background_project_and_tracks_project_switches(
    cx: &mut TestAppContext,
) {
    let (harness, host) = hosted(cx);
    let (client, _) = connect_peer(&host);
    let (original, session, mesh) = harness.view.read_with(cx, |view, _| {
        (
            view.editor.active_id().unwrap(),
            view.editor.document().unwrap().runtime_session_id(),
            view.editor.layers()[0].id.clone(),
        )
    });
    let project_path = harness._config_dir.path().join("Background-rig");
    let job = host
        .runtime
        .write()
        .request_save(client, session, &project_path)
        .unwrap();
    let work = host.runtime.write().take_work(job).unwrap();
    assert!(host.runtime.write().complete(work.run()));
    host.runtime.write().release_job(client, job).unwrap();
    let current = harness.view.update(cx, |view, cx| {
        let id = view.editor.new_document().unwrap();
        view.redraw(cx);
        id
    });
    cx.run_until_parked();
    external_status_rename(&host, session, client, &mesh, "Changed in background");
    cx.run_until_parked();
    assert_eq!(
        host.external_status.read_with(cx, |s, _| s.label()),
        "Background-rig · External changes applied"
    );
    assert_eq!(
        harness
            .view
            .read_with(cx, |view, _| view.editor.active_id()),
        Some(current)
    );
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.activate_document(original, window, cx));
        window.render_frame(cx);
        assert_eq!(
            host.external_status.read(cx).label(),
            "External changes applied"
        );
        harness
            .view
            .update(cx, |view, cx| view.activate_document(current, window, cx));
        window.render_frame(cx);
        assert_eq!(
            host.external_status.read(cx).label(),
            "Background-rig · External changes applied"
        );
    })
    .unwrap();
}

fn hosted(cx: &mut TestAppContext) -> (Harness, DesktopHost) {
    hosted_with_residency(cx, false)
}

fn hosted_with_residency(cx: &mut TestAppContext, resident: bool) -> (Harness, DesktopHost) {
    let harness = setup(cx);
    let host = cx.update(|cx| DesktopHost::new(resident, cx));
    drop(host.take_editor());
    harness.view.update(cx, |view, cx| {
        view.editor.rehost(host.runtime.clone()).unwrap();
        view.host = Some(host.clone());
        view.connect_runtime_events(cx);
    });
    cx.update(|cx| cx.set_global(host.clone()));
    (harness, host)
}

fn connect_peer(host: &DesktopHost) -> (zipzi_runtime::ClientId, u64) {
    let hello = host.protocol.borrow_mut().hello(
        &mut host.runtime.write(),
        serde_json::json!({"protocol": zipzi_runtime::protocol::CONTRACT}),
        std::time::Instant::now(),
    );
    (
        serde_json::from_value(hello["ok"]["client"].clone()).unwrap(),
        hello["ok"]["connection"].as_u64().unwrap(),
    )
}

fn modify(harness: &Harness, cx: &mut TestAppContext) {
    harness.view.update(cx, |view, cx| {
        view.editor
            .execute(zipzi_editor_core::EditorCommand::CreateParameter(
                zipzi_editor_core::NewParameter {
                    name: "Unsaved".into(),
                    ..Default::default()
                },
            ))
            .unwrap();
        view.redraw(cx);
        assert!(view.editor.document().unwrap().modified());
    });
}

#[gpui_kit::test]
fn rendering_external_busy_job_before_notification_freezes_and_restores_the_view(
    cx: &mut TestAppContext,
) {
    use zipzi_editor_core::{CommandDisabledReason, CommandId};

    let (harness, host) = hosted(cx);
    cx.run_until_parked();
    let agent = host.runtime.write().register_client().unwrap();
    let target = harness._config_dir.path().join("external-save");
    cx.update_window(harness.window, |_, window, cx| {
        let (job, menu_index) = harness.view.update(cx, |view, cx| {
            let session = view.editor.document().unwrap().runtime_session_id();
            let command = view.editor.command(CommandId::CreateParameter);
            let menu_index = zipzi_editor_core::COMMANDS
                .iter()
                .position(|spec| spec.id == CommandId::CreateParameter)
                .unwrap();
            assert!(view.editor.command_state(command).enabled);
            let job = host
                .runtime
                .write()
                .request_save(agent, session, &target)
                .unwrap();
            cx.notify();
            (job, menu_index)
        });
        // Draw in this same App update, before the notification task can run,
        // and hold the queued Job until all Busy assertions complete.
        window.render_frame(cx);
        harness.view.read_with(cx, |view, _| {
            assert!(view.editor.is_busy());
            assert!(view.editor.document().is_none());
            let state = &view.menu_state.commands[menu_index];
            assert!(!state.enabled);
            assert_eq!(state.disabled_reason, Some(CommandDisabledReason::Busy));
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        });
        let work = host.runtime.write().take_work(job).unwrap();
        host.runtime.write().complete(work.run());
        host.runtime.write().release_job(agent, job).unwrap();
        // Ready also has to be visible without waiting for that task.
        harness.view.update(cx, |_, cx| cx.notify());
        window.render_frame(cx);
        harness.view.read_with(cx, |view, _| {
            assert!(!view.editor.is_busy());
            assert!(view.editor.document().is_some());
            assert!(view.menu_state.commands[menu_index].enabled);
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        });
    })
    .unwrap();
    assert!(target.join("project.zipzi.json").is_file());
}

#[gpui_kit::test]
fn last_idle_window_enters_quit_instead_of_parking_sessions(cx: &mut TestAppContext) {
    let (harness, host) = hosted(cx);
    // Exercise the native red-button callback. Quit owns the window lifetime;
    // the test platform's quit itself is a no-op.
    let mut visual = gpui_kit::VisualTestContext::from_window(harness.window, cx);
    assert!(!visual.simulate_close());
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.exiting());
        assert_eq!(view.editor.document_count(), 0);
        assert!(!view.modal);
    });
    assert!(host.runtime.read().list_sessions().is_empty());
}

#[gpui_kit::test]
fn closing_dirty_last_window_supports_cancel_and_discard(cx: &mut TestAppContext) {
    let (harness, host) = hosted(cx);
    modify(&harness, cx);
    for discard in [false, true] {
        cx.update_window(harness.window, |_, window, cx| {
            harness.view.update(cx, |view, cx| {
                view.close_window(window, cx);
                assert!(view.modal);
                assert!(view.editor.exiting());
            });
        })
        .unwrap();
        cx.simulate_prompt_answer(if discard {
            "Discard and Close"
        } else {
            "Cancel"
        });
        cx.run_until_parked();
        harness.view.read_with(cx, |view, _| {
            assert!(!view.modal);
            assert_eq!(view.editor.exiting(), discard);
            assert_eq!(view.editor.document_count(), usize::from(!discard));
            if !discard {
                assert!(view.editor.document().unwrap().modified());
            }
            assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        });
    }
    assert!(host.runtime.read().list_sessions().is_empty());
}

#[gpui_kit::test]
fn closing_last_window_saves_before_quitting(cx: &mut TestAppContext) {
    let (harness, host) = hosted(cx);
    harness
        .view
        .update(cx, |view, _| view.editor.new_document().unwrap());
    modify(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.close_window(window, cx));
    })
    .unwrap();
    cx.simulate_prompt_answer("Save");
    cx.run_until_parked();
    assert!(cx.did_prompt_for_new_path());
    assert_eq!(host.runtime.read().list_sessions().len(), 1);
    let target = harness._config_dir.path().join("saved-on-close.zipzi");
    cx.simulate_new_path_selection(|_| Some(target.clone()));
    cx.run_until_parked();
    assert!(target.join("project.zipzi.json").is_file());
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.exiting());
        assert_eq!(view.editor.document_count(), 0);
        assert!(!view.modal);
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
    });
    assert!(host.runtime.read().list_sessions().is_empty());
}

#[gpui_kit::test]
fn cancelling_close_save_picker_or_failing_save_preserves_unsaved_document(
    cx: &mut TestAppContext,
) {
    let (harness, _) = hosted(cx);
    harness
        .view
        .update(cx, |view, _| view.editor.new_document().unwrap());
    modify(&harness, cx);
    for fail in [false, true] {
        cx.update_window(harness.window, |_, window, cx| {
            harness
                .view
                .update(cx, |view, cx| view.close_window(window, cx));
        })
        .unwrap();
        cx.simulate_prompt_answer("Save");
        cx.run_until_parked();
        let target = harness._config_dir.path().join("invalid-save.zipzi");
        if fail {
            std::fs::create_dir_all(target.join("project.zipzi.json")).unwrap();
        }
        cx.simulate_new_path_selection(|_| fail.then_some(target));
        cx.run_until_parked();
        harness.view.read_with(cx, |view, _| {
            assert!(!view.editor.exiting());
            assert!(!view.modal);
            assert!(view.editor.document().unwrap().modified());
            assert_eq!(view.edit_error.is_some(), fail);
        });
    }
}

#[gpui_kit::test]
fn connected_sdk_keeps_host_and_unsaved_content_alive_without_attaching_a_session(
    cx: &mut TestAppContext,
) {
    let (harness, host) = hosted(cx);
    modify(&harness, cx);
    let _peer = connect_peer(&host);
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.close_window(window, cx));
    })
    .unwrap();
    assert!(cx.update(|cx| cx.windows().is_empty()));
    harness.view.read_with(cx, |view, _| {
        assert!(!view.editor.exiting());
        assert!(!view.modal);
        assert!(view.editor.document().unwrap().modified());
    });
    assert_eq!(host.runtime.read().list_sessions().len(), 1);
}

#[gpui_kit::test]
fn disconnected_sdk_does_not_delay_close_for_reconnect_grace(cx: &mut TestAppContext) {
    let (harness, host) = hosted(cx);
    let session = harness.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().runtime_session_id()
    });
    let (peer, connection) = connect_peer(&host);
    host.runtime.write().attach_session(peer, session).unwrap();
    host.protocol
        .borrow_mut()
        .disconnect(peer, connection, std::time::Instant::now());
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.close_window(window, cx));
    })
    .unwrap();
    harness.view.read_with(cx, |view, _| {
        assert!(view.editor.exiting());
        assert_eq!(view.editor.document_count(), 0);
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
    });
    assert!(host.runtime.read().list_sessions().is_empty());
}

#[gpui_kit::test]
fn explicit_no_window_host_stays_resident_after_its_window_is_closed(cx: &mut TestAppContext) {
    let (harness, host) = hosted_with_residency(cx, true);
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.close_window(window, cx));
    })
    .unwrap();
    assert!(cx.update(|cx| cx.windows().is_empty()));
    harness
        .view
        .read_with(cx, |view, _| assert!(!view.editor.exiting()));
    assert_eq!(host.runtime.read().list_sessions().len(), 1);
}

#[gpui_kit::test]
fn non_last_window_closes_without_starting_quit(cx: &mut TestAppContext) {
    let (harness, host) = hosted(cx);
    let _other = cx.add_window(|_, _| gpui_kit::EmptyView);
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.close_window(window, cx));
    })
    .unwrap();
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
    harness
        .view
        .read_with(cx, |view, _| assert!(!view.editor.exiting()));
    assert_eq!(host.runtime.read().list_sessions().len(), 1);
}

#[gpui_kit::test]
fn foreign_background_job_keeps_host_alive_without_sdk_connection(cx: &mut TestAppContext) {
    let (harness, host) = hosted(cx);
    // Host jobs include work on sessions not attached to the visible workspace.
    let client = host.runtime.write().register_client().unwrap();
    let session = host
        .runtime
        .write()
        .create_session(
            client,
            &uuid::Uuid::new_v4().to_string(),
            zipzi_core::ModelSetup::default(),
        )
        .unwrap();
    let target = harness._config_dir.path().join("background.zipzi");
    let job = host
        .runtime
        .write()
        .request_save(client, session, &target)
        .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            assert_eq!(view.editor.pending_io_count(), 0);
            view.close_window(window, cx);
        });
    })
    .unwrap();
    assert!(cx.update(|cx| cx.windows().is_empty()));
    harness
        .view
        .read_with(cx, |view, _| assert!(!view.editor.exiting()));
    let work = host.runtime.write().take_work(job).unwrap();
    host.runtime.write().complete(work.run());
    assert!(target.join("project.zipzi.json").is_file());
}

#[gpui_kit::test]
fn sdk_connecting_during_save_confirmation_cancels_automatic_exit(cx: &mut TestAppContext) {
    let (harness, host) = hosted(cx);
    modify(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.close_window(window, cx));
    })
    .unwrap();
    let _peer = connect_peer(&host);
    cx.simulate_prompt_answer("Discard and Close");
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert!(!view.editor.exiting());
        assert!(!view.modal);
        assert!(view.editor.document().unwrap().modified());
    });
    assert_eq!(cx.update(|cx| cx.windows().len()), 1);
}

#[gpui_kit::test]
fn closing_last_window_also_prompts_for_a_detached_dirty_tab(cx: &mut TestAppContext) {
    let (harness, host) = hosted(cx);
    modify(&harness, cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.close_active(window, cx);
            assert_eq!(view.editor.document_count(), 0);
            view.close_window(window, cx);
            assert!(view.modal);
            assert_eq!(view.editor.document_count(), 1);
        });
    })
    .unwrap();
    cx.simulate_prompt_answer("Cancel");
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert!(!view.editor.exiting());
        assert!(view.editor.document().unwrap().modified());
    });
    assert_eq!(host.runtime.read().list_sessions().len(), 1);
}

fn save_and_close(cx: &mut TestAppContext, fail: bool, reopen_before_completion: bool) {
    let (harness, host) = hosted(cx);
    let id = harness
        .view
        .read_with(cx, |view, _| view.editor.active_id().unwrap());
    let session = harness.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().runtime_session_id()
    });
    let mesh = harness.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().layers()[0].id.clone()
    });
    let edit = host
        .runtime
        .write()
        .begin_edit(host.runtime.client_id(), session, "R2 unsaved edit", None)
        .unwrap();
    host.runtime
        .write()
        .apply_edit(host.runtime.client_id(), edit, |edit| {
            edit.rename_mesh(&mesh, "R2 changed mesh")
        })
        .unwrap();
    host.runtime
        .write()
        .commit_edit(host.runtime.client_id(), edit)
        .unwrap();
    harness.view.update(cx, |view, _| {
        view.editor.sync_runtime().unwrap();
    });
    let version = host.runtime.read().session(session).unwrap().version();
    let history = host
        .runtime
        .read()
        .session(session)
        .unwrap()
        .history_lengths();
    let old_path = harness.view.read_with(cx, |view, _| {
        view.editor
            .document()
            .unwrap()
            .path()
            .map(std::path::PathBuf::from)
    });
    let target = harness._config_dir.path().join("host-save");
    if fail {
        std::fs::create_dir_all(target.join("project.zipzi.json")).unwrap();
    }
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.save_to(id, target.clone(), false, window, cx);
            assert!(view.editor.is_busy());
            view.close_window(window, cx);
        });
    })
    .unwrap();
    let Harness {
        view,
        _assets,
        _config_dir,
        ..
    } = harness;
    drop(view); // Release the last strong view; task keeps only host + weak view.
    assert!(cx.update(|cx| cx.windows().is_empty()));
    let reopened = if reopen_before_completion {
        Some(cx.update(|cx| crate::host::open_window(cx, false).unwrap()))
    } else {
        None
    };
    cx.run_until_parked();
    let info = host.runtime.read().session_info(session).unwrap();
    assert_eq!(info.state, zipzi_editor_core::RuntimeSessionState::Ready);
    assert_eq!(info.version.session_id, version.session_id);
    assert_eq!(host.outcomes().len(), 1);
    assert_eq!(host.outcomes()[0].result.as_ref().unwrap().is_err(), fail);
    let (window, view) = reopened.unwrap_or_else(|| {
        assert!(
            cx.update(|cx| cx.windows().is_empty()),
            "completion reopened a window"
        );
        cx.update(|cx| crate::host::open_window(cx, false).unwrap())
    });
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        view.read_with(cx, |view, _| {
            assert!(!view.editor.is_busy());
            let document = view.editor.document().unwrap();
            assert_eq!(document.runtime_session_id(), session);
            assert_eq!(document.history_lengths(), history);
            assert_eq!(document.modified(), fail);
            if fail {
                assert_eq!(document.path(), old_path.as_deref());
                assert!(view.edit_error.is_some());
            } else {
                assert_eq!(
                    document.path(),
                    target
                        .join("project.zipzi.json")
                        .canonicalize()
                        .ok()
                        .as_deref()
                );
            }
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn save_succeeds_with_no_window_and_reopens_same_runtime_session(cx: &mut TestAppContext) {
    save_and_close(cx, false, false);
}

#[gpui_kit::test]
fn save_failure_with_no_window_returns_session_and_reopens_with_error(cx: &mut TestAppContext) {
    save_and_close(cx, true, false);
}

#[gpui_kit::test]
fn window_reopened_before_old_views_save_completion_does_not_remain_busy(cx: &mut TestAppContext) {
    save_and_close(cx, false, true);
}

#[gpui_kit::test]
fn host_accepts_typed_creation_without_any_window_then_attaches_editor(cx: &mut TestAppContext) {
    let harness = setup(cx); // initializes kit/settings; close this unrelated fixture
    cx.update_window(harness.window, |_, window, _| window.remove_window())
        .unwrap();
    drop(harness);
    let host = cx.update(|cx| DesktopHost::new(false, cx));
    let session = host
        .runtime
        .write()
        .create_session(
            host.runtime.client_id(),
            "63000000-0000-4000-8000-000000000001",
            zipzi_core::ModelSetup::default(),
        )
        .unwrap();
    assert!(cx.update(|cx| cx.windows().is_empty()));
    cx.update(|cx| cx.set_global(host.clone()));
    let (window, view) = cx.update(|cx| crate::host::open_window(cx, false).unwrap());
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        assert_eq!(
            view.read(cx)
                .editor
                .document()
                .unwrap()
                .runtime_session_id(),
            session
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn quit_reports_an_attached_peer_without_reentering_the_close_loop(cx: &mut TestAppContext) {
    let (harness, host) = hosted(cx);
    let session = harness.view.read_with(cx, |view, _| {
        assert!(!view.editor.document().unwrap().modified());
        view.editor.document().unwrap().runtime_session_id()
    });
    let peer = host.runtime.write().register_client().unwrap();
    host.runtime.write().attach_session(peer, session).unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.close(window, cx);
            assert!(!view.editor.exiting());
            assert!(!view.modal);
            assert!(view.edit_error.is_some());
            assert_eq!(view.editor.document_count(), 1);
        });
    })
    .unwrap();
    assert!(host.runtime.read().session(session).is_ok());
}

use zipzi_editor_core::{ObjectKind, ObjectRef};
#[gpui_kit::test]
fn r2_reopen_catches_up_external_edits(cx: &mut TestAppContext) {
    let (harness, host) = hosted(cx);
    let (session, mesh) = harness.view.read_with(cx, |view, _| {
        let document = view.editor.document().unwrap();
        let mesh = document.layers()[0].id.clone();
        let _cached = document.object_index();
        (document.runtime_session_id(), mesh)
    });
    let (client, _) = connect_peer(&host);
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.close_window(window, cx));
    })
    .unwrap();
    let Harness {
        view,
        _assets,
        _config_dir,
        ..
    } = harness;
    drop(view);
    cx.run_until_parked();
    assert!(cx.update(|cx| cx.windows().is_empty()));
    let edit = host
        .runtime
        .write()
        .begin_edit(client, session, "while hidden", None)
        .unwrap();
    host.runtime
        .write()
        .apply_edit(client, edit, |e| {
            e.rename_mesh(&mesh, "UPDATED WHILE HIDDEN")
        })
        .unwrap();
    host.runtime.write().commit_edit(client, edit).unwrap();
    assert_eq!(
        host.runtime
            .read()
            .session(session)
            .unwrap()
            .mesh(&mesh)
            .unwrap()
            .name,
        "UPDATED WHILE HIDDEN"
    );
    let (window, view) = cx.update(|cx| crate::host::open_window(cx, false).unwrap());
    cx.run_until_parked();
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        view.read_with(cx, |view, _| {
            let document = view.editor.document().unwrap();
            assert_eq!(document.runtime_session_id(), session);
            assert_eq!(
                document
                    .object_index()
                    .name(&ObjectRef::new(ObjectKind::Mesh, &mesh)),
                "UPDATED WHILE HIDDEN"
            );
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn r2_detached_tab_save_failure_is_visible(cx: &mut TestAppContext) {
    let (harness, host) = hosted(cx);
    let id = harness
        .view
        .read_with(cx, |view, _| view.editor.active_id().unwrap());
    let target = harness._config_dir.path().join("failed-save");
    std::fs::create_dir_all(target.join("project.zipzi.json")).unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.save_to(id, target.clone(), false, window, cx);
            assert!(view.editor.is_busy());
            view.close_active(window, cx);
            assert_eq!(view.editor.document_count(), 0);
        });
    })
    .unwrap();
    cx.run_until_parked();
    assert!(
        host.outcomes()
            .iter()
            .any(|job| job.result.as_ref().unwrap().is_err())
    );
    harness.view.read_with(cx, |view, _| {
        assert!(
            view.edit_error.is_some(),
            "failed save was consumed without showing an error"
        );
    });
}
