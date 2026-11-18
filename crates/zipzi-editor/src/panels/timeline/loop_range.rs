//! The upper half of the time ruler edits the session's loop range.
use super::*;
use zipzi_editor_core::{TimelineLoopRange, TimelineLoopRangeError};

pub(super) const LOOP_LANE_HEIGHT: f32 = 16.;

#[derive(Default)]
pub(super) struct LoopLane {
    bounds: Rc<Cell<Bounds<Pixels>>>,
    hover: Hit,
    error: Option<String>,
}

impl LoopLane {
    pub fn reset(&mut self) {
        self.hover = Hit::Empty;
        self.error = None;
    }
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Hit {
    #[default]
    Empty,
    Body,
    Start,
    End,
    Clear,
}

pub(super) struct Drag {
    hit: Hit,
    baseline: Option<TimelineLoopRange>,
    origin: Point<Pixels>,
    origin_time: f64,
    started: bool,
    candidate: Option<TimelineLoopRange>,
}

impl Drag {
    fn moved(&self, position: Point<Pixels>, scale: f32) -> bool {
        let threshold = if self.hit == Hit::Body { 3. } else { 0.5 };
        (position.x - self.origin.x).abs() >= px(threshold * scale)
    }

    fn endpoints(&self, time: f64) -> (f64, f64) {
        match (self.hit, self.baseline) {
            (Hit::Start, Some(range)) => (range.start + time - self.origin_time, range.end),
            (Hit::End, Some(range)) => (range.start, range.end + time - self.origin_time),
            _ => (self.origin_time.min(time), self.origin_time.max(time)),
        }
    }

    fn resizing(&self) -> bool {
        matches!(self.hit, Hit::Start | Hit::End)
    }
}

struct RangeGeometry {
    left: f32,
    right: f32,
    start_visible: bool,
    end_visible: bool,
    width: f32,
    scale: f32,
}

impl RangeGeometry {
    fn new(range: TimelineLoopRange, view: (f64, f64), width: f32, scale: f32) -> Option<Self> {
        let start = drawing::time_x(range.start, view, width);
        let end = drawing::time_x(range.end, view, width);
        if width <= 0. || start > width || end < 0. {
            return None;
        }
        let start_visible = start >= 0.;
        let end_visible = end <= width;
        let mut left = start.max(0.);
        let mut right = end.min(width);
        // Keep even a subpixel range clickable. Endpoint drags still apply a
        // delta to the actual times, independent of this minimum hit width.
        let minimum = (6. * scale).min(width);
        if right - left < minimum {
            left = ((left + right - minimum) / 2.).clamp(0., width - minimum);
            right = left + minimum;
        }
        Some(Self {
            left,
            right,
            start_visible,
            end_visible,
            width,
            scale,
        })
    }

    fn hit(&self, x: f32, show_clear: bool) -> Hit {
        let (clear_x, clear_width) = self.clear_button();
        if show_clear && (clear_x..=clear_x + clear_width).contains(&x) {
            return Hit::Clear;
        }
        let outside = 5. * self.scale;
        let inside = ((self.right - self.left) / 4.).min(3. * self.scale);
        if self.start_visible && (self.left - outside..=self.left + inside).contains(&x) {
            Hit::Start
        } else if self.end_visible && (self.right - inside..=self.right + outside).contains(&x) {
            Hit::End
        } else if (self.left..=self.right).contains(&x) {
            Hit::Body
        } else {
            Hit::Empty
        }
    }

