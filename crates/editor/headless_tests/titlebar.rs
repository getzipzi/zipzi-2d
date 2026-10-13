use super::setup;
use editor_core::CommandId;
use gpui_kit::{AppContext as _, TestAppContext, point, px, test::TestWindowExt};

#[gpui_kit::test]
fn tabs_share_titlebar_and_leave_room_for_native_controls_and_dragging(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let title = window.find("workspace-titlebar").bounds();
        let tabs = window.find("global-document-tabs").bounds();
        let drag = window.find("window-titlebar-drag-region").bounds();
        let first = harness.view.read(cx).editor.active_id().unwrap();
        let tab = window.find(format!("tab-item-{first:?}")).bounds();
        assert_eq!(title.top(), px(0.));
        assert!(tab.left() >= px(80.), "tabs must clear the traffic lights");
        assert_eq!(tabs.top(), title.top() + px(1.));
        assert!(tabs.bottom() <= title.bottom());
        assert!(drag.size.width >= px(72.));
        let entry = window.find("project-new-or-open").bounds();
        assert!(entry.left() >= tab.right());
        assert!(entry.right() <= drag.left());
        assert!(drag.right() <= title.right());
        assert!(title.bottom() <= window.find("workspace-top-bar").bounds().top());

        window.toggle_fullscreen();
        window.render_frame(cx);
        assert!(window.find(format!("tab-item-{first:?}")).bounds().left() < tab.left());
        window.toggle_fullscreen();
        window.render_frame(cx);
        assert_eq!(
            window.find(format!("tab-item-{first:?}")).bounds().left(),
            tab.left()
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn tab_click_double_click_drag_and_close_do_not_trigger_window_drag(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        let first = harness.view.read(cx).editor.active_id().unwrap();
        harness
            .view
            .update(cx, |view, cx| view.run_command(CommandId::New, window, cx));
        let second = harness.view.read(cx).editor.active_id().unwrap();
        window.click(format!("tab-item-{first:?}"), cx);
        assert_eq!(harness.view.read(cx).editor.active_id(), Some(first));
        window.double_click(format!("tab-item-{second:?}"), cx);
        assert_eq!(harness.view.read(cx).editor.active_id(), Some(second));
        let from = window.find(format!("tab-item-{first:?}")).bounds().center();
        // TestWindow::start_window_move is unimplemented and panics. This real
        // pointer sequence also guards against arming a titlebar window drag.
        window.drag(from, from + point(px(20.), px(0.)), cx);
        window.click(format!("tab-close-{second:?}"), cx);
        assert_eq!(harness.view.read(cx).editor.document_count(), 1);
        assert_eq!(harness.view.read(cx).editor.active_id(), Some(first));
    })
    .unwrap();
}

#[gpui_kit::test]
fn overflowing_project_tabs_keep_a_visible_window_drag_region(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            for _ in 0..16 {
                view.editor.new_document().unwrap();
            }
            view.refresh_document(window, cx);
        });
        window.render_frame(cx);
        let title = window.find("workspace-titlebar").bounds();
        let drag = window.find("window-titlebar-drag-region").bounds();
        assert!(window.find("project-new-or-open").visible());
        assert!(window.find("project-new-or-open").bounds().right() <= drag.left());
        assert!(
            window.find("window-titlebar-drag-region").visible(),
            "title={title:?}, drag={drag:?}, tabs={:?}",
            window.find("global-document-tabs").bounds()
        );
        assert_eq!(drag.size.width, px(72.));
        assert!(drag.right() <= title.right());
        assert_eq!(title.size.width, px(1100.));
    })
    .unwrap();
}

#[gpui_kit::test]
fn persistent_project_menu_creates_and_opens_from_an_empty_window(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            view.run_command(CommandId::Close, window, cx)
        });
        window.render_frame(cx);
        assert_eq!(harness.view.read(cx).editor.document_count(), 0);
        assert!(window.find("project-new-or-open").visible());
        window.click("project-new-or-open", cx);
        window.within("popup-menu").click(0usize, cx);
    })
    .unwrap();
    cx.update_window(harness.window, |_, window, cx| {
        assert_eq!(harness.view.read(cx).editor.document_count(), 1);
        let id = harness.view.read(cx).editor.active_id().unwrap();
        window.click(format!("tab-close-{id:?}"), cx);
        window.render_frame(cx);
        assert_eq!(harness.view.read(cx).editor.document_count(), 0);
        assert!(window.find("project-new-or-open").visible());
        window.click("project-new-or-open", cx);
        window.within("popup-menu").click(1usize, cx);
    })
    .unwrap();
    assert!(cx.did_prompt_for_paths());
    cx.simulate_path_prompt_response(|_| None);
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("project-new-or-open").visible());
        assert!(!harness.view.read(cx).modal);
        assert_eq!(harness.view.read(cx).editor.document_count(), 0);
    })
    .unwrap();
}
