use super::setup;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{AppContext as _, TestAppContext, point, px};

#[gpui_kit::test]
fn hand_tool_drags_canvas_and_select_tool_restores_mode(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("tool_hand", cx);
        harness.view.read_with(cx, |view, cx| {
            assert!(view.dock.as_ref().unwrap().canvas.read(cx).hand_tool());
        });

        let canvas = window.find("canvas-body").bounds();
        let from = canvas.center();
        let to = point(from.x + px(40.), from.y + px(20.));
        let before = harness.view.read_with(cx, |view, _| view.editor.view().pan);
        window.drag(from, to, cx);
        let after = harness.view.read_with(cx, |view, _| view.editor.view().pan);
        assert_eq!(after.x - before.x, 40.);
        assert_eq!(after.y - before.y, 20.);

        window.click("tool_select", cx);
        harness.view.read_with(cx, |view, cx| {
            assert!(!view.dock.as_ref().unwrap().canvas.read(cx).hand_tool());
        });
    })
    .unwrap();
}

#[gpui_kit::test]
fn floating_toolbar_and_zoom_controls(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        // Verify top bar is removed
        assert!(
            window.try_find("app-toolbar").is_none(),
            "top bar app-toolbar should be removed"
        );

        // Verify floating toolbar exists on canvas
        assert!(
            window.try_find("canvas-floating-toolbar").is_some(),
            "canvas-floating-toolbar should exist"
        );
        assert!(window.try_find("tool_select").is_some());
        assert!(window.try_find("tool_mesh").is_some());
        assert!(window.try_find("tool_deformer").is_some());
        assert!(window.try_find("tool_hand").is_some());

        // Verify floating zoom bar exists on canvas
        assert!(
            window.try_find("canvas-floating-zoom").is_some(),
            "canvas-floating-zoom should exist"
        );
        assert!(window.try_find("btn_zoom_out").is_some());
        assert!(window.try_find("zoom-percentage").is_some());
        assert!(window.try_find("btn_zoom_in").is_some());

        // Test zoom buttons
        let initial_zoom = harness
            .view
            .read_with(cx, |view, _| view.editor.view().zoom);
        window.click("btn_zoom_in", cx);
        let zoomed_in = harness
            .view
            .read_with(cx, |view, _| view.editor.view().zoom);
        assert!(zoomed_in > initial_zoom);

        window.click("btn_zoom_out", cx);
        let zoomed_out = harness
            .view
            .read_with(cx, |view, _| view.editor.view().zoom);
        assert!(zoomed_out < zoomed_in);
    })
    .unwrap();
}

#[gpui_kit::test]
fn welcome_text_centered_when_no_active_document(cx: &mut TestAppContext) {
    let harness = setup(cx);
    cx.update_window(harness.window, |_, window, cx| {
        // Initially active document is loaded, welcome is not displayed
        assert!(window.try_find("canvas-welcome").is_none());

        // Switch to empty editor (active is None)
        harness.view.update(cx, |view, cx| {
            view.editor = editor_core::Editor::empty();
            cx.notify();
        });
        window.render_frame(cx);

        // canvas-welcome is displayed and covers canvas-body
        let canvas_body = window.find("canvas-body").bounds();
        let welcome = window.find("canvas-welcome").bounds();
        assert_eq!(welcome.origin, canvas_body.origin);
        assert_eq!(welcome.size, canvas_body.size);
    })
    .unwrap();
}
