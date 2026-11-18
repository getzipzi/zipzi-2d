//! Range ruler and transport controls. Pointer candidates live in the panel
//! until release, so cancelling or losing the binding leaves the range intact.
use super::*;
use gpui_kit::component::input::Input;
use zipzi_editor_core::{TimelineLoopRange, TimelineLoopRangeError};

pub(super) const STRIP_HEIGHT: f32 = 12.;

#[derive(Default)]
pub(super) struct Controls {
    pub open: bool,
    bounds: Rc<Cell<Bounds<Pixels>>>,
    fields: Option<Fields>,
    error: Option<String>,
}

impl Controls {
    pub fn reset(&mut self) {
        self.open = false;
        self.fields = None;
        self.error = None;
    }

    pub fn clear_error(&mut self) {
        self.error = None;
    }
}

struct Fields {
    inputs: [Entity<InputState>; 2],
    displayed: [String; 2],
    _events: Vec<Subscription>,
}

enum Edge {
    New,
    Start,
    End,
}

pub(super) struct Drag {
    edge: Edge,
    baseline: Option<TimelineLoopRange>,
    origin: Point<Pixels>,
    origin_time: f64,
    candidate: Option<TimelineLoopRange>,
}

impl Drag {
    fn endpoints(&self, position: Point<Pixels>, time: f64) -> (f64, f64) {
        match (&self.edge, self.baseline) {
            (Edge::Start, Some(range)) => (
                if position.x == self.origin.x {
                    range.start
                } else {
                    range.start + time - self.origin_time
                },
                range.end,
            ),
            (Edge::End, Some(range)) => (
                range.start,
                if position.x == self.origin.x {
                    range.end
                } else {
                    range.end + time - self.origin_time
                },
            ),
            _ => (self.origin_time.min(time), self.origin_time.max(time)),
        }
    }
}

fn range_error(error: &anyhow::Error) -> String {
    match error.downcast_ref::<TimelineLoopRangeError>() {
        Some(TimelineLoopRangeError::Outside) => t!("timeline.loop_outside").into_owned(),
        Some(TimelineLoopRangeError::Order) => t!("timeline.loop_order").into_owned(),
        None => error.to_string(),
    }
}

impl Timeline {
    pub(super) fn toggle_loop_controls(&mut self, cx: &mut Context<Self>) {
        self.loop_range.open = !self.loop_range.open;
        self.loop_range.fields = None;
        self.loop_range.error = None;
        cx.notify();
    }

