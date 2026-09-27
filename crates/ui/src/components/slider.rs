//! Stateful percentage slider with `SliderChanged` events.

use gpui::{prelude::*, *};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SliderChanged(pub f32);

/// A 0–100 slider; emits `SliderChanged` during pointer interaction.
pub struct Slider {
    value: f32,
    bounds: Bounds<Pixels>,
    dragging: bool,
}

impl EventEmitter<SliderChanged> for Slider {}

impl Slider {
    pub fn new(value: f32) -> Self {
        Self {
            value: value.clamp(0.0, 100.0),
            bounds: Bounds::default(),
            dragging: false,
        }
    }

    pub fn value(&self) -> f32 {
        self.value
    }

    pub fn set_value(&mut self, value: f32, cx: &mut Context<Self>) {
        let value = value.clamp(0.0, 100.0).round();
        if self.value != value {
            self.value = value;
            cx.emit(SliderChanged(value));
            cx.notify();
        }
    }

    fn slide(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let width = f32::from(self.bounds.size.width);
        if width > 0.0 {
            self.set_value(
                (f32::from(position.x - self.bounds.left()) / width) * 100.0,
                cx,
            );
        }
    }
}

impl Render for Slider {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let entity = cx.entity().downgrade();
        div()
            .id("slider")
            .relative()
            .h(px(28.0))
            .w_full()
            .cursor_pointer()
            .child(
                div()
                    .absolute()
                    .top(px(11.0))
                    .h(px(6.0))
                    .w_full()
                    .rounded_md()
                    .bg(rgb(0x101a29)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(11.0))
                    .h(px(6.0))
                    .w(relative(self.value / 100.0))
                    .rounded_md()
                    .bg(rgb(0x62cde6)),
            )
            .child(
                div()
                    .absolute()
                    .left(relative(self.value / 100.0))
                    .ml(px(-7.0))
                    .top(px(7.0))
                    .size(px(14.0))
                    .rounded_full()
                    .bg(rgb(0xe5f7ff)),
            )
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = entity.update(cx, |this, _| this.bounds = bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.dragging = true;
                    this.slide(event.position, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.dragging {
                    this.slide(event.position, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.dragging = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.dragging = false),
            )
    }
}
