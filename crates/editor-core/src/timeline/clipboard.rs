use super::*;
use anyhow::{Result, ensure};
use kasane_core::document::{MotionClip, MotionKey, MotionSegment, MotionTrack};
use kasane_sdk::MotionClipCandidate;

#[derive(Debug, Clone)]
pub struct TimelinePasteTarget {
    pub channel: TimelineChannelId,
    pub track_id: Option<String>,
}

#[derive(Clone)]
struct CopiedKey {
    key: MotionKey,
    incoming: Option<MotionSegment>,
}
#[derive(Clone)]
struct CopiedTrack {
    id: String,
    channel: TimelineChannelId,
    keys: Vec<CopiedKey>,
}
#[derive(Clone)]
pub(crate) struct TimelineClipboard {
    document: crate::DocumentId,
    clip_id: String,
    anchor: f64,
    tracks: Vec<CopiedTrack>,
}

impl TimelineClipboard {
    fn capture(
        document: crate::DocumentId,
        clip: &MotionClip,
        selection: &BTreeSet<TimelineKeyRef>,
    ) -> Result<Self> {
        ensure!(!selection.is_empty(), "Select real keys to copy");
        let mut tracks = Vec::new();
        let mut found = 0;
        let mut anchor = f64::INFINITY;
        for track in &clip.tracks {
            let mut keys = Vec::new();
            let mut previous = None;
            for (index, key) in track.keys().enumerate() {
                let reference = TimelineKeyRef {
                    clip_id: clip.id.clone(),
                    track_id: track.id.clone(),
                    key_id: key.id.clone(),
                };
                if selection.contains(&reference) {
                    found += 1;
                    anchor = anchor.min(f64::from(key.time));
                    keys.push(CopiedKey {
                        key: key.clone(),
                        incoming: (previous == index.checked_sub(1) && index > 0)
                            .then(|| track.segments[index - 1].clone()),
                    });
                    previous = Some(index);
                }
            }
            if !keys.is_empty() {
                tracks.push(CopiedTrack {
                    id: track.id.clone(),
                    channel: TimelineChannelId::from_target(&track.target),
                    keys,
                });
            }
        }
        ensure!(found == selection.len(), "Selection contains missing keys");
        Ok(Self {
            document,
            clip_id: clip.id.clone(),
            anchor,
            tracks,
        })
    }

    fn paste(
        &self,
        document_id: crate::DocumentId,
        document: &crate::Document,
        projection: &TimelineProjection,
        time: f64,
        snap: bool,
        target: Option<&TimelinePasteTarget>,
    ) -> Result<(MotionClipCandidate, Vec<TimelineKeyRef>)> {
        ensure!(
            document_id == self.document && projection.clip_id == self.clip_id,
            "Cross-document and cross-clip paste are unavailable"
        );
        ensure!(
            target.is_none() || self.tracks.len() == 1,
            "Multiple copied tracks paste to their original targets"
        );
        let mut clip = document.motion_definition(&self.clip_id).unwrap().clone();
        MotionClipCandidate::new(document.model(), clip.clone())?;
        let anchor = f64::from(timeline_time(time, projection.fps, snap)?);
        let mut selected = Vec::new();
        for copied in &self.tracks {
            ensure!(
                !matches!(&copied.channel, TimelineChannelId::Unresolved { .. })
                    && !matches!(&copied.channel, TimelineChannelId::Model(id) if id != "Opacity"),
                "Copied target type is read-only"
            );
            let channel_id = target.map_or(&copied.channel, |target| &target.channel);
            let channel = projection
                .channel(channel_id)
                .ok_or_else(|| anyhow::anyhow!("Paste target no longer exists"))?;
            ensure!(channel.editable, "Paste target is read-only");
            ensure!(
                std::mem::discriminant(channel_id) == std::mem::discriminant(&copied.channel),
                "Incompatible paste target type"
            );
            let requested = target.and_then(|target| target.track_id.as_deref());
            let target_id = if let Some(id) = requested {
                ensure!(
                    channel.tracks.iter().any(|track| track == id),
                    "Target track belongs to another channel"
                );
                id.to_owned()
            } else if target.is_none() && channel.tracks.contains(&copied.id) {
                copied.id.clone()
            } else if target.is_some() && channel.tracks.len() == 1 {
                channel.tracks[0].clone()
            } else {
                ensure!(
                    channel.tracks.is_empty()
                        && matches!(channel_id, TimelineChannelId::Parameter(_)),
                    "Choose an existing concrete target track"
                );
                uuid::Uuid::new_v4().to_string()
            };
            let mut incoming = Vec::new();
            for key in &copied.keys {
                let value = key.key.value;
                let (min, max) = channel.range.unwrap();
                ensure!(
                    (min..=max).contains(&value),
                    "Pasted value is outside the target range"
                );
                let time = (f64::from(key.key.time) - self.anchor + anchor) as f32;
                ensure!(
                    time.is_finite() && time >= 0. && time <= clip.duration,
                    "Pasted key is outside motion duration"
                );
                let key = MotionKey {
                    id: uuid::Uuid::new_v4().to_string(),
                    time,
                    value,
                };
                selected.push(TimelineKeyRef {
                    clip_id: clip.id.clone(),
                    track_id: target_id.clone(),
                    key_id: key.id.clone(),
                });
                incoming.push(key);
            }
            ensure!(
                incoming.windows(2).all(|keys| keys[0].time < keys[1].time),
                "Pasted times collide at f32 precision"
            );
            let existing = clip.tracks.iter().position(|track| track.id == target_id);
            let original = existing.map(|index| clip.tracks[index].clone());
            let old_keys: Vec<_> = original
                .as_ref()
                .map_or_else(Vec::new, |track| track.keys().cloned().collect());
            let mut merged: Vec<_> = old_keys
                .iter()
                .enumerate()
                .map(|(i, key)| (key.clone(), false, i))
                .chain(
                    incoming
                        .iter()
                        .enumerate()
                        .map(|(i, key)| (key.clone(), true, i)),
                )
                .collect();
            merged.sort_by(|a, b| a.0.time.total_cmp(&b.0.time));
            ensure!(
                merged
                    .windows(2)
                    .all(|keys| keys[0].0.time < keys[1].0.time),
                "Paste collides with an existing key on track {target_id}"
            );
            let mut segments = Vec::new();
            for pair in merged.windows(2) {
                let (left, left_new, left_index) = &pair[0];
                let (right, right_new, right_index) = &pair[1];
                let segment = if !left_new && !right_new && *right_index == left_index + 1 {
                    original.as_ref().unwrap().segments[*left_index].clone()
                } else if *left_new && *right_new && *right_index == left_index + 1 {
                    if let Some(segment) = &copied.keys[*right_index].incoming {
                        let mut segment = segment.clone();
                        *segment.end_mut() = right.clone();
                        if let MotionSegment::Bezier {
                            control1, control2, ..
                        } = &mut segment
                        {
                            for point in [control1, control2] {
                                point.time = (f64::from(point.time) - self.anchor + anchor) as f32;
                            }
                            if clip.restricted_beziers {
                                let MotionSegment::Bezier {
                                    control1, control2, ..
                                } = &mut segment
                                else {
                                    unreachable!()
                                };
                                let span = f64::from(right.time) - f64::from(left.time);
                                control1.time = (f64::from(left.time) + span / 3.) as f32;
                                control2.time = (f64::from(right.time) - span / 3.) as f32;
                            }
                        }
                        segment
                    } else {
                        MotionSegment::Linear { end: right.clone() }
                    }
                } else {
                    MotionSegment::Linear { end: right.clone() }
                };
                segments.push(segment);
            }
            let mut track = original.unwrap_or_else(|| MotionTrack {
                id: target_id.clone(),
                target: channel_id.target(),
                initial: merged[0].0.clone(),
                segments: Arc::new(vec![]),
                fade_in: None,
                fade_out: None,
                extensions: Default::default(),
            });
            track.initial = merged[0].0.clone();
            track.segments = Arc::new(segments);
            if let Some(index) = existing {
                clip.tracks[index] = track;
            } else {
                clip.tracks.push(track);
            }
        }
        Ok((MotionClipCandidate::new(document.model(), clip)?, selected))
    }
}

