use super::drawing::{display_value, selected_tracks, track_range, value_y, x_time, y_value};
use super::input::Gesture;
use super::*;
use std::time::Duration;

impl Timeline {
    pub(super) fn start_pan(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.allowed(window, cx) {
            return;
        }
        self.finish_number(cx);
        self.edge_scroll = None;
        window.focus(&self.focus, cx);
        self.gesture = Some(Gesture::Pan {
            last: self.local(event.position),
            button: event.button,
        });
        cx.stop_propagation();
        cx.notify();
    }

    /// Move the camera without canceling an in-flight SDK candidate. Gesture
    /// origins move with their world coordinates, so total deltas stay stable.
    pub(super) fn pan_by(
        &mut self,
        dx: f32,
        dy: f32,
        curve_pan: bool,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(target) = self.target(cx) else {
            return false;
        };
        let Some(owner) = self.owner.upgrade() else {
            return false;
        };
        let Some(state) = owner.read(cx).editor.timeline_view().cloned() else {
            return false;
        };
        let bounds = self.graph.get();
        let width = bounds.size.width.as_f32().max(1.);
        let height = bounds.size.height.as_f32().max(1.);
        let span = state.time_range.1 - state.time_range.0;
        let left = (state.time_range.0 + f64::from(dx / width) * span).max(0.);
        let horizontal = ((left - state.time_range.0) / span * f64::from(width)) as f32;
        let max_scroll = (owner
            .read(cx)
            .editor
            .timeline_rows()
            .map_or(0, |rows| rows.len()) as f64
            * self.row_height() as f64
            - f64::from(height))
        .max(0.);
        let scroll = if curve_pan {
            state.scroll
        } else {
            (state.scroll + f64::from(dy)).clamp(0., max_scroll)
        };
        let vertical = (scroll - state.scroll) as f32;
        let value_shift = if curve_pan {
            y_value(dy, self.value_range, height, self.row_height())
                - y_value(0., self.value_range, height, self.row_height())
        } else {
            0.
        };
        if horizontal == 0. && vertical == 0. && value_shift == 0. {
            return false;
        }
        let old_range = self.value_range;
        self.value_range = (old_range.0 + value_shift, old_range.1 + value_shift);
        let row_height = self.row_height();
        let move_origin = |origin: &mut Point<f32>| {
            origin.x -= horizontal;
            if curve_pan {
                origin.y = value_y(
                    y_value(origin.y, old_range, height, row_height),
                    self.value_range,
                    height,
                    row_height,
                );
            } else if state.mode == TimelineDisplayMode::Overview {
                origin.y -= vertical;
            }
        };
        match self.gesture.as_mut() {
            Some(
                Gesture::Marquee { origin, .. }
                | Gesture::Keys { origin, .. }
                | Gesture::Handle { origin, .. },
            ) => move_origin(origin),
            Some(Gesture::Event { origin, .. }) => *origin -= horizontal,
            _ => {}
        }
        let _ = self.owner.update(cx, |owner, cx| {
            let result =
                owner
                    .editor
                    .navigate_timeline_view(target.document, (left, left + span), scroll);
            owner.complete_edit(result, cx);
        });
        cx.notify();
        true
    }

