use super::{Harness, setup};
use gpui_kit::{AppContext as _, TestAppContext, component::WindowExt, test::TestWindowExt};
use zipzi_editor_core::{CommandId, Editor};

fn empty_start(cx: &mut TestAppContext) -> Harness {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.replace_editor(Editor::empty(), cx);
            view.welcome_pending = true;
            view.sync_welcome(window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    harness
}

#[gpui_kit::test]
fn welcome_new_then_last_document_close_keeps_panel_dismissed(cx: &mut TestAppContext) {
    let harness = empty_start(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("welcome-panel").visible());
        window.click("welcome-new", cx);
        assert!(!window.has_active_dialog(cx));
        assert_eq!(harness.view.read(cx).editor.document_count(), 1);
        harness.view.update(cx, |view, cx| {
            // Observe the open document before closing it.
            view.sync_welcome(window, cx);
            view.run_command(CommandId::Close, window, cx);
            view.sync_welcome(window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
        assert!(window.try_find("welcome-panel").is_none());
        assert!(harness.view.read(cx).welcome.is_none());
        assert_eq!(harness.view.read(cx).editor.document_count(), 0);
    })
    .unwrap();
}

#[gpui_kit::test]
fn welcome_open_picker_cancel_and_manual_reopen(cx: &mut TestAppContext) {
    let harness = empty_start(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("welcome-open", cx);
        assert!(!window.has_active_dialog(cx));
    })
    .unwrap();
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            assert!(!view.modal);
            assert!(view.welcome.is_none());
            view.show_welcome(window, cx);
        });
        window.render_frame(cx);
        window.dispatch_action(Box::new(gpui_kit::component::dialog::Cancel), cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        assert!(!window.has_active_dialog(cx));
        assert!(harness.view.read(cx).welcome.is_none());
        harness
            .view
            .update(cx, |view, cx| view.sync_welcome(window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    assert!(harness.view.read_with(cx, |view, _| view.welcome.is_none()));
}

#[gpui_kit::test]
fn welcome_external_open_dismisses_without_blocking_queue(cx: &mut TestAppContext) {
    let harness = empty_start(cx);
    let path = harness._config_dir.path().join("welcome.zipzi");
    let mut session = zipzi_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        zipzi_core::Canvas::new(42., 42., Default::default(), 1.),
    )
    .unwrap();
    session.save_project(&path, None).unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.enqueue_projects(vec![path.clone()], window, cx);
            assert!(view.welcome.is_none());
        });
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document_count(), 1);
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
    });
}

#[gpui_kit::test]
fn welcome_existing_project_skips_start_panel(cx: &mut TestAppContext) {
    let harness = setup(cx);
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document_count(), 1);
        assert!(view.welcome.is_none());
        assert!(!view.welcome_pending);
    });
}

#[gpui_kit::test]
fn welcome_manual_panel_stays_over_project_and_opens_github(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.show_welcome(window, cx));
        window.render_frame(cx);
        assert!(window.find("welcome-panel").visible());
        window.click("welcome-github", cx);
        assert!(harness.view.read(cx).welcome.is_some());
        assert_eq!(harness.view.read(cx).editor.document_count(), 1);
    })
    .unwrap();
    assert_eq!(
        cx.opened_url().as_deref(),
        Some(crate::ui::about::REPOSITORY)
    );
}

#[gpui_kit::test]
fn welcome_routes_native_new_from_panel_focus(cx: &mut TestAppContext) {
    let harness = empty_start(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.dispatch_action(
            Box::new(crate::actions::RunCommand::new(CommandId::New)),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert!(view.welcome.is_none());
        assert_eq!(view.editor.document_count(), 1);
    });
}

#[gpui_kit::test]
fn welcome_recent_button_opens_project(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let path = harness._config_dir.path().join("recent.zipzi");
    let mut session = zipzi_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        zipzi_core::Canvas::new(42., 42., Default::default(), 1.),
    )
    .unwrap();
    session.save_project(&path, None).unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.record_recent_file(path.clone(), cx);
            view.show_welcome(window, cx);
        });
        window.render_frame(cx);
        window.click(("welcome-recent", 0usize), cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert!(view.welcome.is_none());
        assert_eq!(view.editor.document_count(), 2);
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
    });
}
