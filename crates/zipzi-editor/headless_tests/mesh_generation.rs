use super::{Harness, document_fixture, setup_with_document};
use gpui_kit::component::WindowExt;
use gpui_kit::{AppContext as _, TestAppContext, test::TestWindowExt};
use zipzi_editor_core::CommandId;
fn settle(cx: &mut TestAppContext) {
    for _ in 0..4 {
        cx.run_until_parked();
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(150));
    }
    cx.run_until_parked();
}
fn open(h: &Harness, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        h.view.update(cx, |v, cx| {
            v.run_command(CommandId::GenerateMesh, window, cx)
        });
    })
    .unwrap();
    settle(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(!window.has_active_dialog(cx));
        assert!(window.find("mesh-generation-panel").visible());
        assert!(window.find("mesh-generation-counts").visible());
    })
    .unwrap();
}
fn click(h: &Harness, id: &'static str, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, w, cx| w.click(id, cx))
        .unwrap();
    settle(cx);
}
fn fill(h: &Harness, id: &'static str, value: &str, cx: &mut TestAppContext) {
    cx.update_window(h.window, |_, w, cx| {
        w.click(id, cx);
        w.press("cmd-a", cx);
        w.input(value, cx);
        w.blur(cx);
    })
    .unwrap();
    settle(cx);
}
#[gpui_kit::test]
fn viewport_generation_stages_preview_cancel_apply_and_undo(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::alpha_document().unwrap();
    let h = setup_with_document(cx, doc, assets);
    open(&h, cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0));
        assert!(v.editor.layers()[0].vertices > 4);
    });
    cx.update_window(h.window, |_, w, _| {
        assert!(w.try_find("mesh-generation-interior-spacing").is_none());
        assert!(
            w.find("mesh-generation-panel").bounds().top()
                >= w.find("mesh-edit-actions").bounds().bottom()
        );
    })
    .unwrap();
    click(&h, "mesh-generation-stage-interior", cx);
    cx.update_window(h.window, |_, w, _| {
        assert!(w.try_find("mesh-generation-edge-spacing").is_none());
        assert!(w.find("mesh-generation-interior-spacing").visible());
        assert!(w.find("mesh-generation-counts").visible());
    })
    .unwrap();
    click(&h, "mesh-generation-cancel", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0));
        assert_eq!(v.editor.layers()[0].vertices, 4);
        assert!(v.mesh_generation.is_none());
    });
    open(&h, cx);
    click(&h, "mesh-generation-submit", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.document().unwrap().history_lengths(), (1, 0));
        assert!(v.mesh_generation.is_none());
    });
    cx.update_window(h.window, |_, w, cx| {
        h.view
            .update(cx, |v, cx| v.run_command(CommandId::Undo, w, cx))
    })
    .unwrap();
    settle(cx);
    h.view
        .read_with(cx, |v, _| assert_eq!(v.editor.layers()[0].vertices, 4));
}
#[gpui_kit::test]
fn invalid_inputs_clear_candidate_and_escape_discards_the_session(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::alpha_document().unwrap();
    let h = setup_with_document(cx, doc, assets);
    open(&h, cx);
    fill(&h, "mesh-generation-edge-spacing", "0", cx);
    cx.update_window(h.window, |_, w, _| {
        assert!(w.find("mesh-generation-error").visible());
        assert!(w.try_find("mesh-generation-counts").is_none());
    })
    .unwrap();
    h.view
        .read_with(cx, |v, _| assert_eq!(v.editor.layers()[0].vertices, 4));
    fill(&h, "mesh-generation-edge-spacing", "12.5", cx);
    cx.update_window(h.window, |_, w, cx| {
        assert!(w.find("mesh-generation-counts").visible());
        h.view
            .update(cx, |v, cx| v.run_command(CommandId::CancelGesture, w, cx));
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |v, _| {
        assert!(v.mesh_generation.is_none());
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0));
    });
}

