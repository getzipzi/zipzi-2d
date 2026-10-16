use std::time::Duration;

use crate::ui::compact;
use gpui_kit::component::{button::Button, tooltip::TooltipDefaults};
use gpui_kit::{
    AnyWindowHandle, AppContext, Context, IntoElement, Render, TestAppContext, TestSupportExt,
    Window, WindowOptions, div, prelude::*, px, test::TestWindowExt,
};

struct TooltipHarness;

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
