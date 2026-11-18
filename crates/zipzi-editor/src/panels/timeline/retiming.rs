//! Explicit selection retiming. Both input paths use the same Draft command.
use super::*;
use zipzi_editor_core::{MotionRetimeFixedEnd as Fixed, TimelineRetimeSelection};

pub(super) const HANDLE_LANE: f32 = 16.;

#[derive(Default)]
pub(super) struct Controls {
    pub open: bool,
    pub fixed_end: Option<Fixed>,
}

pub(super) struct Drag {
    pub token: TimelineCandidateIdentity,
    selection: TimelineRetimeSelection,
    fixed: Fixed,
    origin_x: Pixels,
    origin_time: f64,
}

impl Timeline {
    pub(super) fn retime_baseline(&self, cx: &App) -> Option<TimelineRetimeSelection> {
        let owner = self.owner.upgrade()?;
        let editor = &owner.read(cx).editor;
        editor
            .timeline_baseline_projection()?
            .retime_selection(editor.timeline_view()?)
    }

    pub(super) fn retime_fixed_end(&self) -> Fixed {
        self.retime.fixed_end.unwrap_or(Fixed::Earliest)
    }

    pub(super) fn toggle_retime_controls(&mut self, cx: &mut Context<Self>) {
        self.retime.open = !self.retime.open;
        self.error = None;
        cx.notify();
    }

