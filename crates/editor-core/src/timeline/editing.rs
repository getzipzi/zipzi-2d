use super::*;
use crate::{AnimationSource, DocumentId, DocumentSession, Editor, MotionSelection};
use anyhow::{Result, ensure};
use kasane_core::document::{MotionEvent, MotionKey, MotionPoint, MotionTrack};
use kasane_sdk::{
    MotionBezierHandle, MotionClipCandidate, MotionInterpolation, MotionKeyMove, Version,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TimelineCandidateIdentity {
    pub edit: u64,
    pub revision: u64,
}

#[derive(Debug, Clone)]
pub enum TimelineCommand {
    WriteValue {
        channel: TimelineChannelId,
        track_id: Option<String>,
        selected_key: Option<TimelineKeyRef>,
        value: f32,
    },
    InsertKey {
        channel: TimelineChannelId,
        track_id: Option<String>,
        selected_key: Option<TimelineKeyRef>,
    },
    MoveKeys {
        keys: Vec<TimelineKeyRef>,
        anchor: TimelineKeyRef,
        time_delta: f64,
        value_delta: f64,
    },
    SetKey {
        key: TimelineKeyRef,
        time: f64,
        value: f32,
    },
    Delete {
        keys: Vec<TimelineKeyRef>,
        events: Vec<String>,
    },
    Interpolation {
        keys: Vec<TimelineKeyRef>,
        kind: MotionInterpolation,
    },
    Handle {
        end: TimelineKeyRef,
        handle: MotionBezierHandle,
        time: f64,
        value: f32,
    },
    Event {
        id: Option<String>,
        time: f64,
        value: String,
    },
    ScaleTime(f64),
    Timing {
        duration: f32,
        fps: f32,
        looping: bool,
        fade_in: Option<f32>,
        fade_out: Option<f32>,
    },
}
impl TimelineCommand {
    fn target(&self) -> String {
        match self {
            Self::WriteValue {
                channel,
                track_id,
                selected_key,
                ..
            }
            | Self::InsertKey {
                channel,
                track_id,
                selected_key,
            } => format!("write:{channel:?}:{track_id:?}:{selected_key:?}"),
            Self::MoveKeys { keys, anchor, .. } => format!("move:{keys:?}:{anchor:?}"),
            Self::SetKey { key, .. } => format!("key:{key:?}"),
            Self::Delete { keys, events } => format!("delete:{keys:?}:{events:?}"),
            Self::Interpolation { keys, .. } => format!("interpolation:{keys:?}"),
            Self::Handle { end, handle, .. } => format!("handle:{end:?}:{handle:?}"),
            Self::Event { id, .. } => format!("event:{id:?}"),
            Self::ScaleTime(_) => "scale".into(),
            Self::Timing { .. } => "timing".into(),
        }
    }
}

pub struct TimelineEdit {
    pub identity: TimelineCandidateIdentity,
    pub base_version: Version,
    pub local_time: f64,
    pub candidate: MotionClipCandidate,
    original_candidate: MotionClipCandidate,
    original_projection: Arc<TimelineProjection>,
    original_keys: HashSet<String>,
    source: Arc<AnimationSource>,
    target: Option<String>,
    track_id: String,
    key_id: String,
    event_id: String,
    label: String,
}
impl TimelineEdit {
    fn apply(
        &self,
        document: &crate::Document,
        command: &TimelineCommand,
        snap: bool,
    ) -> Result<MotionClipCandidate> {
        let model = document.model();
        let mut candidate = self.original_candidate.clone();
        let projection = &self.original_projection;
        let validate_key = |key: &TimelineKeyRef| -> Result<()> {
            ensure!(
                projection.key(key).is_some(),
                "Key no longer exists in this clip"
            );
            Ok(())
        };
        match command {
            TimelineCommand::WriteValue {
                channel,
                track_id,
                selected_key,
                value,
            } => self.write(
                document,
                &projection,
                &mut candidate,
                channel,
                track_id.as_deref(),
                selected_key.as_ref(),
                Some(*value),
                snap,
            )?,
            TimelineCommand::InsertKey {
                channel,
                track_id,
                selected_key,
            } => self.write(
                document,
                &projection,
                &mut candidate,
                channel,
                track_id.as_deref(),
                selected_key.as_ref(),
                None,
                snap,
            )?,
            TimelineCommand::MoveKeys {
                keys,
                anchor,
                time_delta,
                value_delta,
            } => {
                ensure!(
                    time_delta.is_finite() && value_delta.is_finite() && keys.contains(anchor),
                    "Invalid key movement"
                );
                for key in keys {
                    validate_key(key)?;
                    ensure!(
                        projection.tracks[&key.track_id].editable,
                        "Track is read-only"
                    );
                }
                if *time_delta == 0. && *value_delta == 0. {
                    return Ok(candidate);
                }
                ensure!(
                    *value_delta == 0. || keys.iter().all(|key| key.track_id == anchor.track_id),
                    "Value movement requires a single concrete track"
                );
                let anchor_time = projection.key(anchor).unwrap().time;
                let mut delta = if *time_delta == 0. {
                    0.
                } else {
                    f64::from(timeline_time(
                        anchor_time + time_delta,
                        projection.fps,
                        snap,
                    )?) - anchor_time
                };
                let lower = keys
                    .iter()
                    .map(|key| -projection.key(key).unwrap().time)
                    .fold(f64::NEG_INFINITY, f64::max);
                let upper = keys
                    .iter()
                    .map(|key| projection.duration - projection.key(key).unwrap().time)
                    .fold(f64::INFINITY, f64::min);
                ensure!(lower <= upper, "Selected key span exceeds motion duration");
                if *time_delta != 0. {
                    delta = delta.clamp(lower, upper);
                }
                let moves = keys
                    .iter()
                    .map(|key| {
                        let point = projection.key(key).unwrap();
                        let value = if *value_delta == 0. {
                            point.value
                        } else {
                            let range = projection
                                .channel(&projection.tracks[&key.track_id].channel)
                                .unwrap()
                                .range
                                .unwrap();
                            (f64::from(point.value) + value_delta)
                                .clamp(f64::from(range.0), f64::from(range.1))
                                as f32
                        };
                        MotionKeyMove {
                            key: key.sdk(),
                            point: MotionPoint {
                                time: (point.time + delta) as f32,
                                value,
                            },
                        }
                    })
                    .collect::<Vec<_>>();
                candidate.move_keys(model, &moves)?;
            }
            TimelineCommand::SetKey { key, time, value } => {
                validate_key(key)?;
                let original_time = projection.key(key).unwrap().time;
                // A value-only inspector edit must not quantize an imported key.
                let time = if *time == original_time {
                    original_time as f32
                } else {
                    timeline_time(*time, projection.fps, snap)?
                };
                candidate.move_keys(
                    model,
                    &[MotionKeyMove {
                        key: key.sdk(),
                        point: MotionPoint {
                            time,
                            value: *value,
                        },
                    }],
                )?;
            }
            TimelineCommand::Delete { keys, events } => {
                for key in keys {
                    validate_key(key)?;
                }
                candidate.delete_keys(
                    model,
                    &keys.iter().map(TimelineKeyRef::sdk).collect::<Vec<_>>(),
                )?;
                let mut clip = candidate.into_clip();
                for id in events {
                    ensure!(
                        clip.events.iter().any(|event| &event.id == id),
                        "Event no longer exists"
                    );
                }
                clip.events.retain(|event| !events.contains(&event.id));
                candidate = MotionClipCandidate::new(model, clip)?;
            }
            TimelineCommand::Interpolation { keys, kind } => {
                for key in keys {
                    validate_key(key)?;
                }
                candidate.set_interpolation(
                    model,
                    &keys.iter().map(TimelineKeyRef::sdk).collect::<Vec<_>>(),
                    *kind,
                )?;
            }
            TimelineCommand::Handle {
                end,
                handle,
                time,
                value,
            } => {
                validate_key(end)?;
                ensure!(
                    projection
                        .display_handles(end)
                        .is_some_and(|handles| handles.value_editable),
                    "Bezier handles are read-only for this segment"
                );
                ensure!(time.is_finite(), "Invalid handle time");
                candidate.set_handle(
                    model,
                    &end.sdk(),
                    *handle,
                    MotionPoint {
                        time: *time as f32,
                        value: *value,
                    },
                )?;
            }
            TimelineCommand::Event { id, time, value } => {
                let mut clip = candidate.into_clip();
                if let Some(id) = id {
                    let event = clip
                        .events
                        .iter_mut()
                        .find(|event| &event.id == id)
                        .ok_or_else(|| anyhow::anyhow!("Event no longer exists"))?;
                    if *time != f64::from(event.time) {
                        event.time = timeline_time(*time, projection.fps, snap)?;
                    }
                    event.value = value.clone();
                } else {
                    clip.events.push(MotionEvent {
                        id: self.event_id.clone(),
                        time: timeline_time(*time, projection.fps, snap)?,
                        value: value.clone(),
                        extensions: Default::default(),
                    });
                }
                candidate = MotionClipCandidate::new(model, clip)?;
            }
            TimelineCommand::ScaleTime(duration) => {
                ensure!(duration.is_finite(), "Invalid duration");
                candidate.scale_time(model, *duration as f32)?;
            }
            TimelineCommand::Timing {
                duration,
                fps,
                looping,
                fade_in,
                fade_out,
            } => {
                let mut clip = candidate.into_clip();
                clip.duration = *duration;
                clip.fps = *fps;
                clip.looping = *looping;
                clip.fade_in = *fade_in;
                clip.fade_out = *fade_out;
                candidate = MotionClipCandidate::new(model, clip)?;
            }
        }
        Ok(candidate)
    }

    #[allow(clippy::too_many_arguments)]
    fn write(
        &self,
        document: &crate::Document,
        projection: &TimelineProjection,
        candidate: &mut MotionClipCandidate,
        channel_id: &TimelineChannelId,
        track_id: Option<&str>,
        selected: Option<&TimelineKeyRef>,
        value: Option<f32>,
        snap: bool,
    ) -> Result<()> {
        let channel = projection
            .channel(channel_id)
            .ok_or_else(|| anyhow::anyhow!("Missing channel"))?;
        ensure!(channel.editable, "Channel is read-only");
        if let Some(value) = value {
            let (min, max) = channel.range.unwrap();
            ensure!(
                value.is_finite() && (min..=max).contains(&value),
                "Value outside channel range"
            );
        }
        let track_id = match track_id {
            Some(id) => {
                ensure!(
                    channel.tracks.iter().any(|track| track == id),
                    "Track belongs to another channel"
                );
                Some(id)
            }
            None => {
                ensure!(
                    channel.tracks.len() <= 1,
                    "Choose a concrete track for this duplicate target"
                );
                channel.tracks.first().map(String::as_str)
            }
        };
        let selected = if let Some(key) = selected {
            ensure!(
                key.clip_id == projection.clip_id && Some(key.track_id.as_str()) == track_id,
                "Selected key belongs to another track"
            );
            projection
                .key(key)
                .filter(|key| key.time == self.local_time)
        } else {
            None
        };
        let time = selected.map_or_else(
            || timeline_time(self.local_time, projection.fps, snap),
            |key| Ok(key.time as f32),
        )?;
        if let Some(track_id) = track_id {
            let track = &projection.tracks[track_id];
            if let Some(key) =
                selected.or_else(|| track.keys.iter().find(|key| key.time as f32 == time))
            {
                if let Some(value) = value {
                    candidate.move_keys(
                        document.model(),
                        &[MotionKeyMove {
                            key: key.reference.sdk(),
                            point: MotionPoint { time, value },
                        }],
                    )?;
                }
            } else {
                candidate.insert_key(document.model(), track_id, &self.key_id, time, value)?;
            }
        } else {
            let mut clip = candidate.clone().into_clip();
            clip.tracks.push(MotionTrack {
                id: self.track_id.clone(),
                target: channel_id.target(),
                initial: MotionKey {
                    id: self.key_id.clone(),
                    time,
                    value: value.unwrap_or(channel.default_value),
                },
                segments: Arc::new(vec![]),
                fade_in: None,
                fade_out: None,
                extensions: Default::default(),
            });
            ensure!(
                time >= 0. && time <= clip.duration,
                "New key is outside motion duration"
            );
            *candidate = MotionClipCandidate::new(document.model(), clip)?;
        }
        Ok(())
    }
}

impl DocumentSession {
    pub(crate) fn cancel_timeline_edit(&mut self) {
        if self.animation.timeline.edit.take().is_some() {
            if let Some(state) = self.animation.timeline.current_mut() {
                state.generation = 0;
            }
            self.animation.playback.set_candidate(None);
            self.reconcile_animation();
        }
    }
    pub(crate) fn finish_timeline_edit(&mut self) -> Result<()> {
        if let Some(edit) = self.animation.timeline.edit.take() {
            let result = self.document.commit_timeline_candidate(
                edit.candidate,
                edit.base_version,
                &edit.label,
            );
            self.animation.playback.set_candidate(None);
            if let Some(state) = self.animation.timeline.current_mut() {
                state.generation = 0;
            }
            self.reconcile_animation();
            result?;
        }
        Ok(())
    }
}

impl Editor {
    pub fn timeline_projection(&self) -> Option<Arc<TimelineProjection>> {
        self.active_session()?
            .animation
            .timeline
            .current()
            .map(|state| state.projection.clone())
    }
    pub fn timeline_view(&self) -> Option<&TimelineViewState> {
        Some(&self.active_session()?.animation.timeline.current()?.view)
    }
    /// Share the cached visible layout with UI render closures without cloning keys.
    pub fn timeline_visible_rows(&self) -> Option<Arc<[TimelineRow]>> {
        Some(
            self.active_session()?
                .animation
                .timeline
                .current()?
                .visible
                .clone(),
        )
    }
    pub fn timeline_rows(&self) -> Option<&[TimelineRow]> {
        Some(&self.active_session()?.animation.timeline.current()?.visible)
    }
    /// Acknowledge a UI reveal without ending the candidate that requested it.
    pub fn acknowledge_timeline_reveal(
        &mut self,
        document: DocumentId,
        row: &TimelineRowId,
        scroll: f64,
    ) -> Result<()> {
        self.ensure_animation_target(document)?;
        ensure!(
            scroll.is_finite() && scroll >= 0.,
            "Invalid timeline scroll"
        );
        if let Some(state) = self
            .active_session_mut()
            .unwrap()
            .animation
            .timeline
            .current_mut()
        {
            if state.view.reveal_row.as_ref() == Some(row) {
                state.view.scroll = scroll;
                state.view.reveal_row = None;
            }
        }
        Ok(())
    }
    pub fn timeline_edit(&self) -> Option<&TimelineEdit> {
        self.active_session()?.animation.timeline.edit.as_ref()
    }
    /// Delayed panel callbacks may only cancel the candidate they started.
    pub fn cancel_timeline_edit_for(
        &mut self,
        document: DocumentId,
        token: TimelineCandidateIdentity,
    ) -> bool {
        if self.active_id() != Some(document)
            || !self
                .timeline_edit()
                .is_some_and(|edit| edit.identity.edit == token.edit)
        {
            return false;
        }
        self.cancel_timeline_edit();
        true
    }
    pub fn cancel_timeline_edit(&mut self) {
        if let Some(session) = self.active_session_mut() {
            session.cancel_timeline_edit();
        }
    }
    pub fn update_timeline_view(
        &mut self,
        document: DocumentId,
        update: impl FnOnce(&mut TimelineViewState),
    ) -> Result<()> {
        self.ensure_animation_target(document)?;
        let state = self
            .active_session()
            .unwrap()
            .animation
            .timeline
            .current()
            .ok_or_else(|| anyhow::anyhow!("Select a motion"))?;
        let mut view = state.view.clone();
        update(&mut view);
        ensure!(
            view.time_range.0.is_finite()
                && view.time_range.1.is_finite()
                && view.time_range.0 < view.time_range.1
                && view.scroll.is_finite()
                && view.scroll >= 0.,
            "Invalid timeline view"
        );
        self.cancel_timeline_edit();
        let state = self
            .active_session_mut()
            .unwrap()
            .animation
            .timeline
            .current_mut()
            .unwrap();
        let visibility_changed = state.view.collapsed != view.collapsed
            || state.view.search != view.search
            || state.view.filter != view.filter;
        state.view = view;
        if visibility_changed {
            state.refresh_visible();
        } else {
            state.retain_selection();
        }
        Ok(())
    }
    /// Camera motion is allowed during a candidate gesture and never changes selection.
    pub fn navigate_timeline_view(
        &mut self,
        document: DocumentId,
        time_range: (f64, f64),
        scroll: f64,
    ) -> Result<()> {
        self.ensure_animation_target(document)?;
        ensure!(
            time_range.0.is_finite()
                && time_range.0 >= 0.
                && time_range.1.is_finite()
                && time_range.0 < time_range.1
                && scroll.is_finite()
                && scroll >= 0.,
            "Invalid timeline navigation"
        );
        let state = self
            .active_session_mut()
            .unwrap()
            .animation
            .timeline
            .current_mut()
            .ok_or_else(|| anyhow::anyhow!("Select a motion"))?;
        state.view.time_range = time_range;
        state.view.scroll = scroll;
        Ok(())
    }
    pub fn create_timeline_motion(&mut self, document: DocumentId) -> Result<String> {
        self.ensure_animation_target(document)?;
        self.cancel_timeline_edit();
        let id = self
            .active_session_mut()
            .unwrap()
            .document
            .create_timeline_clip()?;
        self.select_motion(
            document,
            MotionSelection {
                clip_id: id.clone(),
                entry: None,
            },
        )?;
        Ok(id)
    }
    pub fn begin_timeline_edit(
        &mut self,
        document: DocumentId,
        label: impl Into<String>,
    ) -> Result<TimelineCandidateIdentity> {
        self.ensure_animation_target(document)?;
        ensure!(
            !self.animation().unwrap().playing,
            "Pause before editing the timeline"
        );
        self.cancel_timeline_edit();
        self.pause_animation();
        let session = self.active_session_mut().unwrap();
        session.document.finish_edit()?;
        session.reconcile_animation();
        if !session.animation.playback.head_located {
            if let Some(frame) = &session.animation.playback.presented {
                session.animation.playback.time = frame.time;
                session.animation.playback.edit_time = session
                    .animation
                    .playback
                    .motion
                    .as_ref()
                    .and_then(|motion| {
                        frame
                            .motion_positions
                            .iter()
                            .find(|position| position.motion_id == motion.clip_id)
                    })
                    .map_or(frame.time, |position| position.local_time);
            }
        }
        if session.animation.playback.preview_mode == AnimationPreviewMode::Editing {
            session.animation.playback.time = session.animation.playback.edit_time;
        }
        session.animation.playback.head_located = true;
        let clip_id = session
            .animation
            .timeline
            .active
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Select a motion"))?;
        let original = session.document.motion_definition(clip_id).unwrap().clone();
        let candidate = MotionClipCandidate::new(session.document.model(), original.clone())?;
        let original_projection = session
            .animation
            .timeline
            .current()
            .unwrap()
            .projection
            .clone();
        let source = Arc::new(session.document.animation_source());
        session.animation.timeline.next_edit += 1;
        let identity = TimelineCandidateIdentity {
            edit: session.animation.timeline.next_edit,
            revision: 0,
        };
        let original_keys = original
            .tracks
            .iter()
            .flat_map(|track| track.keys().map(|key| key.id.clone()))
            .collect();
        session.animation.timeline.edit = Some(TimelineEdit {
            identity,
            base_version: source.snapshot.version(),
            original_candidate: candidate.clone(),
            original_projection,
            original_keys,
            source,
            candidate,
            local_time: f64::from(session.animation.playback.edit_time),
            target: None,
            track_id: uuid::Uuid::new_v4().to_string(),
            key_id: uuid::Uuid::new_v4().to_string(),
            event_id: uuid::Uuid::new_v4().to_string(),
            label: label.into(),
        });
        session.animation.playback.set_candidate(Some(identity));
        Ok(identity)
    }
    /// Keep the gesture token returned by begin across updates. Only its edit ID
    /// is matched here; the returned revision identifies each immutable candidate.
    pub fn update_timeline_edit(
        &mut self,
        document: DocumentId,
        gesture: TimelineCandidateIdentity,
        command: TimelineCommand,
    ) -> Result<TimelineCandidateIdentity> {
        self.ensure_animation_target(document)?;
        let session = self.active_session_mut().unwrap();
        let snap = session
            .animation
            .timeline
            .current()
            .ok_or_else(|| anyhow::anyhow!("Select a motion"))?
            .view
            .snap;
        let previous_projection = session
            .animation
            .timeline
            .current()
            .unwrap()
            .projection
            .clone();
        let edit = session
            .animation
            .timeline
            .edit
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("No timeline edit"))?;
        ensure!(
            edit.identity.edit == gesture.edit,
            "Timeline gesture changed"
        );
        ensure!(
            edit.base_version.revision == session.document.model_generation(),
            "Timeline edit is stale"
        );
        let target = command.target();
        ensure!(
            edit.target
                .as_ref()
                .is_none_or(|expected| expected == &target),
            "Gesture target changed"
        );
        let candidate = edit.apply(&session.document, &command, snap)?;
        edit.target = Some(target);
        if candidate != edit.candidate {
            let source = Arc::new(AnimationSource {
                snapshot: edit.source.snapshot.clone(),
                textures: edit.source.textures.clone(),
                candidate: Some(Arc::new(candidate.clone())),
                materialized: Default::default(),
            });
            let incremental = matches!(command, TimelineCommand::WriteValue { .. })
                .then(|| {
                    previous_projection.with_value_changes(edit.candidate.clip(), candidate.clip())
                })
                .flatten();
            let projection = Arc::new(match incremental {
                Some(projection) => projection,
                None => TimelineProjection::from_clip(&session.document, candidate.clip())?,
            });
            let same_rows = Arc::ptr_eq(&previous_projection.rows, &projection.rows);
            let added: Vec<_> = if same_rows {
                vec![]
            } else {
                projection
                    .tracks
                    .values()
                    .flat_map(|track| &track.keys)
                    .filter(|key| !edit.original_keys.contains(key.reference.key_id.as_str()))
                    .map(|key| key.reference.clone())
                    .collect()
            };
            edit.candidate = candidate;
            edit.identity.revision += 1;
            session
                .animation
                .playback
                .set_candidate(Some(edit.identity));
            session.animation.playback.source = Some(source);
            let state = session.animation.timeline.current_mut().unwrap();
            state.projection = projection;
            // Value changes do not change filtering, hierarchy or selection.
            if !same_rows {
                if added.is_empty() {
                    state.refresh_visible();
                } else {
                    state.reveal(&added);
                }
            }
        }
        Ok(session.animation.timeline.edit.as_ref().unwrap().identity)
    }
    pub fn finish_timeline_edit(
        &mut self,
        document: DocumentId,
        gesture: TimelineCandidateIdentity,
    ) -> Result<()> {
        self.ensure_animation_target(document)?;
        ensure!(
            self.timeline_edit()
                .is_some_and(|edit| edit.identity.edit == gesture.edit),
            "Timeline gesture changed"
        );
        self.active_session_mut().unwrap().finish_timeline_edit()
    }
    pub fn execute_timeline(
        &mut self,
        document: DocumentId,
        command: TimelineCommand,
    ) -> Result<()> {
        let gesture = self.begin_timeline_edit(document, "Edit motion")?;
        if let Err(error) = self.update_timeline_edit(document, gesture, command) {
            self.cancel_timeline_edit();
            return Err(error);
        }
        self.finish_timeline_edit(document, gesture)
    }
    pub fn delete_timeline_selection(&mut self, document: DocumentId) -> Result<()> {
        self.ensure_animation_target(document)?;
        let view = self
            .timeline_view()
            .ok_or_else(|| anyhow::anyhow!("Select a motion"))?;
        let command = TimelineCommand::Delete {
            keys: view.keys.iter().cloned().collect(),
            events: view.events.iter().cloned().collect(),
        };
        if view.keys.is_empty() && view.events.is_empty() {
            return Ok(());
        }
        self.execute_timeline(document, command)
    }
}