#[gpui_kit::test]
fn interior_can_use_original_boundary_without_accumulating_preview_points(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::alpha_document().unwrap();
    let h = setup_with_document(cx, doc, assets);
    open(&h, cx);
    click(&h, "mesh-generation-stage-interior", cx);
    fill(&h, "mesh-generation-interior-spacing", "4096", cx);
    click(&h, "mesh-generation-use-original", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.layers()[0].vertices, 4);
    });
    fill(&h, "mesh-generation-interior-spacing", "12", cx);
    h.view.read_with(cx, |v, _| {
        assert!(v.editor.layers()[0].vertices > 4);
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0));
    });
    fill(&h, "mesh-generation-interior-spacing", "4096", cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.layers()[0].vertices, 4);
    });
    click(&h, "mesh-generation-cancel", cx);
}
#[gpui_kit::test]
fn fill_controls_retain_parameters_and_small_viewport_keeps_actions_visible(
    cx: &mut TestAppContext,
) {
    let (doc, assets) = document_fixture::alpha_document().unwrap();
    let h = setup_with_document(cx, doc, assets);
    open(&h, cx);
    click(&h, "mesh-generation-stage-interior", cx);
    fill(&h, "mesh-generation-edge-ratio", "1.5", cx);
    cx.update_window(h.window, |_, w, cx| {
        w.click("mesh-generation-fill-algorithm", cx);
        w.press("down", cx);
        w.press("down", cx);
        w.press("enter", cx);
    })
    .unwrap();
    settle(cx);
    fill(&h, "mesh-generation-edge-ratio", "3", cx);
    cx.update_window(h.window, |_, w, cx| {
        w.click("mesh-generation-fill-algorithm", cx);
        w.press("up", cx);
        w.press("up", cx);
        w.press("enter", cx);
    })
    .unwrap();
    settle(cx);
    cx.update_window(h.window, |_, w, cx| {
        w.click("mesh-generation-edge-ratio", cx);
        assert_eq!(w.focused_input(cx).unwrap().value(cx).as_ref(), "1.5");
        w.blur(cx);
    })
    .unwrap();
    cx.simulate_window_resize(
        h.window,
        gpui_kit::size(gpui_kit::px(800.), gpui_kit::px(480.)),
    );
    settle(cx);
    cx.update_window(h.window, |_, w, cx| {
        w.render_frame(cx);
        for id in ["mesh-generation-submit", "mesh-generation-cancel"] {
            let b = w.find(id).bounds();
            assert!(b.right() <= gpui_kit::px(800.) && b.bottom() <= gpui_kit::px(480.));
        }
    })
    .unwrap();
    click(&h, "mesh-generation-cancel", cx);
}

#[gpui_kit::test]
fn generation_preserves_camera_navigation_and_discards_on_document_switch(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::alpha_document().unwrap();
    let h = setup_with_document(cx, doc, assets);
    open(&h, cx);
    let (original, zoom) = h.view.read_with(cx, |v, _| {
        (v.editor.active_id().unwrap(), v.editor.view().zoom)
    });
    cx.update_window(h.window, |_, w, cx| {
        h.view
            .update(cx, |v, cx| v.run_command(CommandId::ZoomIn, w, cx))
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |v, _| {
        assert!(v.mesh_generation.is_some());
        assert!(v.editor.view().zoom > zoom);
    });
    // Start another request, then switch documents before its debounce completes.
    cx.update_window(h.window, |_, w, cx| {
        w.click("mesh-generation-edge-spacing", cx);
        w.press("cmd-a", cx);
        w.input("18", cx);
        h.view.update(cx, |v, cx| {
            v.editor.new_document().unwrap();
            v.refresh_document(w, cx);
        });
    })
    .unwrap();
    settle(cx);
    h.view
        .read_with(cx, |v, _| assert!(v.mesh_generation.is_none()));
    cx.update_window(h.window, |_, w, cx| {
        h.view.update(cx, |v, cx| {
            v.editor.activate(original).unwrap();
            v.refresh_document(w, cx);
        })
    })
    .unwrap();
    settle(cx);
    h.view.read_with(cx, |v, _| {
        assert_eq!(v.editor.layers()[0].vertices, 4);
        assert_eq!(v.editor.document().unwrap().history_lengths(), (0, 0));
    });
}
