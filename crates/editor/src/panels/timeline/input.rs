use super::drawing::{Hit, track_range, x_time, y_value};
use super::*;
use kasane_sdk::MotionBezierHandle;

// Native text editing has priority through the Input context.
actions!(
    timeline,
    [
        DeleteKeys,
        CopyKeys,
        PasteKeys,
        FrameLeft,
        FrameRight,
        PreviousKey,
        NextKey
    ]
);
pub(super) fn bind_keys(cx: &mut App) {
    let scope = Some("KasaneTimeline && !Input && !KasaneModal");
    cx.bind_keys([
        KeyBinding::new("backspace", DeleteKeys, scope),
        KeyBinding::new("delete", DeleteKeys, scope),
        KeyBinding::new("cmd-c", CopyKeys, scope),
        KeyBinding::new("cmd-v", PasteKeys, scope),
        KeyBinding::new("left", FrameLeft, scope),
        KeyBinding::new("right", FrameRight, scope),
        KeyBinding::new("alt-left", PreviousKey, scope),
        KeyBinding::new("alt-right", NextKey, scope),
    ]);
}
pub(super) enum Gesture {
    Seek,
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
    fn local(&self, position: Point<Pixels>) -> Point<f32> {
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
        window.focus(&self.focus, cx);
        self.gesture = Some(Gesture::Seek);
        self.seek_at(event.position, true, cx);
        cx.stop_propagation();
    }
    fn seek_at(&mut self, position: Point<Pixels>, scrub: bool, cx: &mut Context<Self>) {
        let Some(target) = self.target(cx) else {
            return;
        };
        let bounds = self.ruler.get();
        let _ = self.owner.update(cx, |owner, cx| {
            let Some(state) = owner.editor.timeline_view() else {
                return;
            };
            let Some(projection) = owner.editor.timeline_projection() else {
                return;
            };
            let max = if target.mode == AnimationPreviewMode::Result {
                owner
                    .editor
                    .animation()
                    .and_then(|a| a.result_playback_position())
                    .map_or(projection.display_end, |p| {
                        f64::from(p.period).max(projection.display_end)
                    })
            } else {
                projection.display_end
            };
            let time = x_time(
                (position.x - bounds.left()).as_f32(),
                state.time_range,
                bounds.size.width.as_f32(),
            )
            .clamp(0., max);
            let result = timeline_time(time, projection.fps, state.snap).and_then(|time| {
                if scrub {
                    owner.editor.scrub_timeline(target.document, time)
                } else {
                    owner.editor.seek_timeline(target.document, time)
                }
            });
            owner.animation_anchor = None;
            owner.complete_edit(result, cx);
        });
    }
    fn begin(&mut self, cx: &mut Context<Self>) -> Option<TimelineCandidateIdentity> {
        let target = self.target(cx)?;
        let result = self
            .owner
            .update(cx, |owner, cx| {
                let token = owner
                    .editor
                    .begin_timeline_edit(target.document, "Edit motion");
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
                let result = owner
                    .editor
                    .update_timeline_edit(target.document, token, command);
                owner.redraw(cx);
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
        let additive = event.modifiers.shift || event.modifiers.platform;
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
        cx.notify();
        cx.stop_propagation();
    }
    pub(super) fn move_pointer(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.gesture.is_none() {
            return;
        }
        if !self.allowed(window, cx) || event.pressed_button != Some(MouseButton::Left) {
            self.cancel(window, cx);
            return;
        }
        let local = self.local(event.position);
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let state = owner
            .read(cx)
            .editor
            .timeline_view()
            .cloned()
            .unwrap_or_default();
        let width = self.graph.get().size.width.as_f32().max(1.);
        let height = self.graph.get().size.height.as_f32();
        let time_delta = |origin: f32| {
            (local.x - origin) as f64 / width as f64 * (state.time_range.1 - state.time_range.0)
        };
        let value_delta = |origin: f32, scale: (f64, f64)| {
            let delta = y_value(local.y, self.value_range, height)
                - y_value(origin, self.value_range, height);
            delta
                * if state.normalized {
                    scale.1 - scale.0
                } else {
                    1.
                }
        };
        let command = match self.gesture.as_mut().unwrap() {
            Gesture::Seek => {
                self.seek_at(event.position, true, cx);
                return;
            }
            Gesture::Marquee {
                origin,
                current,
                keys,
                events,
            } => {
                *current = local;
                let (mut selected, mut selected_events) = self
                    .scene
                    .borrow()
                    .scene
                    .as_ref()
                    .map(|scene| {
                        scene.marquee(
                            point(px(origin.x), px(origin.y)),
                            point(px(local.x), px(local.y)),
                        )
                    })
                    .unwrap_or_default();
                selected.extend(keys.clone());
                selected_events.extend(events.clone());
                let projection = owner.read(cx).editor.timeline_projection().unwrap();
                self.change_view(
                    move |view| {
                        view.keys = selected.into_iter().collect();
                        view.events = selected_events.into_iter().collect();
                        for row in &projection.rows {
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
        if !self.allowed(window, cx) {
            if self.gesture.is_some() || self.number_edit.is_some() {
                self.cancel(window, cx);
            }
            return;
        }
        if let Some(gesture) = self.gesture.take() {
            if matches!(gesture, Gesture::Seek) {
                self.seek_at(window.mouse_position(), false, cx);
            }
            if let Some(token) = gesture.token() {
                self.finish_token(token, cx);
            }
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
        if !self.allowed(window, cx) || self.gesture.is_some() || self.number_edit.is_some() {
            return;
        }
        let delta = match event.delta {
            ScrollDelta::Pixels(p) => point(p.x.as_f32(), p.y.as_f32()),
            ScrollDelta::Lines(p) => point(p.x * ROW, p.y * ROW),
        };
        let Some(owner) = self.owner.upgrade() else {
            return;
        };
        let Some(state) = owner.read(cx).editor.timeline_view().cloned() else {
            return;
        };
        if event.modifiers.platform {
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
            let offset = -if event.modifiers.shift {
                delta.y
            } else {
                delta.x
            } as f64
                / self.graph.get().size.width.as_f32().max(1.) as f64
                * (state.time_range.1 - state.time_range.0);
            let left = (state.time_range.0 + offset).max(0.);
            let span = state.time_range.1 - state.time_range.0;
            self.change_view(move |view| view.time_range = (left, left + span), cx);
        } else {
            let max = (owner
                .read(cx)
                .editor
                .timeline_rows()
                .unwrap_or_default()
                .len() as f64
                * ROW as f64
                - self.body_height as f64)
                .max(0.);
            self.change_view(
                move |view| view.scroll = (view.scroll - delta.y as f64).clamp(0., max),
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
    pub(super) fn key_command(
        &mut self,
        command: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.allowed(window, cx) || window.has_focused_input(cx) {
            return;
        }
        let Some(target) = self.target(cx) else {
            return;
        };
        self.finish_number(cx);
        match command {
            "delete" | "copy" | "paste" => {
                if command != "copy" && !self.editable(window, cx) {
                    return;
                }
                let _ = self.owner.update(cx, |owner, cx| {
                    let result = match command {
                        "delete" => owner.editor.delete_timeline_selection(target.document),
                        "copy" => owner
                            .editor
                            .copy_timeline_selection(target.document)
                            .map(|_| ()),
                        _ => {
                            let selected = owner.editor.timeline_view().and_then(|state| {
                                (state.channels.len() == 1).then(|| state.channels.first().unwrap())
                            });
                            let paste = selected
                                .and_then(|id| {
                                    owner
                                        .editor
                                        .timeline_rows()?
                                        .iter()
                                        .find(|row| &row.id == id)
                                })
                                .and_then(|row| {
                                    row.channel.clone().map(|channel| TimelinePasteTarget {
                                        channel,
                                        track_id: row.track_id.clone(),
                                    })
                                });
                            owner.editor.paste_timeline(target.document, paste)
                        }
                    };
                    owner.complete_edit(result, cx);
                });
            }
            _ => {
                let _ = self.owner.update(cx, |owner, cx| {
                    let Some(projection) = owner.editor.timeline_projection() else {
                        return;
                    };
                    let time = owner.editor.animation().unwrap().edit_time as f64;
                    let next = match command {
                        "left" => time - 1. / projection.fps as f64,
                        "right" => time + 1. / projection.fps as f64,
                        "previous" => projection
                            .tracks
                            .values()
                            .flat_map(|track| &track.keys)
                            .map(|key| key.time)
                            .filter(|key| *key < time)
                            .max_by(f64::total_cmp)
                            .unwrap_or(0.),
                        _ => projection
                            .tracks
                            .values()
                            .flat_map(|track| &track.keys)
                            .map(|key| key.time)
                            .filter(|key| *key > time)
                            .min_by(f64::total_cmp)
                            .unwrap_or(projection.duration),
                    }
                    .clamp(0., projection.display_end);
                    let result = owner.editor.seek_timeline(target.document, next as f32);
                    owner.animation_anchor = None;
                    owner.complete_edit(result, cx);
                });
            }
        }
        cx.stop_propagation();
    }
}
