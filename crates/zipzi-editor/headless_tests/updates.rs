use super::setup;
use crate::updates::{self, Phase};
use gpui_kit::{AppContext, TestAppContext, px, test::TestWindowExt};

#[gpui_kit::test]
fn update_indicator_is_quiet_until_ready_and_keeps_workspace_centered(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let updater = cx.update(updates::init);
    let center = cx
        .update_window(harness.window, |_, window, _| {
            window.find("workspace-switcher").bounds().center().x
        })
        .unwrap();
    for phase in [
        Phase::Idle,
        Phase::Checking,
        Phase::Downloading(42),
        Phase::Failed,
        Phase::Ready,
    ] {
        let visible = phase == Phase::Ready;
        cx.update(|cx| {
            updater.update(cx, |updater, cx| {
                updater.phase = phase;
                cx.notify();
            });
            cx.refresh_windows();
        });
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            assert_eq!(window.try_find("workspace-update-ready").is_some(), visible);
            let switcher = window.find("workspace-switcher").bounds();
            assert!((center - switcher.center().x).abs() <= px(0.5));
        })
        .unwrap();
    }
    cx.update_window(harness.window, |_, window, cx| {
        window.click("workspace-update-ready", cx)
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("updates-panel").is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn restart_update_refuses_unsaved_projects_without_closing_them(cx: &mut TestAppContext) {
    let harness = setup(cx);
    harness.view.update(cx, |view, _| {
        view.editor
            .execute(zipzi_editor_core::EditorCommand::CreateParameter(
                zipzi_editor_core::NewParameter {
                    name: "Unsaved".into(),
                    ..Default::default()
                },
            ))
            .unwrap();
        assert!(view.editor.document().unwrap().modified());
    });
    let before = harness
        .view
        .read_with(cx, |view, _| view.editor.document_count());
    let error = cx.update(updates::prepare_restart).unwrap_err();
    assert!(error.contains("Save or close"), "{error}");
    assert_eq!(
        harness
            .view
            .read_with(cx, |view, _| view.editor.document_count()),
        before
    );
}

#[gpui_kit::test]
fn checking_updates_from_menu_opens_the_shared_panel(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.dispatch_action(Box::new(crate::actions::CheckUpdates), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("updates-panel").is_some());
        assert!(window.try_find("workspace-update-ready").is_none());
        window.click("updates-primary", cx);
    })
    .unwrap();
    assert_eq!(cx.opened_url().as_deref(), Some(updates::DOWNLOADS));
}

fn hosted(cx: &mut TestAppContext) -> (super::Harness, crate::host::DesktopHost) {
    let harness = setup(cx);
    let host = cx.update(|cx| crate::host::DesktopHost::new(false, cx));
    drop(host.take_editor());
    harness.view.update(cx, |view, cx| {
        view.editor.rehost(host.runtime.clone()).unwrap();
        view.host = Some(host.clone());
        view.connect_runtime_events(cx);
    });
    cx.update(|cx| cx.set_global(host.clone()));
    (harness, host)
}

#[gpui_kit::test]
fn restart_update_refuses_connected_clients(cx: &mut TestAppContext) {
    let (_harness, host) = hosted(cx);
    host.protocol.borrow_mut().hello(
        &mut host.runtime.write(),
        serde_json::json!({"protocol": zipzi_runtime::protocol::CONTRACT}),
        std::time::Instant::now(),
    );
    let error = cx.update(updates::prepare_restart).unwrap_err();
    assert!(error.contains("Python clients"), "{error}");
}

#[gpui_kit::test]
fn restart_update_refuses_a_pending_runtime_job(cx: &mut TestAppContext) {
    let (harness, host) = hosted(cx);
    let session = harness.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().runtime_session_id()
    });
    let client = host.runtime.client_id();
    let edit = host
        .runtime
        .write()
        .begin_edit(client, session, "Pending edit", None)
        .unwrap();
    host.runtime.write().request_commit(client, edit).unwrap();
    let error = cx.update(updates::prepare_restart).unwrap_err();
    assert!(error.contains("Jobs are still running"), "{error}");
}

#[gpui_kit::test]
fn restart_update_refuses_unsaved_sessions_without_a_window(cx: &mut TestAppContext) {
    let (harness, host) = hosted(cx);
    harness.view.update(cx, |view, _| {
        view.editor
            .execute(zipzi_editor_core::EditorCommand::CreateParameter(
                zipzi_editor_core::NewParameter {
                    name: "Hidden unsaved".into(),
                    ..Default::default()
                },
            ))
            .unwrap();
    });
    // Detaching the window parks its editor and leaves the dirty session resident.
    cx.update_window(harness.window, |_, window, _| window.remove_window())
        .unwrap();
    drop(harness);
    assert!(
        host.runtime
            .read()
            .list_sessions()
            .iter()
            .any(|session| session.modified)
    );
    let error = cx.update(updates::prepare_restart).unwrap_err();
    assert!(error.contains("Save or close"), "{error}");
}
