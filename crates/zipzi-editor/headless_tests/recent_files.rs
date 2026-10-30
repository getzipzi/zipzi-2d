use super::setup;
use crate::actions::{ClearRecent, OpenRecent};
use crate::settings::Settings;
use gpui_kit::{AppContext as _, TestAppContext};
use std::path::PathBuf;
use zipzi_editor_core::RecentFiles;

#[gpui_kit::test]
fn recent_files_record_clear_and_missing_handling(cx: &mut TestAppContext) {
    let harness = setup(cx);

    // Initial state: recent files list is empty
    let recent = cx.read(|cx| cx.global::<Settings>().recent_files.clone());
    assert!(recent.is_empty());

    // Record a recent file
    let test_file = harness._config_dir.path().join("model1.zipzi");
    std::fs::write(&test_file, b"{}").unwrap();

    cx.update_window(harness.window, |_, _, cx| {
        harness.view.update(cx, |view, cx| {
            view.record_recent_file(test_file.clone(), cx);
        });
    })
    .unwrap();

    let recent = cx.read(|cx| cx.global::<Settings>().recent_files.clone());
    assert_eq!(recent.len(), 1);
    assert_eq!(recent.paths[0], test_file);

    // Verify it was persisted to config store
    let store = cx.read(|cx| cx.global::<Settings>().store.clone());
    let loaded: Option<RecentFiles> = store.read("recent-files.json").unwrap();
    assert_eq!(loaded.as_ref().map(|r| r.len()), Some(1));

    // Open non-existent recent file removes it and sets edit_error
    let missing_file = PathBuf::from("/non/existent/path/model_missing.zipzi");
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.record_recent_file(missing_file.clone(), cx);
        });
        window.dispatch_action(
            Box::new(OpenRecent {
                path: missing_file.clone(),
            }),
            cx,
        );
    })
    .unwrap();

    let (recent, error) = cx.read(|cx| {
        let settings = cx.global::<Settings>();
        let view = harness.view.read(cx);
        (settings.recent_files.clone(), view.edit_error.clone())
    });
    assert!(!recent.paths.contains(&missing_file));
    assert!(error.is_some_and(|e| e.contains("model_missing.zipzi")));

    // Dispatch ClearRecent action clears recent files
    cx.update_window(harness.window, |_, window, cx| {
        window.dispatch_action(Box::new(ClearRecent), cx);
    })
    .unwrap();

    let recent = cx.read(|cx| cx.global::<Settings>().recent_files.clone());
    assert!(recent.is_empty());

    let loaded: Option<RecentFiles> = store.read("recent-files.json").unwrap();
    assert_eq!(loaded.as_ref().map(|r| r.len()), Some(0));
}

#[gpui_kit::test]
fn recent_files_open_and_save_keep_one_project_directory(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let project = harness._config_dir.path().join("recent.zipzi");
    let mut session = zipzi_sdk::AuthoringSession::new(
        &uuid::Uuid::new_v4().to_string(),
        zipzi_core::Canvas::new(42., 42., Default::default(), 1.),
    )
    .unwrap();
    session.save_project(&project, None).unwrap();
    let project = std::fs::canonicalize(project).unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.record_recent_file(project.clone(), cx);
            view.open_recent(&project, window, cx);
        });
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, cx| {
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert_eq!(
            cx.global::<Settings>().recent_files.paths,
            vec![project.clone()]
        );
    });
    cx.update_window(harness.window, |_, window, cx| {
        harness
            .view
            .update(cx, |view, cx| view.save(false, window, cx));
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, cx| {
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert_eq!(
            cx.global::<Settings>().recent_files.paths,
            vec![project.clone()]
        );
        let persisted: RecentFiles = cx
            .global::<Settings>()
            .store
            .read("recent-files.json")
            .unwrap()
            .unwrap();
        assert_eq!(persisted.paths, vec![project]);
    });
}
