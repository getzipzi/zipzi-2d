use super::{ColorField, ColorPickerEvent, ColorPickerState, color};
use gpui_kit::{
    AnyWindowHandle, AppContext as _, Context, Entity, InputEvent, IntoElement, ParentElement,
    Render, Rgba, Styled, Subscription, TestAppContext, Window, WindowOptions,
    component::button::Button, div, px, test::TestWindowExt,
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

struct Harness {
    state: Entity<ColorPickerState>,
    idle: Entity<IdleHost>,
    _event: Subscription,
}

struct IdleHost(Rc<Cell<usize>>);
impl Render for IdleHost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        self.0.set(self.0.get() + 1);
        div().size(px(10.))
    }
}

impl Render for Harness {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .p(px(20.))
            .w(px(360.))
            .flex()
            .flex_col()
            .gap(px(16.))
            .child(Button::new("outside").label("Outside"))
            .child(ColorField::new("test-color", &self.state))
            .child(
                self.idle
                    .clone()
                    .cached(gpui_kit::StyleRefinement::default().size(px(10.))),
            )
    }
}

struct TestPicker {
    window: AnyWindowHandle,
    state: Entity<ColorPickerState>,
    events: Rc<RefCell<Vec<ColorPickerEvent>>>,
    renders: Rc<Cell<usize>>,
}

fn setup(cx: &mut TestAppContext, color: Rgba, alpha: bool) -> TestPicker {
    let events = Rc::new(RefCell::new(vec![]));
    let renders = Rc::new(Cell::new(0));
    cx.update(gpui_kit::init);
    crate::i18n::activate(Some("en"));
    let (window, harness) = cx.update(|cx| {
        gpui_kit::open_window(WindowOptions::default(), cx, |window, cx| {
            cx.new(|cx| {
                let state = cx.new(|cx| ColorPickerState::new(color, alpha, window, cx));
                let log = events.clone();
                let event =
                    cx.subscribe(&state, move |_, _, event, _| log.borrow_mut().push(*event));
                Harness {
                    state,
                    idle: cx.new(|_| IdleHost(renders.clone())),
                    _event: event,
                }
            })
        })
        .unwrap()
    });
    let state = harness.read_with(cx, |harness, _| harness.state.clone());
    cx.update_window(window, |_, window, cx| {
        window.activate_window();
        window.render_frame(cx);
    })
    .unwrap();
    TestPicker {
        window,
        state,
        events,
        renders,
    }
}

