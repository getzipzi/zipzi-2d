use std::time::Duration;

use crate::ui::compact;
use gpui_kit::component::{button::Button, tooltip::TooltipDefaults};
use gpui_kit::{
    AnyWindowHandle, AppContext, Context, IntoElement, Render, TestAppContext, TestSupportExt,
    Window, WindowOptions, div, prelude::*, px, test::TestWindowExt,
};

struct TooltipHarness;

struct LongTooltipHarness(gpui_kit::SharedString);

impl Render for LongTooltipHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().p(px(20.)).child(compact::button(
            Button::new("long-tooltip")
                .label("Help")
                .tooltip(self.0.clone()),
        ))
    }
}

#[gpui_kit::test]
fn long_editor_tooltips_wrap_inside_narrow_windows(cx: &mut TestAppContext) {
    use gpui_kit::{Bounds, WindowBounds, point, size};
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
        TooltipDefaults::new()
            .with_show_delay(Duration::from_millis(100))
            .install(cx);
    });
    for (locale, source) in [
        ("en", include_str!("../assets/locales/en.toml")),
        ("zh-CN", include_str!("../assets/locales/zh-CN.toml")),
    ] {
        let strings: toml::Value = toml::from_str(source).unwrap();
        for key in ["manual_mesh_edit_hint", "deformer_gesture_hint"] {
            for scale in [1., 1.5] {
                let mut wide_height = None;
                for width in [420., 240.] {
                    let text = strings["editor"][key].as_str().unwrap().to_owned();
                    let (handle, _) = cx.update(|cx| {
                        gpui_kit::component::Theme::update(cx, |theme| {
                            theme.font_size = px(16. * scale);
                        });
                        gpui_kit::open_window(
                            WindowOptions {
                                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                                    point(px(0.), px(0.)),
                                    size(px(width), px(600.)),
                                ))),
                                ..Default::default()
                            },
                            cx,
                            |window, cx| {
                                window.set_rem_size(px(16. * scale));
                                cx.new(|_| LongTooltipHarness(text.into()))
                            },
                        )
                        .unwrap()
                    });
                    cx.update_window(handle, |_, window, cx| {
                        window.activate_window();
                        window.render_frame(cx);
                    })
                    .unwrap();
                    cx.run_until_parked();
                    cx.update_window(handle, |_, window, cx| window.hover("long-tooltip", cx))
                        .unwrap();
                    advance(cx, 250);
                    cx.update_window(handle, |_, window, cx| {
                    window.render_frame(cx);
                    let bounds = window.find("tooltip").bounds();
                    if key == "manual_mesh_edit_hint" {
                        assert!(bounds.size.height > px(70. * scale),
                            "{locale} {key} at {scale}: {bounds:?}");
                    }
                    if let Some(height) = wide_height {
                        assert!(if locale == "en" { bounds.size.height > height } else { bounds.size.height >= height },
                            "{locale} {key} must preserve all lines in a narrower window at {scale}: {bounds:?}, wide height {height:?}");
                    } else {
                        wide_height = Some(bounds.size.height);
                    }
                    assert!(
                        bounds.origin.x >= px(0.) && bounds.right() <= window.viewport_size().width,
                        "{key}: {bounds:?}"
                    );
                    assert!(
                        bounds.origin.y >= px(0.)
                            && bounds.bottom() <= window.viewport_size().height,
                        "{key}: {bounds:?}"
                    );
                    window.remove_window();
                })
                .unwrap();
                }
            }
        }
    }
}

impl Render for TooltipHarness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .p(px(64.))
            .flex()
            .gap(px(8.))
            .child(compact::button(
                Button::new("first").label("First").tooltip("First tooltip"),
            ))
            .child(compact::button(
                Button::new("second")
                    .label("Second")
                    .tooltip("Second tooltip"),
            ))
            .child(div().id("outside").test_support().size(px(40.)))
    }
}

fn setup(cx: &mut TestAppContext) -> AnyWindowHandle {
    cx.update(|cx| {
        gpui_kit::init(cx);
        cx.set_reduce_motion(true);
        TooltipDefaults::new()
            .with_show_delay(Duration::from_millis(100))
            .with_grace_period(Duration::from_millis(300))
            .install(cx);
    });
    let (handle, _) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |_, cx| {
            cx.new(|_| TooltipHarness)
        })
        .unwrap()
    });
    cx.update_window(handle, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
    })
    .unwrap();
    cx.run_until_parked();
    handle
}

fn advance(cx: &mut TestAppContext, milliseconds: u64) {
    cx.run_until_parked();
    cx.executor()
        .advance_clock(Duration::from_millis(milliseconds));
    cx.run_until_parked();
}

#[gpui_kit::test]
fn switching_buttons_keeps_the_new_tooltip_until_its_own_trigger_is_left(cx: &mut TestAppContext) {
    let handle = setup(cx);
    cx.update_window(handle, |_, window, cx| window.hover("first", cx))
        .unwrap();
    advance(cx, 100);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("tooltip").is_some());
    })
    .unwrap();

    // Exercise both mouse-event orders and repeat after every button has rerendered.
    for target in ["second", "first", "second", "first"] {
        cx.update_window(handle, |_, window, cx| {
            window.hover(target, cx);
            assert!(
                window.try_find("tooltip").is_some(),
                "switch to {target} must be immediate"
            );
        })
        .unwrap();
        advance(cx, 600);
        cx.update_window(handle, |_, window, cx| {
            window.render_frame(cx);
            let tooltip = window.try_find("tooltip").unwrap_or_else(|| {
                panic!("tooltip for {target} disappeared while its button was still hovered")
            });
            let trigger = window.find(target).bounds();
            assert!((tooltip.bounds().center().x - trigger.center().x).abs() <= px(1.));
        })
        .unwrap();
    }

    cx.update_window(handle, |_, window, cx| window.hover("outside", cx))
        .unwrap();
    advance(cx, 299);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("tooltip").is_some());
    })
    .unwrap();
    advance(cx, 1);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("tooltip").is_none());
    })
    .unwrap();
}

#[gpui_kit::test]
fn switching_before_show_delay_keeps_the_new_request_and_click_dismisses_it(
    cx: &mut TestAppContext,
) {
    let handle = setup(cx);
    cx.update_window(handle, |_, window, cx| window.hover("first", cx))
        .unwrap();
    advance(cx, 50);
    cx.update_window(handle, |_, window, cx| window.hover("second", cx))
        .unwrap();
    advance(cx, 99);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("tooltip").is_none());
    })
    .unwrap();
    advance(cx, 1);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("tooltip").is_some());
    })
    .unwrap();
    advance(cx, 600);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("tooltip").is_some());
        window.click("second", cx);
        assert!(window.try_find("tooltip").is_none());
        window.hover("outside", cx);
        window.hover("first", cx);
    })
    .unwrap();
    advance(cx, 50);
    cx.update_window(handle, |_, window, cx| window.hover("outside", cx))
        .unwrap();
    advance(cx, 600);
    cx.update_window(handle, |_, window, cx| {
        window.render_frame(cx);
        assert!(window.try_find("tooltip").is_none());
    })
    .unwrap();
}
