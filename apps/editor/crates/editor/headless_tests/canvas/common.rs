use gpui_kit::test::TestWindowExt;
use gpui_kit::{
    AppContext as _, InputEvent as _, ScrollDelta, ScrollWheelEvent, TestAppContext, point,
};

pub(super) fn advance_zoom(
    harness: &super::super::Harness,
    cx: &mut TestAppContext,
    milliseconds: u64,
) {
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(milliseconds));
    cx.update_window(harness.window, |_, window, cx| {
        window.simulate_next_frame(cx);
        window.render_frame(cx);
    })
    .unwrap();
}

pub(super) fn wheel(
    harness: &super::super::Harness,
    cx: &mut TestAppContext,
    position: gpui_kit::Point<gpui_kit::Pixels>,
    lines: f32,
) {
    cx.update_window(harness.window, |_, window, cx| {
        window.dispatch_event(
            ScrollWheelEvent {
                position,
                delta: ScrollDelta::Lines(point(0., lines)),
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
}

pub(super) fn navigation_view(
    harness: &super::super::Harness,
    cx: &TestAppContext,
) -> editor_core::ViewState {
    harness.view.read_with(cx, |view, _| view.editor.view())
}

pub(super) fn present_canvas(harness: &super::super::Harness, cx: &mut TestAppContext) {
    cx.update_window(harness.window, |_, window, cx| {
        harness.view.update(cx, |view, cx| {
            let frame = view.editor.preview().unwrap().canvas.unwrap();
            let device = metal::Device::system_default().unwrap();
            let descriptor = metal::TextureDescriptor::new();
            descriptor.set_width(1);
            descriptor.set_height(1);
            descriptor.set_pixel_format(metal::MTLPixelFormat::RGBA8Unorm);
            let image = device.new_texture(&descriptor);
            assert!(view.editor.present_frame(frame.clone()));
            view.viewport_frames
                .entry(view.viewport_slots[0])
                .or_default()
                .modeling = Some(crate::preview::Presented {
                image,
                canvas: frame,
            });
            cx.notify();
        });
        window.render_frame(cx);
    })
    .unwrap();
}