#[gpui_kit::test]
fn swatch_tooltips_show_and_hide_without_rendering_the_host(cx: &mut TestAppContext) {
    use std::time::Duration;
    let p = setup(cx, gpui_kit::rgb(0x804020), false);
    cx.update(|cx| cx.set_reduce_motion(true));
    open(&p, cx);
    cx.run_until_parked();
    // Settle popover focus before observing tooltip-only work.
    cx.update_window(p.window, |_, window, cx| window.draw(cx).clear(cx))
        .unwrap();
    let before = p.renders.get();
    let events_before = p.events.borrow().len();
    for label in ["Original", "Current"] {
        cx.update_window(p.window, |_, window, cx| {
            let position = window
                .find(format!("color-picker-swatch-{label}"))
                .bounds()
                .center();
            window.dispatch_event(
                gpui_kit::MouseMoveEvent {
                    position,
                    pressed_button: None,
                    modifiers: window.modifiers(),
                }
                .to_platform_input(),
                cx,
            );
        })
        .unwrap();
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(600));
        cx.run_until_parked();
        cx.update_window(p.window, |_, window, cx| {
            window.draw(cx).clear(cx);
            assert!(window.try_find("tooltip").is_some());
        })
        .unwrap();
    }
    cx.update_window(p.window, |_, window, cx| {
        let position = window.find("color-picker-hex").bounds().center();
        window.dispatch_event(
            gpui_kit::MouseMoveEvent {
                position,
                pressed_button: None,
                modifiers: window.modifiers(),
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(600));
    cx.run_until_parked();
    cx.update_window(p.window, |_, window, cx| {
        window.draw(cx).clear(cx);
        assert!(window.try_find("tooltip").is_none());
    })
    .unwrap();
    assert_eq!(
        p.renders.get(),
        before,
        "tooltip refreshed its unrelated host"
    );
    assert_eq!(p.events.borrow().len(), events_before);
}

fn open(picker: &TestPicker, cx: &mut TestAppContext) {
    cx.update_window(picker.window, |_, window, cx| {
        window.click("test-color", cx);
        window.render_frame(cx);
        assert!(window.try_find("color-picker-panel").is_some());
    })
    .unwrap();
}

#[gpui_kit::test]
fn color_picker_retyping_displayed_hex_preserves_exact_color(cx: &mut TestAppContext) {
    let original = Rgba {
        r: 0.1234567,
        g: 0.4567891,
        b: 0.7654321,
        a: 1.,
    };
    let p = setup(cx, original, false);
    cx.update_window(p.window, |_, window, cx| {
        window.click("test-color-input", cx);
        window.press("cmd-a", cx);
        window.input(&color::hex(original, false).to_lowercase(), cx);
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(p.state.read_with(cx, |state, _| state.value()), original);
    assert!(
        !p.events
            .borrow()
            .iter()
            .any(|event| matches!(event, ColorPickerEvent::Preview(_)))
    );
}

#[gpui_kit::test]
fn color_picker_retyping_displayed_channels_preserves_exact_color(cx: &mut TestAppContext) {
    let original = Rgba {
        r: 0.1234567,
        g: 0.4567891,
        b: 0.7654321,
        a: 0.3456789,
    };
    let p = setup(cx, original, true);
    open(&p, cx);
    for mode in ["RGB", "HSV"] {
        cx.update_window(p.window, |_, window, cx| {
            window.click(mode, cx);
            window.render_frame(cx);
        })
        .unwrap();
        for channel in 0..4 {
            let text = p.state.read_with(cx, |state, cx| {
                state.inputs[channel + 2].read(cx).value().to_string()
            });
            cx.update_window(p.window, |_, window, cx| {
                window.click(format!("color-picker-channel-{channel}"), cx);
                window.press("cmd-a", cx);
                window.input(&format!("{text}0"), cx);
                window.render_frame(cx);
            })
            .unwrap();
        }
    }
    cx.update_window(p.window, |_, window, cx| {
        window.press("enter", cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(p.state.read_with(cx, |state, _| state.value()), original);
    assert!(
        !p.events
            .borrow()
            .iter()
            .any(|event| matches!(event, ColorPickerEvent::Preview(_)))
    );
}

#[gpui_kit::test]
fn color_picker_open_mode_switch_and_close_preserve_exact_rgb(cx: &mut TestAppContext) {
    let original = Rgba {
        r: 0.1234567,
        g: 0.4567891,
        b: 0.7654321,
        a: 1.,
    };
    let p = setup(cx, original, false);
    open(&p, cx);
    cx.update_window(p.window, |_, window, cx| {
        assert!(window.try_find("color-picker-alpha").is_none());
        window.click("HSV", cx);
        window.render_frame(cx);
        window.click("RGB", cx);
        window.click("color-picker-done", cx);
        window.render_frame(cx);
        assert!(window.try_find("color-picker-panel").is_none());
    })
    .unwrap();
    assert_eq!(p.state.read_with(cx, |state, _| state.value()), original);
    assert_eq!(
        *p.events.borrow(),
        [ColorPickerEvent::Begin, ColorPickerEvent::Commit(original)]
    );
    // External changes are quiet and keep their original precision too.
    cx.update_window(p.window, |_, window, cx| {
        p.state.update(cx, |state, cx| {
            state.sync(
                Rgba {
                    r: 0.7654321,
                    ..original
                },
                "Color",
                false,
                window,
                cx,
            )
        });
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(p.events.borrow().len(), 2);
}

#[gpui_kit::test]
fn color_picker_alpha_previews_and_escape_restores_the_whole_session(cx: &mut TestAppContext) {
    let original = Rgba {
        r: 0.3,
        g: 0.4,
        b: 0.9,
        a: 0.7,
    };
    let p = setup(cx, original, true);
    open(&p, cx);
    cx.update_window(p.window, |_, window, cx| {
        window.click("color-picker-alpha", cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert!((p.state.read_with(cx, |state, _| state.value().a) - 0.5).abs() < 0.01);
    cx.update_window(p.window, |_, window, cx| {
        window.press("escape", cx);
        window.render_frame(cx);
        assert!(window.try_find("color-picker-panel").is_none());
    })
    .unwrap();
    assert_eq!(p.state.read_with(cx, |state, _| state.value()), original);
    assert_eq!(p.events.borrow().last(), Some(&ColorPickerEvent::Cancel));
    assert!(
        !p.events
            .borrow()
            .iter()
            .any(|e| matches!(e, ColorPickerEvent::Commit(_)))
    );
}

#[gpui_kit::test]
fn color_picker_invalid_draft_never_mutates_color_and_outside_click_commits_once(
    cx: &mut TestAppContext,
) {
    let original = Rgba {
        r: 0.1234567,
        g: 0.4,
        b: 0.9,
        a: 1.,
    };
    let p = setup(cx, original, false);
    open(&p, cx);
    cx.update_window(p.window, |_, window, cx| {
        window.click("color-picker-hex", cx);
        window.press("cmd-a", cx);
        window.input("#12", cx);
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(p.state.read_with(cx, |state, _| state.value()), original);
    assert!(p.state.read_with(cx, |state, _| state.invalid[1]));
    cx.update_window(p.window, |_, window, cx| {
        window.press("cmd-a", cx);
        window.input("#36ADEF", cx);
        window.render_frame(cx);
        window.click("outside", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let value = p.state.read_with(cx, |state, _| state.value());
    assert_eq!(color::hex(value, false), "#36ADEF");
    assert_eq!(
        p.events
            .borrow()
            .iter()
            .filter(|e| matches!(e, ColorPickerEvent::Commit(_)))
            .count(),
        1
    );
}

#[gpui_kit::test]
fn color_picker_inline_hex_is_a_preview_until_enter_and_escape_keeps_precision(
    cx: &mut TestAppContext,
) {
    let original = Rgba {
        r: 0.1234567,
        g: 0.4,
        b: 0.9,
        a: 1.,
    };
    let p = setup(cx, original, false);
    for cancel in [true, false] {
        cx.update_window(p.window, |_, window, cx| {
            window.click("test-color-input", cx);
            window.press("cmd-a", cx);
            window.input("#F08040", cx);
            window.render_frame(cx);
        })
        .unwrap();
        assert_eq!(
            color::hex(p.state.read_with(cx, |state, _| state.value()), false),
            "#F08040"
        );
        cx.update_window(p.window, |_, window, cx| {
            window.press(if cancel { "escape" } else { "enter" }, cx);
            window.render_frame(cx);
        })
        .unwrap();
        if cancel {
            assert_eq!(p.state.read_with(cx, |state, _| state.value()), original);
        }
    }
    assert_eq!(
        p.events
            .borrow()
            .iter()
            .filter(|e| matches!(e, ColorPickerEvent::Commit(_)))
            .count(),
        1
    );
}

#[gpui_kit::test]
fn color_picker_hsv_numeric_edits_retain_hue_at_gray(cx: &mut TestAppContext) {
    let p = setup(
        cx,
        Rgba {
            r: 0.5,
            g: 0.5,
            b: 0.5,
            a: 1.,
        },
        false,
    );
    open(&p, cx);
    cx.update_window(p.window, |_, window, cx| {
        window.click("HSV", cx);
        window.render_frame(cx);
    })
    .unwrap();
    for (channel, text) in [(0, "240"), (1, "100"), (2, "80")] {
        cx.update_window(p.window, |_, window, cx| {
            window.click(format!("color-picker-channel-{channel}"), cx);
            window.press("cmd-a", cx);
            window.input(text, cx);
            window.render_frame(cx);
        })
        .unwrap();
    }
    let value = p.state.read_with(cx, |state, _| state.value());
    assert!(value.r < 0.000001 && value.g < 0.000001);
    assert!((value.b - 0.8).abs() < 0.000001);
    cx.update_window(p.window, |_, window, cx| window.press("enter", cx))
        .unwrap();
    assert_eq!(
        p.events.borrow().last(),
        Some(&ColorPickerEvent::Commit(value))
    );
}

#[gpui_kit::test]
fn color_picker_blur_settles_live_inline_text_before_queued_events(cx: &mut TestAppContext) {
    let p = setup(
        cx,
        Rgba {
            r: 0.1,
            g: 0.2,
            b: 0.3,
            a: 1.,
        },
        false,
    );
    cx.update_window(p.window, |_, window, cx| {
        window.click("test-color-input", cx);
        window.press("cmd-a", cx);
        window.input("#804020", cx);
        window.click("outside", cx);
        window.render_frame(cx);
    })
    .unwrap();
    let value = p.state.read_with(cx, |state, _| state.value());
    assert_eq!(color::hex(value, false), "#804020");
    assert_eq!(
        p.events.borrow().last(),
        Some(&ColorPickerEvent::Commit(value))
    );
}

#[gpui_kit::test]
fn color_picker_surface_drag_clamps_outside_and_escape_ignores_late_motion(
    cx: &mut TestAppContext,
) {
    use gpui_kit::{
        InputEvent as _, Modifiers, MouseButton, MouseDownEvent, MouseMoveEvent, point,
    };
    let original = Rgba {
        r: 0.1234567,
        g: 0.8,
        b: 0.3,
        a: 1.,
    };
    let p = setup(cx, original, false);
    open(&p, cx);
    let black = p.state.read_with(cx, |state, _| {
        super::wheel::Triangle::new(state.hsv.h).vertices[2]
    });
    let outside = cx
        .update_window(p.window, |_, window, cx| {
            let bounds = window.find("color-picker-sv").bounds();
            window.dispatch_event(
                MouseDownEvent {
                    button: MouseButton::Left,
                    position: bounds.center(),
                    modifiers: Modifiers::default(),
                    click_count: 1,
                    first_mouse: false,
                }
                .to_platform_input(),
                cx,
            );
            window.render_frame(cx);
            let wheel = window.find("color-picker-hue").bounds();
            point(
                wheel.left() + wheel.size.width * (0.5 + (black[0] - 0.5) * 4.),
                wheel.top() + wheel.size.height * (0.5 + (black[1] - 0.5) * 4.),
            )
        })
        .unwrap();
    cx.update_window(p.window, |_, window, cx| {
        window.dispatch_event(
            MouseMoveEvent {
                position: outside,
                pressed_button: Some(MouseButton::Left),
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
    assert_eq!(
        p.state.read_with(cx, |state, _| state.value()),
        Rgba {
            r: 0.,
            g: 0.,
            b: 0.,
            a: 1.
        }
    );
    cx.update_window(p.window, |_, window, cx| {
        window.press("escape", cx);
        window.render_frame(cx);
        window.dispatch_event(
            MouseMoveEvent {
                position: point(px(1.), px(1.)),
                pressed_button: Some(MouseButton::Left),
                modifiers: Modifiers::default(),
            }
            .to_platform_input(),
            cx,
        );
    })
    .unwrap();
    assert_eq!(p.state.read_with(cx, |state, _| state.value()), original);
    assert_eq!(p.events.borrow().last(), Some(&ColorPickerEvent::Cancel));
}

#[gpui_kit::test]
fn color_picker_triangle_handle_drags_from_outside_every_edge_and_vertex(cx: &mut TestAppContext) {
    use super::{Surface, color::Hsv, wheel};
    use gpui_kit::{InputEvent as _, Modifiers, MouseButton, MouseDownEvent, point};

    let original = Hsv {
        h: 0.2,
        s: 0.4,
        v: 0.7,
    }
    .rgb(1.);
    let p = setup(cx, original, false);
    open(&p, cx);
    for hue in [0., 0.17, 0.42] {
        for (s, v) in [
            (1., 1.),
            (0., 1.),
            (0.63, 0.),
            (0.5, 1.),
            (0., 0.5),
            (1., 0.5),
        ] {
            cx.update_window(p.window, |_, window, cx| {
                let hsv = Hsv { h: hue, s, v };
                let initial = hsv.rgb(1.);
                p.state.update(cx, |state, cx| {
                    state.hsv = hsv;
                    state.color = initial;
                    cx.notify();
                });
                window.render_frame(cx);
                let bounds = window.find("color-picker-hue").bounds();
                let triangle = wheel::Triangle::new(hue);
                let center = triangle.position(s, v);
                let direction = [center[0] - 0.5, center[1] - 0.5];
                let length = direction[0].hypot(direction[1]);
                // Within the visible 4px-radius handle, but outside the triangle.
                let distance = window.find("color-picker-sv-handle").bounds().size.width * 0.35;
                let from = point(
                    bounds.left()
                        + bounds.size.width * center[0]
                        + distance * direction[0] / length,
                    bounds.top()
                        + bounds.size.height * center[1]
                        + distance * direction[1] / length,
                );
                assert!(
                    wheel::hit(
                        [
                            (from.x - bounds.left()) / bounds.size.width,
                            (from.y - bounds.top()) / bounds.size.height,
                        ],
                        hue
                    )
                    .is_none()
                );
                window.dispatch_event(
                    MouseDownEvent {
                        button: MouseButton::Left,
                        position: from,
                        modifiers: Modifiers::default(),
                        click_count: 1,
                        first_mouse: false,
                    }
                    .to_platform_input(),
                    cx,
                );
                p.state.read_with(cx, |state, _| {
                    assert!(state.drag == Some(Surface::Sv), "hue={hue}, s={s}, v={v}");
                    assert_eq!(
                        state.value(),
                        initial,
                        "grabbing the handle changed the color"
                    );
                });
                let target = triangle.position(0.37, 0.71);
                window.drag(
                    from,
                    point(
                        bounds.left() + bounds.size.width * target[0],
                        bounds.top() + bounds.size.height * target[1],
                    ),
                    cx,
                );
                p.state.read_with(cx, |state, _| {
                    assert!((state.hsv.s - 0.37).abs() < 0.00001);
                    assert!((state.hsv.v - 0.71).abs() < 0.00001);
                    assert!((state.hsv.h - hue).abs() < 0.00001);
                    assert!(state.drag.is_none());
                });
            })
            .unwrap();
        }
    }
    cx.update_window(p.window, |_, window, cx| window.press("escape", cx))
        .unwrap();
    assert_eq!(p.state.read_with(cx, |state, _| state.value()), original);
}

#[gpui_kit::test]
fn color_picker_ring_and_triangle_edit_independently_in_the_narrow_vertical_panel(
    cx: &mut TestAppContext,
) {
    use super::{color::Hsv, wheel};
    use gpui_kit::point;
    let original = Hsv {
        h: 0.,
        s: 0.63,
        v: 0.74,
    }
    .rgb(1.);
    let p = setup(cx, original, false);
    open(&p, cx);
    cx.update_window(p.window, |_, window, cx| {
        let panel = window.find("color-picker-panel").bounds();
        let wheel = window.find("color-picker-hue").bounds();
        let top = wheel.top() - panel.top();
        assert!((wheel.left() - panel.left() - top).abs() <= px(1.));
        assert!((panel.right() - wheel.right() - top).abs() <= px(1.));
        let mut previous_bottom = panel.top();
        for i in 0..3 {
            let label = window
                .find(format!("color-picker-channel-label-{i}"))
                .bounds();
            let field = window.find(format!("color-picker-channel-{i}")).bounds();
            assert!(label.right() <= field.left());
            assert!((label.center().y - field.center().y).abs() <= px(2.));
            assert!(field.top() >= previous_bottom && field.right() <= panel.right());
            previous_bottom = field.bottom();
        }
        let bounds = window.find("color-picker-hue").bounds();
        let [x, y] = wheel::hue_position(0.125);
        // This ring segment overlaps the triangle's rectangular bounding box.
        // Shape hit testing must still select hue, rather than saturation/value.
        window.click_at(
            "color-picker-hue",
            point(bounds.size.width * x, bounds.size.height * y),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
    p.state.read_with(cx, |state, _| {
        assert!((state.hsv.h - 0.125).abs() < 0.00001);
        assert!((state.hsv.s - 0.63).abs() < 0.00001);
        assert!((state.hsv.v - 0.74).abs() < 0.00001);
    });
    cx.update_window(p.window, |_, window, cx| {
        let bounds = window.find("color-picker-hue").bounds();
        let [x, y] = wheel::Triangle::new(0.125).position(0.27, 0.83);
        window.click_at(
            "color-picker-hue",
            point(bounds.size.width * x, bounds.size.height * y),
            cx,
        );
        window.render_frame(cx);
    })
    .unwrap();
    p.state.read_with(cx, |state, _| {
        assert!((state.hsv.h - 0.125).abs() < 0.00001);
        assert!((state.hsv.s - 0.27).abs() < 0.00001);
        assert!((state.hsv.v - 0.83).abs() < 0.00001);
    });
    cx.update_window(p.window, |_, window, cx| window.press("escape", cx))
        .unwrap();
    assert_eq!(p.state.read_with(cx, |state, _| state.value()), original);
    assert!(
        !p.events
            .borrow()
            .iter()
            .any(|event| matches!(event, ColorPickerEvent::Commit(_)))
    );
}
