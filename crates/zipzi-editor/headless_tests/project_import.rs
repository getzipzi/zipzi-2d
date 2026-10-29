use super::setup;
use crate::{project::ImportKind, settings::Settings};
use gpui_kit::{AppContext as _, TestAppContext};

#[gpui_kit::test]
fn psd_picker_opens_an_unsaved_document_and_save_asks_for_a_path(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/zipzi-psd/tests/fixtures/layered.psd");
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.import(ImportKind::Psd, window, cx));
    })
    .unwrap();
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|options| {
        assert!(options.files && !options.directories && !options.multiple);
        Some(vec![source])
    });
    cx.run_until_parked();
    assert!(!cx.did_prompt_for_new_path());
    let imported = harness.view.read_with(cx, |view, cx| {
        assert!(!view.modal);
        assert!(view.status_bar.read(cx).load_states().is_empty());
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert_eq!(view.editor.documents().len(), 2);
        let document = view.editor.document().unwrap();
        assert!(document.path().is_none());
        assert!(document.modified());
        assert_eq!(document.import_name(), Some("layered"));
        assert_eq!(document.layers().len(), 1);
        assert!(cx.global::<Settings>().recent_files.is_empty());
        view.editor.active_id()
    });
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.save(false, window, cx));
    })
    .unwrap();
    assert!(cx.did_prompt_for_new_path());
    let destination = harness._config_dir.path().join("layers.zipzi");
    cx.simulate_new_path_selection(|_| Some(destination.clone()));
    cx.run_until_parked();
    harness.view.read_with(cx, |view, cx| {
        assert_eq!(view.editor.active_id(), imported);
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert!(!view.editor.document().unwrap().modified());
        assert_eq!(
            cx.global::<Settings>().recent_files.paths,
            vec![destination.clone()]
        );
    });
    assert!(destination.join("project.zipzi.json").exists());
}

#[gpui_kit::test]
fn failed_or_cancelled_psd_picker_preserves_the_active_document(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let original = harness
        .view
        .read_with(cx, |view, _| view.editor.active_id());
    for source in [None, Some(harness._config_dir.path().join("bad.psd"))] {
        if let Some(path) = &source {
            std::fs::write(path, b"bad").unwrap();
        }
        cx.update_window(harness.window, |_, window, cx| {
            harness
                .view
                .update(cx, |view, cx| view.import(ImportKind::Psd, window, cx));
        })
        .unwrap();
        cx.simulate_path_prompt_response(|_| source.clone().map(|path| vec![path]));
        cx.run_until_parked();
        assert!(!cx.did_prompt_for_new_path());
        harness.view.read_with(cx, |view, cx| {
            assert!(!view.modal);
            assert!(view.status_bar.read(cx).load_states().is_empty());
            assert_eq!(view.editor.active_id(), original);
            assert_eq!(view.editor.documents().len(), 1);
            assert_eq!(view.edit_error.is_some(), source.is_some());
        });
    }
}

