use super::{Harness, setup};
use gpui_kit::{AppContext as _, TestAppContext};
use std::time::Duration;

fn advance(cx: &mut TestAppContext, seconds: u64) {
    cx.run_until_parked();
    cx.background_executor
        .advance_clock(Duration::from_secs(seconds));
    cx.run_until_parked();
}

fn setup_status(cx: &mut TestAppContext) -> Harness {
    let h = setup(cx);
    h.view.update(cx, |view, cx| {
        view._frames = None;
        view.requests = Some(crate::preview::RequestSender::deferred());
        view.reset_viewport_images();
        view.clear_edit_error(cx);
    });
    h
}

#[gpui_kit::test]
fn an_older_status_timeout_cannot_clear_a_newer_status(cx: &mut TestAppContext) {
    let h = setup_status(cx);
    let bar = h.view.read_with(cx, |view, _| view.status_bar.clone());
    h.view
        .update(cx, |view, cx| view.set_status_message("First", cx));
    advance(cx, 2);
    h.view
        .update(cx, |view, cx| view.set_status_message("Second", cx));
    advance(cx, 2);
    bar.read_with(cx, |bar, _| {
        assert_eq!(bar.displayed_message(), Some(("Second", false)))
    });
    advance(cx, 2);
    bar.read_with(cx, |bar, _| assert_eq!(bar.displayed_message(), None));
    h.view.read_with(cx, |view, _| {
        assert!(view.message.is_empty());
        assert!(view.edit_error.is_none());
        assert!(view.feedback.is_none());
    });
}

#[gpui_kit::test]
fn clearing_error_feedback_does_not_resurrect_an_old_success(cx: &mut TestAppContext) {
    let h = setup_status(cx);
    let bar = h.view.read_with(cx, |view, _| view.status_bar.clone());
    h.view
        .update(cx, |view, cx| view.set_status_message("Saved", cx));
    advance(cx, 1);
    h.view
        .update(cx, |view, cx| view.set_edit_error("Failed", cx));
    bar.read_with(cx, |bar, _| {
        assert_eq!(bar.displayed_message(), Some(("Failed", true)))
    });
    h.view.update(cx, |view, cx| view.clear_edit_error(cx));
    bar.read_with(cx, |bar, _| assert_eq!(bar.displayed_message(), None));
    let problems = bar.read_with(cx, |bar, _| bar.problems.clone());
    assert_eq!(
        problems.read_with(cx, |problems, _| problems.counts()),
        (1, 0)
    );
    advance(cx, 3);
    bar.read_with(cx, |bar, _| assert_eq!(bar.displayed_message(), None));
}

#[gpui_kit::test]
fn a_normal_status_can_cover_an_error_and_duplicates_refresh_their_lifetime(
    cx: &mut TestAppContext,
) {
    let h = setup_status(cx);
    let bar = h.view.read_with(cx, |view, _| view.status_bar.clone());
    h.view
        .update(cx, |view, cx| view.set_edit_error("Old failure", cx));
    advance(cx, 1);
    h.view
        .update(cx, |view, cx| view.set_status_message("Saved", cx));
    h.view
        .read_with(cx, |view, _| assert!(view.edit_error.is_none()));
    advance(cx, 2);
    h.view
        .update(cx, |view, cx| view.set_status_message("Saved", cx));
    advance(cx, 2);
    bar.read_with(cx, |bar, _| {
        assert_eq!(bar.displayed_message(), Some(("Saved", false)))
    });
    advance(cx, 2);
    bar.read_with(cx, |bar, _| assert_eq!(bar.displayed_message(), None));
}

#[gpui_kit::test]
fn status_tasks_defer_only_the_latest_feedback_until_all_tasks_finish(cx: &mut TestAppContext) {
    use gpui_kit::test::TestWindowExt;
    let h = setup_status(cx);
    let bar = h.view.read_with(cx, |view, _| view.status_bar.clone());
    let (first, second) = h.view.update(cx, |view, cx| {
        let first = view.begin_operation_status(
            "Importing".into(),
            h._config_dir.path().join("one.psd"),
            cx,
        );
        let second = view.begin_operation_status(
            "Saving".into(),
            h._config_dir.path().join("two.zipzi"),
            cx,
        );
        view.set_status_message("First hidden message", cx);
        (first, second)
    });
    advance(cx, 10);
    h.view.update(cx, |view, cx| {
        view.set_status_message("Latest hidden message", cx)
    });
    advance(cx, 10);
    h.view
        .update(cx, |view, cx| view.finish_load_status(first, cx));
    advance(cx, 10);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("app-status-tasks").is_some());
        assert!(window.try_find("app-status-message").is_none());
        assert_eq!(bar.read(cx).displayed_message(), None);
        h.view
            .update(cx, |view, cx| view.finish_load_status(second, cx));
        window.render_frame(cx);
        assert!(window.try_find("app-status-tasks").is_none());
        assert!(window.try_find("app-status-message").is_some());
    })
    .unwrap();
    bar.read_with(cx, |bar, _| {
        assert_eq!(
            bar.displayed_message(),
            Some(("Latest hidden message", false))
        )
    });
    advance(cx, 3);
    bar.read_with(cx, |bar, _| {
        assert_eq!(
            bar.displayed_message(),
            Some(("Latest hidden message", false))
        )
    });
    advance(cx, 1);
    bar.read_with(cx, |bar, _| assert_eq!(bar.displayed_message(), None));
}

#[gpui_kit::test]
fn status_starting_a_task_pauses_visible_feedback_until_completion(cx: &mut TestAppContext) {
    let h = setup_status(cx);
    let bar = h.view.read_with(cx, |view, _| view.status_bar.clone());
    h.view
        .update(cx, |view, cx| view.set_status_message("Saved", cx));
    advance(cx, 2);
    let task = h.view.update(cx, |view, cx| {
        view.begin_operation_status("Importing".into(), h._config_dir.path().join("one.psd"), cx)
    });
    advance(cx, 10);
    bar.read_with(cx, |bar, _| assert_eq!(bar.displayed_message(), None));
    h.view
        .update(cx, |view, cx| view.finish_load_status(task, cx));
    bar.read_with(cx, |bar, _| {
        assert_eq!(bar.displayed_message(), Some(("Saved", false)))
    });
    advance(cx, 3);
    bar.read_with(cx, |bar, _| {
        assert_eq!(bar.displayed_message(), Some(("Saved", false)))
    });
    advance(cx, 1);
    bar.read_with(cx, |bar, _| assert_eq!(bar.displayed_message(), None));
}
