use super::{Harness, document_fixture, setup_with_document};
use editor_core::{CommandId, EditorCommand};
use gpui_kit::component::WindowExt;
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};

fn open(harness: &Harness, cx: &mut TestAppContext) {
    cx.update(|cx| cx.set_reduce_motion(true));
    cx.update_window(harness.window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.click("tool_generate_mesh", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.has_active_dialog(cx));
        assert!(window.find("mesh-generation-form").visible());
    })
    .unwrap();
}
fn fill(harness: &Harness, field: &'static str, value: &str, cx: &mut TestAppContext) {
    cx.update_window(harness.window, |_, window, cx| {
        window.click(field, cx);
        window.press("cmd-a", cx);
        window.input(value, cx);
    })
    .unwrap();
    cx.run_until_parked();
}

#[gpui_kit::test]
fn toolbar_generation_submits_once_and_refreshes_history(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::rectangle_document(false).unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open(&harness, cx);
    fill(&harness, "mesh-generation-columns", "4", cx);
    fill(&harness, "mesh-generation-rows", "3", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
        harness
            .view
            .read(cx)
            .editor
            .document()
            .map(|d| {
                assert_eq!((d.layers()[0].vertices, d.layers()[0].triangles), (20, 24));
                assert_eq!(d.history_lengths(), (1, 0));
            })
            .unwrap();
        harness
            .view
            .update(cx, |view, cx| view.run_command(CommandId::Undo, window, cx));
        assert_eq!(harness.view.read(cx).editor.layers()[0].vertices, 4);
        harness
            .view
            .update(cx, |view, cx| view.run_command(CommandId::Redo, window, cx));
        assert_eq!(harness.view.read(cx).editor.layers()[0].vertices, 20);
    })
    .unwrap();
}

#[gpui_kit::test]
fn invalid_bound_counts_and_cancel_leave_mesh_unchanged(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::rectangle_document(true).unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open(&harness, cx);
    for value in ["0", "-1", "abc", "256", "4"] {
        fill(&harness, "mesh-generation-columns", value, cx);
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("mesh-generation-error").visible());
            window.press("enter", cx);
        })
        .unwrap();
        cx.run_until_parked();
        harness.view.read_with(cx, |v, _| {
            assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0));
        });
    }
    cx.update_window(harness.window, |_, window, cx| {
        window.click("mesh-generation-cancel", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        assert!(!window.has_active_dialog(cx));
        assert_eq!(harness.view.read(cx).editor.layers()[0].vertices, 4);
    })
    .unwrap();
}

#[gpui_kit::test]
fn changed_document_blocks_dialog_submission(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::rectangle_document(false).unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open(&harness, cx);
    cx.update_window(harness.window, |_, _, cx| {
        harness.view.update(cx, |view, cx| {
            let target = view
                .editor
                .document()
                .unwrap()
                .context()
                .selection
                .clone()
                .unwrap();
            view.editor
                .execute(EditorCommand::SetObjectsLocked {
                    objects: vec![target],
                    locked: true,
                })
                .unwrap();
            cx.notify();
        });
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("mesh-generation-error").visible());
        window.press("enter", cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.layers()[0].vertices, 4);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
}

#[gpui_kit::test]
fn live_preview_presets_and_cancel_do_not_edit_document(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::rectangle_document(false).unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open(&harness, cx);
    for preset in [
        "mesh-generation-dense",
        "mesh-generation-sparse",
        "mesh-generation-medium",
    ] {
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            window.click(preset, cx);
        })
        .unwrap();
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            assert!(window.find("mesh-generation-preview").visible());
            assert!(window.find("mesh-generation-counts").visible());
            let doc = harness.view.read(cx).editor.document().unwrap();
            assert_eq!(doc.layers()[0].vertices, 4);
            assert_eq!(doc.history_lengths(), (0, 0));
            assert!(!doc.modified());
        })
        .unwrap();
    }
    cx.update_window(harness.window, |_, window, cx| {
        window.click("preview-wireframe", cx);
        window.render_frame(cx);
        assert!(window.try_find("mesh-generation-quality-help").is_none());
        window.click("mesh-generation-cancel", cx);
    })
    .unwrap();
    cx.run_until_parked();
    cx.update_window(harness.window, |_, window, cx| {
        assert!(!window.has_active_dialog(cx));
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
    })
    .unwrap();
}

#[gpui_kit::test]
fn changed_inputs_remove_old_preview_until_valid_again(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::rectangle_document(false).unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open(&harness, cx);
    fill(&harness, "mesh-generation-columns", "0", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("mesh-generation-counts").is_none());
        window.click("mesh-generation-submit", cx);
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
    })
    .unwrap();
    fill(&harness, "mesh-generation-columns", "4", cx);
    fill(&harness, "mesh-generation-rows", "4", cx);
    cx.update_window(harness.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.find("mesh-generation-counts").visible());
        window.click("mesh-generation-submit", cx);
    })
    .unwrap();
    cx.run_until_parked();
    harness.view.read_with(cx, |view, _| {
        assert_eq!(view.editor.document().unwrap().layers()[0].vertices, 25);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (1, 0));
    });
}

#[gpui_kit::test]
fn generator_uses_square_preview_and_keeps_actions_visible_when_resized(cx: &mut TestAppContext) {
    use gpui_kit::{px, size};
    let (doc, assets) = document_fixture::rectangle_document(false).unwrap();
    let harness = setup_with_document(cx, doc, assets);
    open(&harness, cx);
    for (width, height) in [(1100., 740.), (900., 620.), (1500., 1000.)] {
        cx.simulate_window_resize(harness.window, size(px(width), px(height)));
        cx.run_until_parked();
        cx.update_window(harness.window, |_, window, cx| {
            window.render_frame(cx);
            let preview = window.find("preview-viewport").bounds();
            let settings = window.find("mesh-generation-settings").bounds();
            assert!((preview.size.width - preview.size.height).as_f32().abs() < 1.);
            assert!(settings.right() <= preview.left());
            assert!(preview.size.height.as_f32() >= height - 210.);
            assert!(window.find("mesh-generation-submit").visible());
            assert!(window.find("mesh-generation-cancel").visible());
            assert!(window.try_find("mesh-generation-help").is_none());
            assert!(preview.bottom() < px(height));
        })
        .unwrap();
    }
}