#[gpui_kit::test]
fn load_status_tracks_concurrent_jobs_and_status_priority_without_blocking_editor(
    cx: &mut TestAppContext,
) {
    use gpui_kit::test::TestWindowExt;
    use std::time::Duration;
    use zipzi_editor_core::{LoadRequest, LoadStage};

    let harness = setup(cx);
    let first_path = harness._config_dir.path().join("first.zipzi");
    let second_path = harness._config_dir.path().join("second.psd");
    let (bar, first, second, first_task, second_task) = cx
        .update_window(harness.window, |_, window, cx| {
            let result = harness.view.update(cx, |view, cx| {
                // This test advances status lifetimes without a GPU worker.
                view._frames = None;
                view.requests = Some(crate::preview::RequestSender::deferred());
                view.reset_viewport_images();
                view.clear_edit_error(cx);
                let first_task = view
                    .editor
                    .prepare_load(LoadRequest::Open(first_path.clone()))
                    .unwrap()
                    .unwrap();
                let second_task = view
                    .editor
                    .prepare_load(LoadRequest::Psd(second_path.clone()))
                    .unwrap()
                    .unwrap();
                let bar = view.status_bar.clone();
                let first =
                    view.begin_load_status(first_path.clone(), false, first_task.progress(), cx);
                let second =
                    view.begin_load_status(second_path.clone(), true, second_task.progress(), cx);
                assert!(!view.modal);
                assert_eq!(view.editor.document_count(), 1);
                (bar, first, second, first_task, second_task)
            });
            window.render_frame(cx);
            result
        })
        .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.set_status_message("Saved another document", cx);
        });
        assert_eq!(bar.read(cx).displayed_message(), None);
        assert_eq!(harness.view.read(cx).message, "Saved another document");
        window.render_frame(cx);
        // Tasks take the shared center slot; ordinary feedback waits.
        assert!(window.try_find("app-status-tasks").is_some());
        assert!(window.try_find("app-status-message").is_none());
    })
    .unwrap();
    cx.background_executor
        .advance_clock(Duration::from_secs(65));
    cx.run_until_parked();
    bar.read_with(cx, |bar, _| {
        assert_eq!(
            bar.load_states(),
            vec![
                (first_path, LoadStage::Queued),
                (second_path.clone(), LoadStage::Queued),
            ]
        );
        assert_eq!(bar.displayed_message(), None);
    });
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        let activity = window.find("app-status-tasks").bounds();
        let status = window.find("app-status-bar").bounds();
        assert!((activity.center().x - status.center().x).abs() <= gpui_kit::px(0.5));
        assert!(activity.left() >= status.left() && activity.right() <= status.right());
        assert!(activity.top() >= status.top() && activity.bottom() <= status.bottom());
        harness
            .view
            .update(cx, |view, cx| view.set_edit_error("Example failure", cx));
        window.render_frame(cx);
        assert!(window.try_find("app-status-tasks").is_some());
        harness
            .view
            .update(cx, |view, cx| view.finish_load_status(first, cx));
    })
    .unwrap();
    assert_eq!(bar.read_with(cx, |bar, _| bar.load_states()).len(), 1);
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(crate::EditorView::STATUS_MESSAGE_TIMEOUT);
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("app-status-tasks").is_some());
        harness.view.update(cx, |view, cx| {
            view.set_status_message("Still visible after the job finishes", cx);
        });
    })
    .unwrap();
    let first_completion = first_task.run();
    let second_completion = second_task.run();
    cx.background_executor
        .advance_clock(Duration::from_millis(250));
    cx.run_until_parked();
    bar.read_with(cx, |bar, _| {
        assert_eq!(bar.load_states()[0].1, LoadStage::Reading)
    });
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            assert!(view.editor.finish_load(first_completion).is_err());
            assert!(view.editor.finish_load(second_completion).is_err());
            view.finish_load_status(second, cx);
        });
        window.render_frame(cx);
        assert!(window.try_find("app-status-tasks").is_none());
        assert_eq!(
            bar.read(cx).displayed_message(),
            Some(("Still visible after the job finishes", false))
        );
    })
    .unwrap();
    cx.background_executor.advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
    assert!(bar.read_with(cx, |bar, _| bar.load_states()).is_empty());
}

#[gpui_kit::test]
fn save_as_replaces_existing_project_after_a_failed_save(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let destination = harness._config_dir.path().join("existing.zipzi");
    let mut existing = zipzi_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        zipzi_core::Canvas::new(42., 42., Default::default(), 1.),
    )
    .unwrap();
    existing.save_project(&destination, None).unwrap();
    let manifest = destination.join("project.zipzi.json");
    let before = std::fs::read(&manifest).unwrap();
    let document = harness
        .view
        .read_with(cx, |view, _| view.editor.active_id().unwrap());
    // A save without a replacement confirmation must continue to reject it.
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.save_to(document, destination.clone(), false, window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert!(view.edit_error.is_some());
        assert!(!view.editor.is_busy());
    });
    assert_eq!(std::fs::read(&manifest).unwrap(), before);

    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.save(true, window, cx));
    })
    .unwrap();
    assert!(cx.did_prompt_for_new_path());
    cx.simulate_new_path_selection(|_| Some(destination.clone()));
    cx.run_until_parked();
    harness.view.read_with(cx, |view, cx| {
        assert!(!view.modal);
        assert!(!view.editor.is_busy());
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert!(!view.editor.document().unwrap().modified());
        assert_eq!(view.editor.active_id(), Some(document));
        assert_eq!(
            cx.global::<Settings>().recent_files.paths,
            vec![destination]
        );
    });
    assert_ne!(std::fs::read(&manifest).unwrap(), before);
    assert!(
        existing
            .open_project(&manifest, None)
            .unwrap()
            .resources_complete()
    );
    assert_eq!(existing.mesh_ids().len(), 2);
}
