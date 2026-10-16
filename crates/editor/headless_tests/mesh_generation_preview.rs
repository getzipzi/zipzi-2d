use crate::ui::mesh_generation_preview::{MeshGenerationPreview, PreviewContent, PreviewPainter};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Bounds, Entity, InputEvent as _, PinchEvent, Pixels,
    TestAppContext, WindowBounds, WindowOptions, point, px, size, test::TestWindowExt,
};
use std::{cell::RefCell, rc::Rc};

type Painted = Rc<RefCell<Option<(Bounds<Pixels>, Bounds<Pixels>, f32)>>>;

fn setup(cx: &mut TestAppContext) -> (AnyWindowHandle, Entity<MeshGenerationPreview>, Painted) {
    let _editor = super::setup(cx);
    let painted: Painted = Rc::new(RefCell::new(None));
    let captured = painted.clone();
    let painter: PreviewPainter = Rc::new(RefCell::new(
        move |bounds, content, window: &mut gpui_kit::Window| {
            *captured.borrow_mut() = Some((bounds, content, window.scale_factor()));
            Ok(())
        },
    ));
    let (window, viewport) = cx.update(|cx| {
        gpui_kit::open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(800.), px(600.)),
                ))),
                ..Default::default()
            },
            cx,
            |_, cx| cx.new(MeshGenerationPreview::new),
        )
        .unwrap()
    });
    viewport.update(cx, |viewport, cx| {
        viewport.set_content(
            Some(PreviewContent {
                source_size: [480., 320.],
                painter: Some(painter),
                request: 1,
                edges: vec![].into(),
            }),
            "".into(),
            cx,
        );
    });
    (window, viewport, painted)
}

#[gpui_kit::test]
fn actual_pixel_size_survives_viewport_resize_and_display_changes(cx: &mut TestAppContext) {
    let (window, _viewport, painted) = setup(cx);
    for dpi in [1., 1.5, 2., 3.] {
        cx.simulate_window_scale_factor_change(window, dpi);
        for (width, height) in [(800., 600.), (500., 360.)] {
            cx.simulate_window_resize(window, size(px(width), px(height)));
            cx.update_window(window, |_, window, cx| window.render_frame(cx))
                .unwrap();
            let (bounds, content, scale) = painted.borrow().unwrap();
            assert!((content.size.width.as_f32() * scale - 480.).abs() < 0.001);
            assert!((content.size.height.as_f32() * scale - 320.).abs() < 0.001);
            assert_eq!(content.center(), bounds.center());
        }
    }
}

#[gpui_kit::test]
fn fit_uses_actual_zoom_and_pinch_keeps_its_pointer_anchor(cx: &mut TestAppContext) {
    let (window, _viewport, painted) = setup(cx);
    cx.update_window(window, |_, window, cx| {
        window.render_frame(cx);
        window.click("preview-fit", cx);
        let (bounds, fitted, dpi) = painted.borrow().unwrap();
        let fit = ((bounds.size.width.as_f32() - 32.) / 480.)
            .min((bounds.size.height.as_f32() - 32.) / 320.)
            * dpi;
        assert!((fitted.size.width.as_f32() * dpi - 480. * fit).abs() < 0.001);
        assert!((fitted.size.height.as_f32() * dpi - 320. * fit).abs() < 0.001);
        assert_eq!(fitted.center(), bounds.center());

        let anchor = bounds.center() + point(px(30.), px(-20.));
        let normalized = point(
            (anchor.x - fitted.origin.x) / fitted.size.width,
            (anchor.y - fitted.origin.y) / fitted.size.height,
        );
        window.dispatch_event(
            PinchEvent {
                position: anchor,
                delta: 0.25,
                ..Default::default()
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
        let (_, zoomed, _) = painted.borrow().unwrap();
        assert!((zoomed.size.width / fitted.size.width - 1.25).abs() < 0.001);
        let after = zoomed.origin
            + point(
                zoomed.size.width * normalized.x,
                zoomed.size.height * normalized.y,
            );
        assert!((after.x - anchor.x).as_f32().abs() < 0.001);
        assert!((after.y - anchor.y).as_f32().abs() < 0.001);
        window.click("preview-fit", cx);
        assert_eq!(painted.borrow().unwrap().1, fitted);
    })
    .unwrap();
}