    pub(super) fn start_edge_scroll(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.edge_scroll.is_some()
            || self.gesture.is_none()
            || matches!(self.gesture, Some(Gesture::Pan { .. }))
        {
            return;
        }
        self.edge_scroll = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(16))
                    .await;
                let running = this
                    .update_in(cx, |this, window, cx| this.edge_tick(window, cx))
                    .unwrap_or(false);
                if !running {
                    break;
                }
            }
        }));
    }

    fn edge_tick(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.gesture.is_none() {
            return false;
        }
        if !self.allowed(window, cx) {
            self.cancel(window, cx);
            return false;
        }
        let point = self.local(self.pointer);
        let bounds = self.graph.get();
        let velocity = |position: f32, length: f32| {
            let edge = 24_f32.min(length / 3.);
            if edge <= 0. {
                0.
            } else if position < edge {
                -((edge - position) / edge).clamp(0., 2.) * 8.
            } else if position > length - edge {
                ((position - length + edge) / edge).clamp(0., 2.) * 8.
            } else {
                0.
            }
        };
        let mut dx = velocity(point.x, bounds.size.width.as_f32());
        if dx > 0. {
            if let Some(owner) = self.owner.upgrade() {
                let editor = &owner.read(cx).editor;
                if let (Some(projection), Some(view)) =
                    (editor.timeline_projection(), editor.timeline_view())
                {
                    let span = view.time_range.1 - view.time_range.0;
                    let end = editor
                        .animation()
                        .and_then(|a| a.result_playback_position())
                        .map_or(projection.display_end, |p| {
                            projection.display_end.max(f64::from(p.period))
                        });
                    let remaining = ((end - span).max(0.) - view.time_range.0).max(0.);
                    dx = dx.min((remaining / span * f64::from(bounds.size.width.as_f32())) as f32);
                }
            }
        }
        let dy = if matches!(self.gesture, Some(Gesture::Seek | Gesture::Event { .. })) {
            0.
        } else {
            velocity(point.y, bounds.size.height.as_f32())
        };
        let curves = self
            .owner
            .upgrade()
            .and_then(|owner| owner.read(cx).editor.timeline_view().map(|view| view.mode))
            == Some(TimelineDisplayMode::Curves);
        if self.pan_by(dx, dy, curves, cx) {
            self.move_pointer(
                &MouseMoveEvent {
                    position: self.pointer,
                    pressed_button: Some(MouseButton::Left),
                    modifiers: Modifiers::default(),
                },
                window,
                cx,
            );
        }
        true
    }
}

/// A marquee covers world coordinates, including keys scrolled off screen.
pub(super) fn marquee_selection(
    projection: &TimelineProjection,
    state: &TimelineViewState,
    rows: &[TimelineRow],
    origin: Point<f32>,
    current: Point<f32>,
    range: (f64, f64),
    width: f32,
    height: f32,
    row_height: f32,
) -> (Vec<TimelineKeyRef>, Vec<String>) {
    let a = x_time(origin.x, state.time_range, width);
    let b = x_time(current.x, state.time_range, width);
    let time = a.min(b)..=a.max(b);
    let mut keys = Vec::new();
    let mut events = Vec::new();
    if state.mode == TimelineDisplayMode::Overview {
        let top = origin.y.min(current.y) as f64 + state.scroll;
        let bottom = origin.y.max(current.y) as f64 + state.scroll;
        for (index, row) in rows.iter().enumerate() {
            let y = index as f64 * row_height as f64 + row_height as f64 / 2.;
            if y < top || y > bottom {
                continue;
            }
            if let Some(track) = row
                .track_id
                .as_ref()
                .and_then(|id| projection.tracks.get(id))
            {
                keys.extend(
                    track
                        .visible_keys(*time.start(), *time.end())
                        .iter()
                        .map(|key| key.reference.clone()),
                );
            }
            if row.id == TimelineRowId::Events {
                events.extend(
                    projection
                        .events
                        .iter()
                        .filter(|event| time.contains(&f64::from(event.time)))
                        .map(|event| event.id.clone()),
                );
            }
        }
    } else {
        let a = y_value(origin.y, range, height, row_height);
        let b = y_value(current.y, range, height, row_height);
        let values = a.min(b)..=a.max(b);
        for track in selected_tracks(projection, state) {
            let scale = track_range(projection, &track.id);
            keys.extend(
                track
                    .visible_keys(*time.start(), *time.end())
                    .iter()
                    .filter(|key| {
                        values.contains(&display_value(
                            f64::from(key.value),
                            state.normalized,
                            scale,
                        ))
                    })
                    .map(|key| key.reference.clone()),
            );
        }
        if values.contains(&y_value(
            height - row_height / 2.,
            range,
            height,
            row_height,
        )) {
            events.extend(
                projection
                    .events
                    .iter()
                    .filter(|event| time.contains(&f64::from(event.time)))
                    .map(|event| event.id.clone()),
            );
        }
    }
    (keys, events)
}