    fn clear_button(&self) -> (f32, f32) {
        let width = (LOOP_LANE_HEIGHT * self.scale).min(self.width);
        let gap = 2. * self.scale;
        let right = self.right + gap;
        let left = self.left - gap - width;
        let x = if right + width <= self.width {
            right
        } else if left >= 0. {
            left
        } else {
            // A range may fill both sides of the viewport. Leave room at its
            // visible end for the handle and use the interior for the clear target.
            (self.right - 6. * self.scale - width).clamp(0., self.width - width)
        };
        (x, width)
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
    pub(super) fn loop_lane(
        &self,
        state: &TimelineViewState,
        fps: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let bounds_cell = self.loop_range.bounds.clone();
        let view = state.time_range;
        let selection = match &self.gesture {
            Some(input::Gesture::LoopRange(drag)) => drag.candidate.or(drag.baseline),
            _ => state.loop_range(),
        };
        let size = self.ui_scale;
        let hover = self.loop_range.hover;
        let error = self.loop_range.error.clone();
        let color = if error.is_some() {
            cx.theme().danger
        } else if selection.is_some_and(|selection| selection.enabled) {
            cx.theme().primary
        } else {
            cx.theme().muted_foreground
        };
        let creation = t!("timeline.loop_create");
        let hint = error
            .clone()
            .or_else(|| {
                let selection = selection?;
                let scale = if self.seconds { 1. } else { f64::from(fps) };
                let units = t!(if self.seconds {
                    "timeline.seconds_label"
                } else {
                    "timeline.frames_label"
                });
                let times = format!(
                    "[{}, {}) {units}",
                    axis::number(selection.start * scale),
                    axis::number(selection.end * scale)
                );
                let action = t!(if selection.enabled {
                    "timeline.loop_disable"
                } else {
                    "timeline.loop_enable"
                });
                Some(format!("{times} · {action}"))
            })
            .unwrap_or_else(|| creation.clone().into_owned());
        let target = self.target(cx);
        let clear = selection.and_then(|range| {
            if self.gesture.is_some() || hover == Hit::Empty || !self.allowed(window, cx) {
                return None;
            }
            let geometry = self.loop_geometry(state, range)?;
            let (x, width) = geometry.clear_button();
            let label = t!("timeline.loop_clear");
            let target = target.clone()?;
            Some(
                div()
                    .id("timeline-loop-clear")
                    .test_support()
                    .absolute()
                    .left(px(x))
                    .top(px(0.))
                    .w(px(width))
                    .h(compact::rem(LOOP_LANE_HEIGHT))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(cx.theme().background)
                    .text_color(cx.theme().foreground)
                    .text_xs()
                    .cursor(CursorStyle::PointingHand)
                    .role(Role::Button)
                    .aria_label(label.clone().into_owned())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if this.allowed(window, cx)
                            && this.target(cx).as_ref() == Some(&target)
                            && this.current_loop_range(cx) == Some(range)
                        {
                            this.loop_range.hover = Hit::Empty;
                            this.apply_loop_control(
                                cx,
                                zipzi_editor_core::Editor::clear_timeline_loop_range,
                            );
                        }
                    }))
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(label.clone().into_owned())
                            .build(window, cx)
                    })
                    .child("×"),
            )
        });

        div()
            .id("timeline-loop-lane")
            .test_support()
            .relative()
            .w_full()
            .h(compact::rem(LOOP_LANE_HEIGHT))
            .flex_shrink_0()
            .overflow_hidden()
            .bg(cx.theme().muted)
            .cursor(match hover {
                Hit::Body | Hit::Clear => CursorStyle::PointingHand,
                Hit::Start | Hit::End => CursorStyle::ResizeLeftRight,
                Hit::Empty => CursorStyle::Crosshair,
            })
            .on_mouse_down(MouseButton::Left, cx.listener(Self::start_loop_range))
            .tooltip(move |window, cx| {
                gpui_kit::component::tooltip::Tooltip::new(hint.clone()).build(window, cx)
            })
            .child(
                canvas(
                    move |bounds, _, _| bounds_cell.set(bounds),
                    move |bounds, _, window, _| {
                        let Some(selection) = selection else {
                            return;
                        };
                        let Some(geometry) =
                            RangeGeometry::new(selection, view, bounds.size.width.as_f32(), size)
                        else {
                            return;
                        };
                        let area = Bounds::new(
                            bounds.origin + point(px(geometry.left), px(2.)),
                            gpui_kit::size(
                                px(geometry.right - geometry.left),
                                bounds.size.height - px(4.),
                            ),
                        );
                        let style = if selection.enabled {
                            BorderStyle::Solid
                        } else {
                            BorderStyle::Dashed
                        };
                        let background = if selection.enabled {
                            color.opacity(0.36)
                        } else {
                            transparent_black()
                        };
                        window.paint_quad(quad(area, px(0.), background, px(1.), color, style));
                        for (visible, x) in [
                            (geometry.start_visible, area.left()),
                            (geometry.end_visible, area.right() - px(2.)),
                        ] {
                            if visible {
                                window.paint_quad(fill(
                                    Bounds::new(
                                        point(x, area.top()),
                                        gpui_kit::size(px(2.), area.size.height),
                                    ),
                                    color,
                                ));
                            }
                        }
                    },
                )
                .size_full(),
            )
            .when(selection.is_none() && error.is_none(), |lane| {
                lane.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .px_1()
                        .text_size(compact::rem(10.))
                        .text_color(cx.theme().muted_foreground)
                        .child(creation),
                )
            })
            .children(clear)
            .into_any_element()
    }

    fn current_loop_range(&self, cx: &App) -> Option<TimelineLoopRange> {
        self.owner
            .upgrade()?
            .read(cx)
            .editor
            .timeline_view()?
            .loop_range()
    }

    fn loop_geometry(
        &self,
        state: &TimelineViewState,
        range: TimelineLoopRange,
    ) -> Option<RangeGeometry> {
        RangeGeometry::new(
            range,
            state.time_range,
            self.loop_range.bounds.get().size.width.as_f32(),
            self.ui_scale,
        )
    }

    fn loop_hit(&self, state: &TimelineViewState, position: Point<Pixels>) -> Hit {
        let x = (position.x - self.loop_range.bounds.get().left()).as_f32();
        let show_clear = self.loop_range.hover != Hit::Empty;
        state
            .loop_range()
            .and_then(|range| self.loop_geometry(state, range))
            .map_or(Hit::Empty, |geometry| geometry.hit(x, show_clear))
    }

    pub(super) fn hover_loop_range(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The ruler and its clear target share one hover region. Keep tracking
        // from the window while moving between the two or leaving the ruler.
        let hit = if self.loop_range.bounds.get().contains(&position) && self.allowed(window, cx) {
            self.owner
                .upgrade()
                .and_then(|owner| owner.read(cx).editor.timeline_view())
                .map_or(Hit::Empty, |state| self.loop_hit(state, position))
        } else {
            Hit::Empty
        };
        if self.loop_range.hover != hit {
            self.loop_range.hover = hit;
            cx.notify();
        }
    }

    fn loop_pointer_time(&self, position: Point<Pixels>, state: &TimelineViewState) -> f64 {
        let bounds = self.loop_range.bounds.get();
        drawing::x_time(
            (position.x - bounds.left()).as_f32(),
            state.time_range,
            bounds.size.width.as_f32(),
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
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let Some(state) = owner.read(cx).editor.timeline_view() else {
            return;
        };
        let hit = self.loop_hit(state, event.position);
        if hit == Hit::Clear {
            return;
        }
        let drag = Drag {
            hit,
            baseline: state.loop_range(),
            origin: event.position,
            origin_time: self.loop_pointer_time(event.position, state),
            started: false,
            candidate: None,
        };
        window.focus(&self.focus, cx);
        self.gesture = Some(input::Gesture::LoopRange(drag));
        self.loop_range.error = None;
        self.sync_status_bar(cx);
        cx.notify();
        cx.stop_propagation();
    }

    fn pause_for_loop_drag(&self, cx: &mut Context<Self>) {
        let _ = self.owner.update(cx, |owner, cx| {
            if owner
                .editor
                .animation()
                .is_some_and(|animation| animation.playing)
            {
                owner.editor.pause_animation();
                owner.animation_anchor = None;
                owner.redraw(cx);
            }
        });
    }

    pub(super) fn move_loop_range(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(input::Gesture::LoopRange(drag)) = &self.gesture else {
            return;
        };
        if !drag.started {
            if !drag.moved(position, self.ui_scale) {
                return;
            }
            self.pause_for_loop_drag(cx);
        }
        let (Some(target), Some(owner)) = (self.target(cx), self.owner.upgrade()) else {
            return;
        };
        let editor = &owner.read(cx).editor;
        let Some(state) = editor.timeline_view() else {
            return;
        };
        let (start, end) = drag.endpoints(self.loop_pointer_time(position, state));
        let result = editor.timeline_loop_candidate(target.binding, start, end);
        self.loop_range.error = result.as_ref().err().map(range_error);
        if let Some(input::Gesture::LoopRange(drag)) = &mut self.gesture {
            drag.started = true;
            if let Ok(mut candidate) = result {
                if drag.resizing() {
                    candidate.enabled = drag.baseline.is_some_and(|range| range.enabled);
                }
                drag.candidate = Some(candidate);
            }
        }
        cx.notify();
    }

    pub(super) fn finish_loop_range(
        &mut self,
        drag: Drag,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let Some(state) = owner.read(cx).editor.timeline_view() else {
            return;
        };
        if state.loop_range() != drag.baseline {
            cx.notify();
            return;
        }
        let dx = (position.x - drag.origin.x).abs();
        if !drag.started && !drag.moved(position, self.ui_scale) {
            if drag.hit == Hit::Body
                && self.loop_range.bounds.get().contains(&position)
                && let Some(range) = drag.baseline
            {
                self.apply_loop_control(cx, |editor, binding| {
                    editor.enable_timeline_loop_range(binding, !range.enabled)
                });
            }
        } else if dx > px(0.) {
            let (start, end) = drag.endpoints(self.loop_pointer_time(position, state));
            self.apply_loop_control(cx, |editor, binding| {
                if drag.resizing() {
                    editor.resize_timeline_loop_range(binding, start, end)
                } else {
                    editor.set_timeline_loop_range(binding, start, end)
                }
            });
        } else {
            self.loop_range.error = None;
        }
        cx.notify();
    }

    fn apply_loop_control(
        &mut self,
        cx: &mut Context<Self>,
        apply: impl FnOnce(
            &mut zipzi_editor_core::Editor,
            zipzi_editor_core::TimelineBinding,
        ) -> anyhow::Result<()>,
    ) {
        let Some(target) = self.target(cx) else {
            return;
        };
        let result = self.owner.update(cx, |owner, cx| {
            let was_range = owner.editor.animation().unwrap().is_range_playing();
            let result = apply(&mut owner.editor, target.binding);
            if result.is_ok() {
                let playback = owner.editor.animation().unwrap();
                if was_range != playback.is_range_playing() || !playback.playing {
                    owner.animation_anchor = None;
                }
                owner.redraw(cx);
            }
            result
        });
        self.loop_range.error = result
            .as_ref()
            .ok()
            .and_then(|r| r.as_ref().err().map(range_error));
        cx.notify();
    }
}
