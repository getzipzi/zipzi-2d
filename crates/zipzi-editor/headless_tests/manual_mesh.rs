use super::{Harness, document_fixture, setup_with_document};
use gpui_kit::{AppContext as _, TestAppContext, point, px, test::TestWindowExt};

fn present(h: &Harness, cx: &mut TestAppContext) {
    cx.run_until_parked();
    cx.update_window(h.window, |_, window, cx| {
        h.view.update(cx, |view, cx| {
            let frame = view.editor.preview().unwrap().canvas.unwrap();
            let device = metal::Device::system_default().unwrap();
            let desc = metal::TextureDescriptor::new();
            desc.set_width(1);
            desc.set_height(1);
            desc.set_pixel_format(metal::MTLPixelFormat::RGBA8Unorm);
            assert!(view.editor.present_frame(frame.clone()));
            view.viewport_frames
                .entry(view.viewport_slots[0])
                .or_default()
                .modeling = Some(crate::preview::Presented {
                image: device.new_texture(&desc),
                canvas: frame,
            });
            cx.notify();
        });
        window.render_frame(cx);
    })
    .unwrap();
}
fn enter(cx: &mut TestAppContext) -> Harness {
    let (doc, assets) = document_fixture::alpha_document().unwrap();
    let h = setup_with_document(cx, doc, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.click("tool_manual_mesh", cx);
    })
    .unwrap();
    assert!(cx.has_pending_prompt());
    cx.simulate_prompt_answer("Use full image");
    present(&h, cx);
    assert!(h.view.read_with(cx, |v, _| v.editor.manual_active()));
    h
}

#[gpui_kit::test]
fn manual_mesh_v_selects_points_without_leaving_the_mode(cx: &mut TestAppContext) {
    let h = enter(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("tool_manual_create", cx);
    })
    .unwrap();
    present(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        window.press("v", cx);
        assert!(
            h.view.read(cx).editor.manual_active(),
            "V should choose manual point selection, not exit manual mesh editing"
        );
        assert_eq!(
            h.view.read(cx).editor.manual_tool(),
            Some(zipzi_editor_core::ManualTool::Select)
        );
    })
    .unwrap();
}

#[gpui_kit::test]
fn manual_mesh_failed_click_explains_why_and_next_valid_click_recovers(cx: &mut TestAppContext) {
    let h = enter(cx);
    cx.update_window(h.window, |_, window, cx| {
        window.click("tool_manual_create", cx);
    })
    .unwrap();
    present(&h, cx);
    cx.update_window(h.window, |_, window, cx| {
        let transform = h.view.read(cx).modeling_frame().unwrap().canvas.transform;
        let p = transform.to_viewport(zipzi_editor_core::ModelPoint { x: 128., y: 244. });
        window.click_at("canvas-body", point(px(p.x), px(p.y)), cx);
        let view = h.view.read(cx);
        assert_eq!(view.editor.document().unwrap().history_lengths(), (0, 0));
        assert!(
            view.editor.canvas_notice().is_some(),
            "A rejected create click must show a reason after the mouse is released"
        );
        let p = transform.to_viewport(zipzi_editor_core::ModelPoint { x: 128., y: 128. });
        window.click_at("canvas-body", point(px(p.x), px(p.y)), cx);
        assert_eq!(
            h.view.read(cx).editor.document().unwrap().history_lengths(),
            (1, 0)
        );
        assert!(h.view.read(cx).editor.canvas_notice().is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn manual_mesh_toolbar_only_shows_relevant_tools(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::alpha_document().unwrap();
    let h = setup_with_document(cx, doc, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("tool_manual_create").is_none());
        window.click("tool_manual_mesh", cx);
    })
    .unwrap();
    cx.simulate_prompt_answer("Use full image");
    present(&h, cx);
    cx.update_window(h.window, |_, window, _| {
        assert!(window.find("tool_select").visible());
        assert!(window.find("tool_lasso").visible());
        assert!(window.find("tool_glue").visible());
        assert!(window.find("mesh-edit-actions").visible());
        let secondary = window.find("mesh-edit-actions").bounds();
        let create = window.find("tool_manual_create").bounds();
        assert!(secondary.contains(&create.center()));
        assert!(window.find("tool_manual_create").visible());
        assert!(window.find("tool_manual_preview").visible());
    })
    .unwrap();
}

#[gpui_kit::test]
fn manual_mesh_entry_reports_unsupported_bindings(cx: &mut TestAppContext) {
    let (doc, assets) = document_fixture::rectangle_document(true).unwrap();
    let h = setup_with_document(cx, doc, assets);
    cx.update_window(h.window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
        window.click("tool_manual_mesh", cx);
        assert!(!h.view.read(cx).editor.manual_active());
        assert!(
            h.view.read(cx).edit_error.is_some(),
            "Unsupported targets need an explanation, not an inert button"
        );
    })
    .unwrap();
    assert!(!cx.has_pending_prompt());
}