    pub(super) fn retime_controls(
        &mut self,
        projection: &TimelineProjection,
        state: &TimelineViewState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.retime.open {
            return None;
        }
        let enabled = self.editable(window, cx);
        let selection = self.retime_baseline(cx);
        let mut row = div()
            .id("timeline-retime-controls")
            .test_support()
            .flex()
            .flex_wrap()
            .items_center()
            .flex_shrink_0()
            .gap_1()
            .px_1()
            .py_1()
            .border_b_1()
            .border_color(cx.theme().border);
        if let Some(selection) = selection {
            let scale = if self.seconds {
                1.
            } else {
                f64::from(projection.fps)
            };
            let candidate = projection
                .retime_selection(state)
                .unwrap_or_else(|| selection.clone());
            let target = candidate.duration();
            let start = t!("timeline.retime_fix_start");
            let end = t!("timeline.retime_fix_end");
            let fixed_end = self.retime_fixed_end();
            let buttons: Vec<_> = [
                ("timeline-retime-fix-start", Fixed::Earliest, start),
                ("timeline-retime-fix-end", Fixed::Latest, end),
            ]
            .into_iter()
            .map(|(id, fixed, label)| {
                let selected = fixed_end == fixed;
                compact::header_button(Button::new(id))
                    .label(label.clone().into_owned())
                    .accessibility_label(label.into_owned())
                    .selected(selected)
                    .toggled(selected)
                    .disabled(!enabled || self.gesture.is_some())
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if this.editable(window, cx) {
                            this.finish_number(cx);
                            this.retime.fixed_end = Some(fixed);
                            cx.notify();
                        }
                    }))
            })
            .collect();
            row = row
                .child(compact::group(
                    "timeline-retime-anchor",
                    t!("timeline.retime_anchor"),
                    Axis::Horizontal,
                    buttons,
                ))
                .child(t!(
                    "timeline.retime_original",
                    duration = axis::number(selection.duration() * scale)
                ))
                .child(t!(
                    "timeline.retime_span",
                    start = axis::number(candidate.first * scale),
                    end = axis::number(candidate.last * scale)
                ))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_1()
                        .child(t!("timeline.retime_target"))
                        .child(div().w(compact::rem(74.)).child(self.number(
                            values::Field::RetimeDuration,
                            target * scale,
                            (0., projection.duration * scale),
                            !enabled || self.gesture.is_some(),
                            window,
                            cx,
                        )))
                        .child(t!(if self.seconds {
                            "timeline.seconds_label"
                        } else {
                            "timeline.frames_label"
                        })),
                );
            if !enabled {
                row = row.child(
                    div()
                        .text_color(cx.theme().muted_foreground)
                        .child(t!("timeline.retime_unavailable")),
                );
            }
        } else {
            row = row.child(t!("timeline.retime_distinct"));
        }
        if let Some(error) = &self.error {
            row = row.child(
                div()
                    .id("timeline-retime-error")
                    .test_support()
                    .text_color(cx.theme().danger)
                    .child(error.clone()),
            );
        }
        Some(row.into_any_element())
    }

    pub(super) fn retime_handles(
        &self,
        projection: &TimelineProjection,
        state: &TimelineViewState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        if !self.retime.open || !self.editable(window, cx) {
            return Vec::new();
        }
        let Some(selection) = projection.retime_selection(state) else {
            return Vec::new();
        };
        let (left, right) = state.time_range;
        let primary = cx.theme().primary;
        let background = cx.theme().background;
        let size_px = px(13. * self.ui_scale);
        [
            (
                "timeline-retime-left",
                selection.first,
                Fixed::Latest,
                t!("timeline.retime_left"),
            ),
            (
                "timeline-retime-right",
                selection.last,
                Fixed::Earliest,
                t!("timeline.retime_right"),
            ),
        ]
        .into_iter()
        .filter(|(_, time, _, _)| (left..=right).contains(time))
        .map(|(id, time, fixed, label)| {
            let fraction = ((time - left) / (right - left)) as f32;
            let offset = match (fixed, fraction) {
                (_, 0.) => px(0.),
                (_, 1.) => -size_px,
                (Fixed::Latest, _) => -size_px,
                (Fixed::Earliest, _) => px(0.),
            };
            div()
                .id(id)
                .test_support()
                .absolute()
                .top(px(0.))
                .left(relative(fraction))
                .ml(offset)
                .w(size_px)
                .h(size_px)
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .border_1()
                .border_color(primary)
                .bg(background)
                .text_color(primary)
                .cursor(CursorStyle::ResizeLeftRight)
                .role(Role::Slider)
                .aria_label(label.clone().into_owned())
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(label.clone().into_owned())
                        .build(window, cx)
                })
                .child(if fixed == Fixed::Latest { "‹" } else { "›" })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.start_retime(fixed, event.position, window, cx);
                    }),
                )
                .into_any_element()
        })
        .collect()
    }

    fn retime_pointer_time(&self, position: Point<Pixels>, state: &TimelineViewState) -> f64 {
        drawing::x_time(
            self.local(position).x,
            state.time_range,
            self.graph.get().size.width.as_f32(),
        )
    }

    fn start_retime(
        &mut self,
        fixed: Fixed,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.editable(window, cx) || self.gesture.is_some() {
            return;
        }
        self.finish_number(cx);
        let Some(selection) = self.retime_baseline(cx) else {
            return;
        };
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let Some(state) = owner.read(cx).editor.timeline_view() else {
            return;
        };
        let origin_time = self.retime_pointer_time(position, state);
        let Some(token) = self.begin(cx) else {
            return;
        };
        self.retime.fixed_end = Some(fixed);
        self.gesture = Some(input::Gesture::Retime(Drag {
            token,
            selection,
            fixed,
            origin_x: position.x,
            origin_time,
        }));
        self.pointer = position;
        window.focus(&self.focus, cx);
        self.sync_status_bar(cx);
        cx.notify();
        cx.stop_propagation();
    }

    fn retime_pointer_command(
        &self,
        drag: &Drag,
        position: Point<Pixels>,
        cx: &App,
    ) -> Option<TimelineCommand> {
        let owner = self.owner.upgrade()?;
        let state = owner.read(cx).editor.timeline_view()?;
        let original = match drag.fixed {
            Fixed::Earliest => drag.selection.last,
            Fixed::Latest => drag.selection.first,
        };
        let delta = if position.x == drag.origin_x {
            0.
        } else {
            self.retime_pointer_time(position, state) - drag.origin_time
        };
        Some(drag.selection.command(drag.fixed, original + delta))
    }

    pub(super) fn move_retime(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(input::Gesture::Retime(drag)) = &self.gesture else {
            return;
        };
        if let Some(command) = self.retime_pointer_command(drag, position, cx) {
            self.update_candidate(drag.token, command, cx);
        }
    }

    pub(super) fn finish_retime(
        &mut self,
        drag: Drag,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        // The last rendered candidate may precede the actual release position.
        let valid = self
            .retime_pointer_command(&drag, position, cx)
            .is_some_and(|command| self.update_candidate(drag.token, command, cx));
        if valid {
            self.finish_token(drag.token, cx);
        } else if let Some(target) = self.target(cx) {
            let _ = self.owner.update(cx, |owner, cx| {
                owner
                    .editor
                    .cancel_timeline_edit_for(target.document, drag.token);
                owner.redraw(cx);
            });
        }
    }
}

pub(super) fn retime_error(error: &anyhow::Error) -> String {
    use zipzi_core::Status;
    if let Some(status) = error.downcast_ref::<Status>() {
        let id = &status.message;
        let message = match status.code.as_str() {
            "MOTION_KEY_COLLISION" => Some(t!("timeline.retime_collision", id = id)),
            "OBJECT_LOCKED" => Some(t!("timeline.retime_locked", id = id)),
            "INVALID_MOTION_RETIME_TARGET" => Some(t!("timeline.retime_invalid_target", id = id)),
            "NON_MONOTONIC_MOTION_BEZIER" => Some(t!("timeline.retime_nonmonotonic", id = id)),
            "READ_ONLY_MOTION_TRACK" => Some(t!("timeline.retime_readonly", id = id)),
            _ => None,
        };
        if let Some(message) = message {
            return message.into_owned();
        }
    }
    error.to_string()
}
