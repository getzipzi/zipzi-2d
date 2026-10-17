use super::drawing::{Hit, track_range, x_time, y_value};
use super::*;
use kasane_sdk::MotionBezierHandle;

pub(super) enum Gesture {
    Pan {
        last: Point<f32>,
        button: MouseButton,
    },
    Seek(editor_core::TimelineScrubToken),
    Marquee {
        origin: Point<f32>,
        current: Point<f32>,
        keys: Vec<TimelineKeyRef>,
        events: Vec<String>,
    },
    Keys {
        token: TimelineCandidateIdentity,
        origin: Point<f32>,
        keys: Vec<TimelineKeyRef>,
        anchor: TimelineKeyRef,
        time_editable: bool,
        value_editable: bool,
        scale: (f64, f64),
    },
    Handle {
        token: TimelineCandidateIdentity,
        origin: Point<f32>,
        end: TimelineKeyRef,
        handle: MotionBezierHandle,
        time: f64,
        value: f32,
        horizontal: bool,
        scale: (f64, f64),
    },
    Event {
        token: TimelineCandidateIdentity,
        origin: f32,
        id: String,
        time: f64,
        value: String,
    },
}
impl Gesture {
    pub fn button(&self) -> MouseButton {
        match self {
            Self::Pan { button, .. } => *button,
            _ => MouseButton::Left,
        }
    }
    pub fn token(&self) -> Option<TimelineCandidateIdentity> {
        match self {
            Self::Keys { token, .. } | Self::Handle { token, .. } | Self::Event { token, .. } => {
                Some(*token)
            }
            _ => None,
        }
    }
}
impl Timeline {
    pub(super) fn local(&self, position: Point<Pixels>) -> Point<f32> {
        let local = position - self.graph.get().origin;
        point(local.x.as_f32(), local.y.as_f32())
    }
    pub(super) fn start_seek(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.allowed(window, cx) {
            return;
        }
        self.finish_number(cx);
        let Some(target) = self.target(cx) else {
            return;
        };
        let Some(time) = self.seek_time(event.position, cx) else {
            return;
        };
        let token = self
            .owner
            .update(cx, |owner, cx| {
                let result = owner.editor.begin_timeline_scrub(target.binding, time);
                let token = result.as_ref().ok().copied();
                owner.animation_anchor = None;
                owner.complete_edit(result.map(|_| ()), cx);
                token
            })
            .ok()
            .flatten();
        if let Some(token) = token {
            window.focus(&self.focus, cx);
            self.gesture = Some(Gesture::Seek(token));
            self.sync_status_bar(cx);
            cx.notify();
        }
        cx.stop_propagation();
    }
    fn seek_time(&self, position: Point<Pixels>, cx: &App) -> Option<f32> {
        let target = self.target(cx)?;
        let owner = self.owner.upgrade()?;
        let editor = &owner.read(cx).editor;
        let state = editor.timeline_view()?;
        let projection = editor.timeline_projection()?;
        let max = if target.mode == AnimationPreviewMode::Result {
            editor
                .animation()
                .and_then(|a| a.result_playback_position())
                .map_or(projection.display_end, |p| {
                    f64::from(p.period).max(projection.display_end)
                })
        } else {
            projection.display_end
        };
        let bounds = self.ruler.get();
        let time = x_time(
            (position.x - bounds.left()).as_f32(),
            state.time_range,
            bounds.size.width.as_f32(),
        )
        .clamp(0., max);
        timeline_time(time, projection.fps, state.snap).ok()
    }
    fn begin(&mut self, cx: &mut Context<Self>) -> Option<TimelineCandidateIdentity> {
        let target = self.target(cx)?;
        let result = self
            .owner
            .update(cx, |owner, cx| {
                let token = owner
                    .editor
                    .begin_timeline_edit_for(target.binding, "Edit motion");
                owner.redraw(cx);
                token
            })
            .ok()?;
        match result {
            Ok(token) => {
                self.error = None;
                Some(token)
            }
            Err(error) => {
                self.error = Some(error.to_string());
                None
            }
        }
    }
    pub(super) fn update_candidate(
        &mut self,
        token: TimelineCandidateIdentity,
        command: TimelineCommand,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.target(cx) else {
            return;
        };
        let result = self
            .owner
            .update(cx, |owner, cx| {
                if !owner.editor.timeline_edit_owned(target.binding, token) {
                    return Ok(token);
                }
                let result = owner
                    .editor
                    .update_timeline_edit(target.document, token, command);
                owner.request_preview();
                owner.sync_animation_clock(cx);
                result
            })
            .ok();
        self.error = result.and_then(Result::err).map(|error| error.to_string());
        cx.notify();
    }
    pub(super) fn start_graph(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.allowed(window, cx) {
            return;
        }
        if self.pan_tool {
            self.start_pan(event, window, cx);
            return;
        }
        self.finish_number(cx);
        window.focus(&self.focus, cx);
        let local = self.local(event.position);
        let hit = self
            .scene
            .borrow()
            .scene
            .as_ref()
            .map(|scene| scene.hit(point(px(local.x), px(local.y))));
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let Some(projection) = owner.read(cx).editor.timeline_projection() else {
            return;
        };
        let state = owner
            .read(cx)
            .editor
            .timeline_view()
            .cloned()
            .unwrap_or_default();
        let additive = event.modifiers.shift || event.modifiers.secondary();
        match hit {
            Some(Err(())) => {
                self.error = Some(t!("timeline.ambiguous").into_owned());
                cx.notify();
                return;
            }
            Some(Ok(Some(Hit::Summary(keys)))) => {
                self.select_keys(keys, additive, true, cx);
                return;
            }
            Some(Ok(Some(Hit::Key(anchor)))) => {
                if projection.key(&anchor).is_none() {
                    return;
                }
                if additive && state.keys.contains(&anchor) {
                    let anchor = anchor.clone();
                    self.change_view(
                        move |view| {
                            view.keys.remove(&anchor);
                        },
                        cx,
                    );
                    return;
                }
                if !state.keys.contains(&anchor) {
                    self.select_keys(vec![anchor.clone()], additive, false, cx);
                }
                if !self.editable(window, cx) {
                    return;
                }
                let keys: Vec<_> = owner
                    .read(cx)
                    .editor
                    .timeline_view()
                    .unwrap()
                    .keys
                    .iter()
                    .cloned()
                    .collect();
                if keys
                    .iter()
                    .any(|key| !projection.tracks[&key.track_id].editable)
                {
                    return;
                }
                let time_editable = keys
                    .iter()
                    .all(|key| projection.key(key).is_some_and(|key| key.time_editable));
                let value_editable = state.mode == TimelineDisplayMode::Curves
                    && keys.iter().all(|key| key.track_id == anchor.track_id)
                    && projection.tracks[&anchor.track_id].editable;
                let scale = track_range(&projection, &anchor.track_id);
                if let Some(token) = self.begin(cx) {
                    self.gesture = Some(Gesture::Keys {
                        token,
                        origin: local,
                        keys,
                        anchor,
                        time_editable,
                        value_editable,
                        scale,
                    });
                }
            }
            Some(Ok(Some(Hit::Handle { end, handle }))) => {
                if !self.editable(window, cx) {
                    return;
                }
                let Some(handles) = projection
                    .display_handles(&end)
                    .filter(|handles| handles.value_editable)
                else {
                    self.error = Some(t!("timeline.non_monotonic").into_owned());
                    return;
                };
                let control = match handle {
                    MotionBezierHandle::Control1 => handles.control1,
                    MotionBezierHandle::Control2 => handles.control2,
                };
                let scale = track_range(&projection, &end.track_id);
                if let Some(token) = self.begin(cx) {
                    self.gesture = Some(Gesture::Handle {
                        token,
                        origin: local,
                        end,
                        handle,
                        time: f64::from(control.time),
                        value: control.value,
                        horizontal: handles.horizontal_editable,
                        scale,
                    });
                }
            }
            Some(Ok(Some(Hit::Event(id)))) => {
                let selected = id.clone();
                self.change_view(
                    move |view| {
                        if !additive {
                            view.keys.clear();
                            view.events.clear();
                            view.channels.clear();
                        }
                        view.events.insert(selected);
                        view.channels.insert(TimelineRowId::Events);
                    },
                    cx,
                );
                if !self.editable(window, cx) {
                    return;
                }
                let event = projection
                    .events
                    .iter()
                    .find(|event| event.id == id)
                    .unwrap();
                if let Some(token) = self.begin(cx) {
                    self.gesture = Some(Gesture::Event {
                        token,
                        origin: local.x,
                        id,
                        time: f64::from(event.time),
                        value: event.value.clone(),
                    });
                }
            }
            _ => {
                self.gesture = Some(Gesture::Marquee {
                    origin: local,
                    current: local,
                    keys: if additive {
                        state.keys.into_iter().collect()
                    } else {
                        vec![]
                    },
                    events: if additive {
                        state.events.into_iter().collect()
                    } else {
                        vec![]
                    },
                });
                if !additive {
                    self.change_view(
                        |view| {
                            view.keys.clear();
                            view.events.clear();
                        },
                        cx,
                    );
                }
            }
        }
        self.pointer = event.position;
        self.sync_status_bar(cx);
        cx.notify();
        cx.stop_propagation();
    }
    pub(super) fn move_pointer(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.discard_stale_interactions(window, cx);
        if self.gesture.is_none() {
            return;
        }
        if !self.allowed(window, cx)
            || event.pressed_button != self.gesture.as_ref().map(Gesture::button)
        {
            self.cancel(window, cx);
            return;
        }
        let local = self.local(event.position);
        self.pointer = event.position;
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let state = owner
            .read(cx)
            .editor
            .timeline_view()
            .cloned()
            .unwrap_or_default();
        if let Some(Gesture::Pan { last, .. }) = self.gesture.as_mut() {
            let delta = local - *last;
            *last = local;
            self.pan_by(
                -delta.x,
                -delta.y,
                state.mode == TimelineDisplayMode::Curves,
                cx,
            );
            cx.stop_propagation();
            return;
        }
        self.start_edge_scroll(window, cx);
        let width = self.graph.get().size.width.as_f32().max(1.);
        let height = self.graph.get().size.height.as_f32();
        let time_delta = |origin: f32| {
            (local.x - origin) as f64 / width as f64 * (state.time_range.1 - state.time_range.0)
        };
        let row_height = self.row_height();
        let value_delta = |origin: f32, scale: (f64, f64)| {
            let delta = y_value(local.y, self.value_range, height, row_height)
                - y_value(origin, self.value_range, height, row_height);
            delta
                * if state.normalized {
                    scale.1 - scale.0
                } else {
                    1.
                }
        };
        let command = match self.gesture.as_mut().unwrap() {
            Gesture::Pan { .. } => unreachable!(),
            Gesture::Seek(token) => {
                let token = *token;
                if let Some(time) = self.seek_time(event.position, cx) {
                    let _ = self.owner.update(cx, |owner, cx| {
                        let result = owner.editor.update_timeline_scrub(token, time);
                        owner.complete_edit(result, cx);
                    });
                }
                return;
            }
            Gesture::Marquee {
                origin,
                current,
                keys,
                events,
            } => {
                *current = local;
                let (mut selected, mut selected_events) = super::navigation::marquee_selection(
                    &owner.read(cx).editor.timeline_projection().unwrap(),
                    &state,
                    &owner.read(cx).editor.timeline_visible_rows().unwrap(),
                    *origin,
                    local,
                    self.value_range,
                    width,
                    height,
                    row_height,
                );
                selected.extend(keys.clone());
                selected_events.extend(events.clone());
                let projection = owner.read(cx).editor.timeline_projection().unwrap();
                self.change_view(
                    move |view| {
                        view.keys = selected.into_iter().collect();
                        view.events = selected_events.into_iter().collect();
                        for row in projection.rows.iter() {
                            if row.keys.iter().any(|key| view.keys.contains(key)) {
                                view.channels.insert(row.id.clone());
                            }
                        }
                    },
                    cx,
                );
                cx.notify();
                return;
            }
            Gesture::Keys {
                token,
                origin,
                keys,
                anchor,
                time_editable,
                value_editable,
                scale,
            } => Some((
                *token,
                TimelineCommand::MoveKeys {
                    keys: keys.clone(),
                    anchor: anchor.clone(),
                    time_delta: if *time_editable {
                        time_delta(origin.x)
                    } else {
                        0.
                    },
                    value_delta: if *value_editable {
                        value_delta(origin.y, *scale)
                    } else {
                        0.
                    },
                },
            )),
            Gesture::Handle {
                token,
                origin,
                end,
                handle,
                time,
                value,
                horizontal,
                scale,
            } => Some((
                *token,
                TimelineCommand::Handle {
                    end: end.clone(),
                    handle: *handle,
                    time: *time
                        + if *horizontal {
                            time_delta(origin.x)
                        } else {
                            0.
                        },
                    value: (f64::from(*value) + value_delta(origin.y, *scale)) as f32,
                },
            )),
            Gesture::Event {
                token,
                origin,
                id,
                time,
                value,
            } => Some((
                *token,
                TimelineCommand::Event {
                    id: Some(id.clone()),
                    time: *time + time_delta(*origin),
                    value: value.clone(),
                },
            )),
        };
        if let Some((token, command)) = command {
            self.update_candidate(token, command, cx);
        }
        cx.stop_propagation();
    }
    pub(super) fn release_pointer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.discard_stale_interactions(window, cx);
        self.edge_scroll = None;
        if !self.allowed(window, cx) {
            if self.gesture.is_some() || self.number_edit.is_some() {
                self.cancel(window, cx);
            }
            return;
        }
        if let Some(gesture) = self.gesture.take() {
            if let Gesture::Seek(token) = gesture {
                if let Some(time) = self.seek_time(window.mouse_position(), cx) {
                    let _ = self.owner.update(cx, |owner, cx| {
                        let result = owner.editor.finish_timeline_scrub(token, time);
                        owner.complete_edit(result, cx);
                    });
                }
            }
            if let Some(token) = gesture.token() {
                self.finish_token(token, cx);
            }
            self.sync_status_bar(cx);
            cx.notify();
        }
        if self.number_edit.as_ref().is_some_and(|edit| edit.sliding) {
            self.finish_number(cx);
        }
    }
    pub(super) fn finish_token(
        &mut self,
        token: TimelineCandidateIdentity,
        cx: &mut Context<Self>,
    ) {
        let Some(target) = self.target(cx) else {
            return;
        };
        let _ = self.owner.update(cx, |owner, cx| {
            if !owner.editor.timeline_edit_owned(target.binding, token) {
                return;
            }
            let result = owner.editor.finish_timeline_edit(target.document, token);
            owner.complete_edit(result, cx);
        });
    }
    pub(super) fn wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.allowed(window, cx) || self.number_edit.is_some() {
            return;
        }
        let delta = match event.delta {
            ScrollDelta::Pixels(p) => point(p.x.as_f32(), p.y.as_f32()),
            ScrollDelta::Lines(p) => point(p.x * self.row_height(), p.y * self.row_height()),
        };
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let Some(state) = owner.read(cx).editor.timeline_view().cloned() else {
            return;
        };
        let mut moved = false;
        if event.modifiers.secondary() {
            if self.gesture.is_some() {
                return;
            }
            let bounds = self.graph.get();
            let anchor = x_time(
                (event.position.x - bounds.left()).as_f32(),
                state.time_range,
                bounds.size.width.as_f32(),
            );
            let scale = (delta.y as f64 * 0.01).exp().clamp(0.5, 2.);
            let span = ((state.time_range.1 - state.time_range.0) * scale).clamp(1e-4, 1e6);
            let fraction =
                (anchor - state.time_range.0) / (state.time_range.1 - state.time_range.0);
            let left = (anchor - fraction * span).max(0.);
            self.change_view(move |view| view.time_range = (left, left + span), cx);
        } else if event.modifiers.shift || delta.x.abs() > delta.y.abs() {
            let horizontal = if event.modifiers.shift && delta.x.abs() <= delta.y.abs() {
                delta.y
            } else {
                delta.x
            };
            moved = self.pan_by(-horizontal, 0., false, cx);
        } else {
            moved = self.pan_by(0., -delta.y, false, cx);
        }
        if moved && self.gesture.is_some() {
            self.move_pointer(
                &MouseMoveEvent {
                    position: self.pointer,
                    pressed_button: self.gesture.as_ref().map(Gesture::button),
                    modifiers: event.modifiers,
                },
                window,
                cx,
            );
        }
        cx.stop_propagation();
    }
    pub(super) fn escape(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.keystroke.key == "escape"
            && self.allowed(window, cx)
            && (self.gesture.is_some() || self.number_edit.is_some())
        {
            self.cancel(window, cx);
            window.focus(&self.focus, cx);
            cx.stop_propagation();
        }
    }
    pub(super) fn user_command(
        &mut self,
        id: editor_core::CommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.allowed(window, cx) || window.has_focused_input(cx) {
            return;
        }
        self.finish_number(cx);
        let Some(target) = self.target(cx) else {
            return;
        };
        let command = self.owner.upgrade().map(|owner| {
            owner.read(cx).editor.command_for_timeline(
                id,
                target.binding,
                editor_core::CommandSource::Toolbar,
            )
        });
        if let Some(command) = command {
            self.user_invocation(command, window, cx);
        }
    }
    pub(super) fn user_invocation(
        &mut self,
        command: editor_core::Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.allowed(window, cx) || window.has_focused_input(cx) {
            return;
        }
        let _ = self
            .owner
            .update(cx, |owner, cx| owner.run_invocation(command, window, cx));
        // A matched but disabled local command stays consumed by this panel.
        cx.stop_propagation();
    }
}
