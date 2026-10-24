//! Window-independent host completion and native attach/detach regressions.
use super::{Harness, setup};
use crate::host::DesktopHost;
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};

fn hosted(cx: &mut TestAppContext) -> (Harness, DesktopHost) {
    let harness = setup(cx);
    let host = DesktopHost::new();
    drop(host.take_editor());
    harness.view.update(cx, |view, cx| {
        view.editor.rehost(host.runtime.clone()).unwrap();
        view.host = Some(host.clone());
        view.connect_runtime_events(cx);
    });
    cx.update(|cx| cx.set_global(host.clone()));
    (harness, host)
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
    let host = DesktopHost::new();
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
    let client = host.runtime.write().register_client().unwrap();
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