impl crate::Editor {
    pub(crate) fn timeline_clipboard_available(&self) -> bool {
        self.active_session().is_some_and(|session| {
            session
                .animation
                .timeline
                .clipboard
                .as_ref()
                .is_some_and(|clipboard| {
                    Some(clipboard.document) == self.active_id()
                        && session
                            .animation
                            .timeline
                            .current()
                            .is_some_and(|state| state.projection.clip_id == clipboard.clip_id)
                })
        })
    }

    /// Only a single copied track may be redirected to the selected channel.
    pub fn timeline_clipboard_can_retarget(&self) -> bool {
        self.active_session()
            .and_then(|session| session.animation.timeline.clipboard.as_ref())
            .is_some_and(|clipboard| clipboard.tracks.len() == 1)
    }

    pub fn copy_timeline_selection(&mut self, document: crate::DocumentId) -> Result<usize> {
        self.ensure_animation_target(document)?;
        self.cancel_timeline_edit();
        let session = self.active_session_mut().unwrap();
        let state = session
            .animation
            .timeline
            .current()
            .ok_or_else(|| anyhow::anyhow!("Select a motion"))?;
        let clipboard = TimelineClipboard::capture(
            document,
            session
                .document
                .motion_definition(&state.projection.clip_id)
                .unwrap(),
            &state.view.keys,
        )?;
        let count = clipboard.tracks.iter().map(|track| track.keys.len()).sum();
        session.animation.timeline.clipboard = Some(clipboard);
        Ok(count)
    }
    pub fn paste_timeline(
        &mut self,
        document: crate::DocumentId,
        target: Option<TimelinePasteTarget>,
    ) -> Result<()> {
        self.ensure_animation_target(document)?;
        self.cancel_timeline_edit();
        let session = self.active_session().unwrap();
        let clipboard = session
            .animation
            .timeline
            .clipboard
            .clone()
            .ok_or_else(|| anyhow::anyhow!("Timeline clipboard is empty"))?;
        let state = session
            .animation
            .timeline
            .current()
            .ok_or_else(|| anyhow::anyhow!("Select a motion"))?;
        let (candidate, keys) = clipboard.paste(
            document,
            &session.document,
            &state.projection,
            f64::from(session.animation.playback.edit_time),
            state.view.snap,
            target.as_ref(),
        )?;
        let gesture = self.begin_timeline_edit(document, "Paste motion keys")?;
        self.active_session_mut()
            .unwrap()
            .animation
            .timeline
            .edit
            .as_mut()
            .unwrap()
            .candidate = candidate;
        self.finish_timeline_edit(document, gesture)?;
        self.reveal_timeline_keys(document, &keys)
    }
    pub fn reveal_timeline_keys(
        &mut self,
        document: crate::DocumentId,
        keys: &[TimelineKeyRef],
    ) -> Result<()> {
        self.ensure_animation_target(document)?;
        let state = self
            .active_session_mut()
            .unwrap()
            .animation
            .timeline
            .current_mut()
            .ok_or_else(|| anyhow::anyhow!("Select a motion"))?;
        ensure!(
            keys.iter().all(|key| state.projection.key(key).is_some()),
            "Keys no longer exist"
        );
        state.reveal(keys);
        Ok(())
    }
}
