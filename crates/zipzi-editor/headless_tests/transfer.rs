use super::{Harness, setup};
use gpui_kit::component::WindowExt;
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};
use zipzi_editor_core::CommandId;
fn settle(cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(150));
    cx.run_until_parked();
}
fn open(harness: &Harness, id: CommandId, cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(harness.window, |_, window, cx| {
        window.activate_window();
        harness
            .view
            .update(cx, |view, cx| view.run_command(id, window, cx));
        window.render_frame(cx);
    })
    .unwrap();
    settle(cx);
}
#[gpui_kit::test]
fn cancel_export_settings_keeps_the_project_unchanged(cx: &mut TestAppContext) {
    let harness = setup(cx);
    let before = harness.view.read_with(cx, |view, _| {
        view.editor.document().unwrap().export_settings()
    });
    open(&harness, CommandId::ExportRuntime, cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
        assert!(window.find("export-settings").visible());
        window.click("export-ppu", cx);
        window.press("cmd-a", cx);
        window.input("64", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("export-cancel", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        assert!(!window.has_active_dialog(cx));
        let doc = harness.view.read(cx).editor.document().unwrap();
        assert_eq!(doc.export_settings(), before);
        assert_eq!(doc.history_lengths(), (0, 0));
    })
    .unwrap();
}
#[gpui_kit::test]
fn invalid_ppu_blocks_export_and_manual_mode_exposes_canvas(cx: &mut TestAppContext) {
    let harness = setup(cx);
    open(&harness, CommandId::ExportCubism, cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("export-manual", cx);
        window.render_frame(cx);
        assert!(window.find("export-width").visible());
        assert!(window.try_find("export-padding-pixels").is_none());
        window.click("export-ppu", cx);
        window.press("cmd-a", cx);
        window.input("0", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.press("enter", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
        assert_eq!(
            harness
                .view
                .read(cx)
                .editor
                .document()
                .unwrap()
                .history_lengths(),
            (0, 0)
        );
        window.click("export-cancel", cx);
    })
    .unwrap();
    settle(cx);
}
#[gpui_kit::test]
fn continue_remembers_settings_before_native_save_cancellation(cx: &mut TestAppContext) {
    let harness = setup(cx);
    open(&harness, CommandId::ExportRuntime, cx);
    assert!(!cx.did_prompt_for_new_path());
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        window.click("export-ppu", cx);
        window.press("cmd-a", cx);
        window.input("64", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.click("export-continue", cx);
    })
    .unwrap();
    settle(cx);
    assert!(cx.did_prompt_for_new_path());
    harness.view.read_with(cx, |view, _| {
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .export_settings()
                .pixels_per_unit,
            64.
        );
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
    cx.simulate_new_path_selection(|_| None);
    settle(cx);
    harness.view.read_with(cx, |view, _| {
        assert!(!view.modal);
        assert!(view.edit_error.is_none());
        assert_eq!(
            view.editor
                .document()
                .unwrap()
                .export_settings()
                .pixels_per_unit,
            64.
        );
    });
}
