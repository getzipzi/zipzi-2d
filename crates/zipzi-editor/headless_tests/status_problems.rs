use super::{Harness, setup};
use crate::status::{ProblemOrigin, ProblemSeverity};
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};
use std::{path::PathBuf, time::Duration};

fn setup_status(cx: &mut TestAppContext) -> Harness {
    let h = setup(cx);
    h.view.update(cx, |view, cx| {
        view._frames = None;
        view.requests = Some(crate::preview::RequestSender::deferred());
        view.reset_viewport_images();
        view.clear_edit_error(cx);
        view.status_bar
            .read(cx)
            .problems
            .clone()
            .update(cx, |p, cx| p.dismiss_all(cx));
    });
    h
}

#[gpui_kit::test]
fn status_problems_keep_errors_after_feedback_and_deduplicate_reports(cx: &mut TestAppContext) {
    let h = setup_status(cx);
    let problems = h
        .view
        .read_with(cx, |view, cx| view.status_bar.read(cx).problems.clone());
    h.view.update(cx, |view, cx| {
        view.set_edit_error("Cannot save the project", cx);
        view.set_edit_error("Cannot save the project", cx);
        view.set_status_message("Another action completed", cx);
    });
    cx.run_until_parked();
    cx.background_executor.advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
    assert_eq!(problems.read_with(cx, |p, _| p.counts()), (1, 0));
    h.view.read_with(cx, |view, _| {
        assert!(view.message.is_empty());
        assert!(view.edit_error.is_none());
    });
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("app-status-problems", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(Duration::from_millis(160));
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("status-problems-content").is_some());
        let id = problems.read(cx).reports()[0].id;
        let list = window.find("status-problems-list").bounds();
        let button = window.find(format!("status-problem-copy-{id}")).bounds();
        assert!(
            list.size.height > gpui_kit::px(0.) && list.contains(&button.center()),
            "{list:?} {button:?}"
        );
        window.click(format!("status-problem-copy-{id}"), cx);
        let text = cx.read_from_clipboard().unwrap().text().unwrap();
        assert!(text.contains("Cannot save the project"));
        window.click(format!("status-problem-dismiss-{id}"), cx);
    })
    .unwrap();
    cx.run_until_parked();
    assert_eq!(problems.read_with(cx, |p, _| p.counts()), (0, 0));
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("status-problems-content").is_none());
        h.view
            .update(cx, |view, cx| view.set_edit_error("A new failure", cx));
        window.render_frame(cx);
        assert!(window.try_find("app-status-problems").is_some());
        assert!(window.try_find("status-problems-content").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn status_problems_keep_warning_sources_full_details_and_other_reports_when_dismissed(
    cx: &mut TestAppContext,
) {
    let h = setup_status(cx);
    let problems = h
        .view
        .read_with(cx, |view, cx| view.status_bar.read(cx).problems.clone());
    let first = h
        .view
        .read_with(cx, |view, _| view.editor.active_id().unwrap());
    let source = PathBuf::from("/art/first.model3.json");
    let long = "Very long warning details with multiple lines\n".repeat(60);
    h.view.update(cx, |view, cx| {
        assert_eq!(
            view.record_load_warnings(
                &source,
                true,
                vec![
                    long.clone(),
                    long.clone(),
                    "".into(),
                    "Missing physics".into()
                ],
                cx
            ),
            2
        );
        view.editor.new_document().unwrap();
        view.record_load_warnings(
            &PathBuf::from("/art/second.psd"),
            true,
            vec!["Unsupported blend mode".into()],
            cx,
        );
        view.set_status_message("Saved", cx);
    });
    problems.read_with(cx, |p, _| {
        assert_eq!(p.counts(), (0, 3), "{:?}", p.reports());
        assert_eq!(
            p.reports()[0].origin,
            ProblemOrigin::Load {
                document: Some(first),
                source: source.clone()
            }
        );
        assert_eq!(p.reports()[0].messages[0], long);
        assert_eq!(p.reports()[0].severity, ProblemSeverity::Warning);
    });
    let id = problems.read_with(cx, |p, _| p.reports()[0].id);
    problems.update(cx, |p, cx| p.dismiss(id, cx));
    assert_eq!(problems.read_with(cx, |p, _| p.counts()), (0, 1));
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert_eq!(problems.read_with(cx, |p, _| p.counts()), (0, 1));
    problems.update(cx, |p, cx| p.dismiss_all(cx));
    assert_eq!(problems.read_with(cx, |p, _| p.counts()), (0, 0));
}

#[gpui_kit::test]
fn status_problems_acknowledge_and_resolve_renderer_faults(cx: &mut TestAppContext) {
    let h = setup_status(cx);
    let problems = h
        .view
        .read_with(cx, |view, cx| view.status_bar.read(cx).problems.clone());
    problems.update(cx, |p, cx| {
        p.sync_renderer(None, Some("GPU fault".into()), cx);
        p.dismiss_all(cx);
        p.sync_renderer(None, Some("GPU fault".into()), cx);
        assert_eq!(p.counts(), (0, 0));
        p.sync_renderer(None, None, cx);
        p.sync_renderer(None, Some("GPU fault".into()), cx);
        assert_eq!(p.counts(), (1, 0));
        p.sync_renderer(None, None, cx);
        assert_eq!(p.counts(), (0, 0));
    });
}

#[gpui_kit::test]
fn status_problems_retain_actual_model3_import_warnings_without_a_modal(cx: &mut TestAppContext) {
    use crate::project::ImportKind;
    use gpui_kit::component::WindowExt;
    let h = setup_status(cx);
    let source = h._config_dir.path().join("warning-source");
    std::fs::create_dir(&source).unwrap();
    let fixture =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/external_v50");
    // Missing texture and sidecar resources are recoverable import diagnostics.
    for name in ["model.moc3", "model.model3.json"] {
        std::fs::copy(fixture.join(name), source.join(name)).unwrap();
    }
    let entry = source.join("model.model3.json");
    cx.update_window(h.window, |_, window, cx| {
        h.view
            .update(cx, |view, cx| view.import(ImportKind::Model3, window, cx));
    })
    .unwrap();
    cx.simulate_path_prompt_response(|_| Some(vec![entry.clone()]));
    cx.run_until_parked();
    h.view.read_with(cx, |view, cx| {
        assert!(view.edit_error.is_none(), "{:?}", view.edit_error);
        assert_eq!(view.editor.document_count(), 2);
        let problems = view.status_bar.read(cx).problems.read(cx);
        assert!(problems.counts().1 > 0);
        assert_eq!(problems.reports()[0].path.as_ref(), Some(&entry));
        assert_eq!(
            problems.reports()[0].origin,
            ProblemOrigin::Load {
                document: view.editor.active_id(),
                source: entry.clone(),
            }
        );
    });
    cx.update_window(h.window, |_, window, cx| {
        assert!(!window.has_active_dialog(cx));
        window.render_frame(cx);
        assert!(window.try_find("app-status-problems").is_some());
        window.click("app-status-problems", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(Duration::from_secs(10));
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("status-problems-content").is_some());
    })
    .unwrap();
}