    pub(super) fn loop_strip(
        &self,
        state: &TimelineViewState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bounds_cell = self.loop_range.bounds.clone();
        let range = state.time_range;
        let selection = match &self.gesture {
            Some(input::Gesture::LoopRange(drag)) => drag.candidate.or(drag.baseline),
            _ => state.loop_range(),
        };
        let background = cx.theme().muted;
        let border = cx.theme().border;
        let error = self.loop_range.error.clone();
        let color = if error.is_some() {
            cx.theme().danger
        } else if selection.is_some_and(|selection| selection.enabled) {
            cx.theme().primary
        } else {
            cx.theme().muted_foreground
        };
        div()
            .id("timeline-loop-strip")
            .test_support()
            .w_full()
            .h(compact::rem(STRIP_HEIGHT))
            .flex_shrink_0()
            .overflow_hidden()
            .bg(background)
            .border_b_1()
            .border_color(border)
            .cursor(CursorStyle::ResizeLeftRight)
            .on_mouse_down(MouseButton::Left, cx.listener(Self::start_loop_range))
            .on_scroll_wheel(cx.listener(Self::wheel))
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(error.clone().unwrap_or_else(|| {
                    format!(
                        "{} · {}",
                        t!("timeline.loop_range"),
                        t!("timeline.loop_exclusive")
                    )
                }))
                .build(window, cx)
            })
            .child(
                canvas(
                    move |bounds, _, _| bounds_cell.set(bounds),
                    move |bounds, _, window, _| {
                        let Some(selection) = selection else { return };
                        let width = bounds.size.width.as_f32();
                        let start = drawing::time_x(selection.start, range, width);
                        let end = drawing::time_x(selection.end, range, width);
                        if start > width || end < 0. {
                            return;
                        }
                        let left = start.max(0.);
                        let right = end.min(width);
                        window.paint_quad(fill(
                            Bounds::new(
                                bounds.origin + point(px(left), px(1.)),
                                size(px(right - left), bounds.size.height - px(2.)),
                            ),
                            color.opacity(0.32),
                        ));
                        for endpoint in [start, end] {
                            if (0. ..=width).contains(&endpoint) {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        bounds.origin + point(px((endpoint - 2.).max(0.)), px(1.)),
                                        size(px(4.), bounds.size.height - px(2.)),
                                    ),
                                    color,
                                ));
                            }
                        }
                    },
                )
                .size_full(),
            )
            .into_any_element()
    }

    fn loop_pointer_time(&self, position: Point<Pixels>, state: &TimelineViewState) -> f64 {
        let bounds = self.loop_range.bounds.get();
        drawing::x_time(
            (position.x - bounds.left()).as_f32(),
            state.time_range,
            bounds.size.width.as_f32().max(1.),
        )
    }

    fn start_loop_range(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.allowed(window, cx) || self.gesture.is_some() {
            return;
        }
        self.finish_number(cx);
        window.focus(&self.focus, cx);
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let Some(state) = owner.read(cx).editor.timeline_view() else {
            return;
        };
        let baseline = state.loop_range();
        let time = self.loop_pointer_time(event.position, state);
        let mut edge = Edge::New;
        if let Some(range) = baseline {
            let bounds = self.loop_range.bounds.get();
            let width = bounds.size.width.as_f32();
            let x = (event.position.x - bounds.left()).as_f32();
            let start = (drawing::time_x(range.start, state.time_range, width) - x).abs();
            let end = (drawing::time_x(range.end, state.time_range, width) - x).abs();
            if start.min(end) <= 6. * self.ui_scale {
                edge = if start <= end { Edge::Start } else { Edge::End };
            }
        }
        self.gesture = Some(input::Gesture::LoopRange(Drag {
            edge,
            baseline,
            origin: event.position,
            origin_time: time,
            candidate: None,
        }));
        self.loop_range.clear_error();
        self.pause_for_loop_range(cx);
        self.sync_status_bar(cx);
        cx.notify();
        cx.stop_propagation();
    }

    fn pause_for_loop_range(&self, cx: &mut Context<Self>) {
        let _ = self.owner.update(cx, |owner, cx| {
            owner.editor.pause_animation();
            owner.animation_anchor = None;
            owner.redraw(cx);
        });
    }

    pub(super) fn move_loop_range(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(input::Gesture::LoopRange(drag)) = &self.gesture else {
            return;
        };
        let (Some(target), Some(owner)) = (self.target(cx), self.owner.upgrade()) else {
            return;
        };
        let editor = &owner.read(cx).editor;
        let Some(state) = editor.timeline_view() else {
            return;
        };
        let (start, end) = drag.endpoints(position, self.loop_pointer_time(position, state));
        let result = editor.timeline_loop_candidate(target.binding, start, end);
        self.loop_range.error = result.as_ref().err().map(range_error);
        if let (Ok(candidate), Some(input::Gesture::LoopRange(drag))) = (result, &mut self.gesture)
        {
            drag.candidate = Some(candidate);
        }
        cx.notify();
    }

    pub(super) fn finish_loop_range(
        &mut self,
        drag: Drag,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        if drag.origin.x == position.x {
            self.loop_range.clear_error();
            cx.notify();
            return;
        }
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let Some(state) = owner.read(cx).editor.timeline_view() else {
            return;
        };
        let (start, end) = drag.endpoints(position, self.loop_pointer_time(position, state));
        self.apply_loop_control(cx, move |editor, binding| {
            editor.set_timeline_loop_range(binding, start, end)
        });
    }

    fn apply_loop_control(
        &mut self,
        cx: &mut Context<Self>,
        apply: impl FnOnce(
            &mut zipzi_editor_core::Editor,
            zipzi_editor_core::TimelineBinding,
        ) -> anyhow::Result<()>,
    ) -> bool {
        let Some(target) = self.target(cx) else {
            return false;
        };
        let changed = self.owner.update(cx, |owner, cx| {
            let result = apply(&mut owner.editor, target.binding);
            if result.is_ok() {
                owner.animation_anchor = None;
                owner.redraw(cx);
            }
            result
        });
        self.loop_range.error = changed
            .as_ref()
            .ok()
            .and_then(|r| r.as_ref().err().map(range_error));
        cx.notify();
        matches!(changed, Ok(Ok(())))
    }

    pub(super) fn apply_loop_input(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(fields) = &self.loop_range.fields else {
            return;
        };
        let input = fields.inputs[index].read(cx).value();
        if input.as_ref() == fields.displayed[index] {
            return;
        }
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let editor = &owner.read(cx).editor;
        let Some(projection) = editor.timeline_projection() else {
            return;
        };
        let current = editor.timeline_view().and_then(|view| view.loop_range());
        let divisor = if self.seconds {
            1.
        } else {
            f64::from(projection.fps)
        };
        let time = input.trim().parse::<f64>().unwrap_or(f64::NAN) / divisor;
        let (start, end) = if index == 0 {
            (time, current.map_or(projection.duration, |range| range.end))
        } else {
            (current.map_or(0., |range| range.start), time)
        };
        self.apply_loop_control(cx, move |editor, binding| {
            editor.set_timeline_loop_range(binding, start, end)
        });
    }

    pub(super) fn focused_loop_input(&self, window: &Window, cx: &App) -> Option<usize> {
        self.loop_range
            .fields
            .as_ref()?
            .inputs
            .iter()
            .position(|input| input.read(cx).focus_handle(cx).is_focused(window))
    }

    pub(super) fn cancel_loop_input(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(fields) = &self.loop_range.fields else {
            return false;
        };
        for (input, displayed) in fields.inputs.iter().zip(&fields.displayed) {
            if input.read(cx).focus_handle(cx).is_focused(window) {
                input.update(cx, |input, cx| {
                    input.set_value(displayed.clone(), window, cx)
                });
                self.loop_range.clear_error();
                window.focus(&self.focus, cx);
                cx.notify();
                return true;
            }
        }
        false
    }

    pub(super) fn render_loop_controls(
        &mut self,
        projection: &TimelineProjection,
        state: &TimelineViewState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.loop_range.open {
            return None;
        }
        let range = state.loop_range();
        let allowed = self.allowed(window, cx);
        let scale = if self.seconds {
            1.
        } else {
            f64::from(projection.fps)
        };
        let displayed = [
            axis::number(range.map_or(0., |r| r.start) * scale),
            axis::number(range.map_or(projection.duration, |r| r.end) * scale),
        ];
        let fields = self.loop_range.fields.get_or_insert_with(|| {
            let inputs = displayed
                .each_ref()
                .map(|value| cx.new(|cx| InputState::new(window, cx).default_value(value.clone())));
            let subscriptions = inputs
                .iter()
                .enumerate()
                .map(|(index, input)| {
                    cx.subscribe_in(
                        input,
                        window,
                        move |this, source, event: &InputEvent, window, cx| {
                            if !this.allowed(window, cx)
                                || this
                                    .loop_range
                                    .fields
                                    .as_ref()
                                    .is_none_or(|fields| &fields.inputs[index] != source)
                            {
                                return;
                            }
                            match event {
                                InputEvent::Focus => this.pause_for_loop_range(cx),
                                InputEvent::Blur | InputEvent::PressEnter { .. } => {
                                    this.apply_loop_input(index, cx)
                                }
                                _ => {}
                            }
                        },
                    )
                })
                .collect();
            Fields {
                inputs,
                displayed: displayed.clone(),
                _events: subscriptions,
            }
        });
        for (index, value) in displayed.into_iter().enumerate() {
            if fields.displayed[index] != value {
                fields.displayed[index] = value.clone();
                fields.inputs[index].update(cx, |input, cx| input.set_value(value, window, cx));
            }
        }
        let inputs = fields.inputs.clone();
        let enabled = range.is_some_and(|range| range.enabled);
        let toggle = t!("timeline.loop_range");
        let clear = t!("timeline.loop_clear");
        let mut row = div()
            .id("timeline-loop-controls")
            .test_support()
            .flex()
            .flex_wrap()
            .items_center()
            .flex_shrink_0()
            .gap_1()
            .px_1()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .child(
                compact::header_button(Button::new("timeline-loop-toggle"))
                    .icon(IconName::Repeat)
                    .selected(enabled)
                    .toggled(enabled)
                    .disabled(!allowed || range.is_none())
                    .accessibility_label(toggle.clone().into_owned())
                    .tooltip(toggle)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if this.allowed(window, cx) {
                            this.apply_loop_control(cx, move |editor, binding| {
                                editor.enable_timeline_loop_range(binding, !enabled)
                            });
                        }
                    })),
            )
            .child(
                compact::header_button(Button::new("timeline-loop-clear"))
                    .icon(IconName::X)
                    .disabled(!allowed || range.is_none())
                    .accessibility_label(clear.clone().into_owned())
                    .tooltip(clear)
                    .on_click(cx.listener(|this, _, window, cx| {
                        if this.allowed(window, cx) {
                            this.apply_loop_control(
                                cx,
                                zipzi_editor_core::Editor::clear_timeline_loop_range,
                            );
                        }
                    })),
            );
        for (index, (id, label)) in [
            ("timeline-loop-start", t!("timeline.loop_start")),
            ("timeline-loop-end", t!("timeline.loop_end")),
        ]
        .into_iter()
        .enumerate()
        {
            let units = t!(if self.seconds {
                "timeline.seconds_label"
            } else {
                "timeline.frames_label"
            });
            row = row.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .child(label.clone())
                    .child(
                        div().id(id).test_support().w(compact::rem(65.)).child(
                            compact::header_input(Input::new(&inputs[index]))
                                .aria_label(format!("{label} ({units})"))
                                .disabled(!allowed),
                        ),
                    ),
            );
        }
        if let Some(error) = &self.loop_range.error {
            row = row.child(
                div()
                    .id("timeline-loop-error")
                    .test_support()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        Some(row.into_any_element())
    }
}
