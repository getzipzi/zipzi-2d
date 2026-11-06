//! Persistent motion clips. Playback time and queue state live outside Document.
use std::collections::{BTreeMap, HashSet};

use serde::Serialize;

use super::{Document, StructureIssue};
use crate::types::{ChangeKind, EditResult, Status};
mod authoring;
#[cfg(test)]
mod validation_tests;
pub(super) use authoring::MotionLookup;
pub use authoring::{
    DisabledMotionTrack, MotionContainer, MotionDisabledReason, MotionEdit, MotionPlan,
    MotionProjection,
};

pub use zipzi_contract::document::{
    MotionKey, MotionPoint, MotionSegment, MotionTrack, MotionTrackTarget,
};
pub trait MotionTrackEditing {
    fn retain_keys(&mut self, retain: impl Fn(&str) -> bool, restricted: bool) -> bool;
}
impl MotionTrackEditing for MotionTrack {
    /// Retain stable keys using final surviving neighbours. Returns false when
    /// no keys remain, so the owning clip can retire the whole track.
    fn retain_keys(&mut self, retain: impl Fn(&str) -> bool, restricted: bool) -> bool {
        let original = self.clone();
        let all: Vec<_> = original.keys().collect();
        let survivors: Vec<_> = all
            .iter()
            .enumerate()
            .filter_map(|(i, k)| retain(&k.id).then_some(i))
            .collect();
        let Some(&first) = survivors.first() else {
            return false;
        };
        self.initial = all[first].clone();
        let mut segments = Vec::with_capacity(survivors.len() - 1);
        for pair in survivors.windows(2) {
            let [left, right] = [pair[0], pair[1]];
            if right == left + 1 {
                segments.push(original.segments[left].clone());
                continue;
            }
            let span = &original.segments[left..right];
            let end = all[right].clone();
            let merged = if span
                .iter()
                .all(|s| matches!(s, MotionSegment::Stepped { .. }))
            {
                MotionSegment::Stepped { end }
            } else if span
                .iter()
                .all(|s| matches!(s, MotionSegment::InverseStepped { .. }))
            {
                MotionSegment::InverseStepped { end }
            } else if span
                .iter()
                .all(|s| matches!(s, MotionSegment::Bezier { .. }))
            {
                let MotionSegment::Bezier { control1, .. } = span.first().unwrap() else {
                    unreachable!()
                };
                let MotionSegment::Bezier { control2, .. } = span.last().unwrap() else {
                    unreachable!()
                };
                let (mut first, mut second) = (*control1, *control2);
                if restricted {
                    let span = f64::from(end.time) - f64::from(all[left].time);
                    first.time = (f64::from(all[left].time) + span / 3.) as f32;
                    second.time = (f64::from(end.time) - span / 3.) as f32;
                } else {
                    first.time = first.time.clamp(all[left].time, end.time);
                    second.time = second.time.clamp(first.time, end.time);
                }
                MotionSegment::Bezier {
                    control1: first,
                    control2: second,
                    end,
                }
            } else {
                MotionSegment::Linear { end }
            };
            segments.push(merged);
        }
        self.segments = std::sync::Arc::new(segments);
        true
    }
}

pub use zipzi_contract::document::{
    MotionClip, MotionEvent, MotionGroup, MotionRegistration, MotionSourceRange,
};
const SOURCE_RANGE: &str = "ZipziSourceKeyRange";
pub trait MotionClipEditing {
    fn preserve_source_key_range(&mut self);
}
impl MotionClipEditing for MotionClip {
    fn preserve_source_key_range(&mut self) {
        let start = self
            .tracks
            .iter()
            .flat_map(|t| t.keys())
            .map(|k| k.time)
            .fold(0., f32::min);
        let end = self
            .tracks
            .iter()
            .flat_map(|t| t.keys())
            .map(|k| k.time)
            .fold(self.duration, f32::max);
        if start < 0. || end > self.duration {
            self.meta_extensions.insert(
                SOURCE_RANGE.into(),
                serde_json::to_value(MotionSourceRange { start, end })
                    .expect("finite validated source keys"),
            );
        }
    }
}
impl Document {
    pub fn motion_groups(&self) -> &[MotionGroup] {
        &self.content.motion_groups
    }

    pub fn set_motion_groups(&mut self, groups: Vec<MotionGroup>) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        self.set_motion_library(groups, self.content.motion_roots.clone())
    }
    pub fn set_motion_library(
        &mut self,
        groups: Vec<MotionGroup>,
        roots: Vec<MotionRegistration>,
    ) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        for (kind, id) in groups.iter().map(|g| ("group", &g.id)).chain(
            groups
                .iter()
                .flat_map(|g| &g.entries)
                .chain(&roots)
                .map(|e| ("registration", &e.id)),
        ) {
            if self.contains_id(id)
                && match kind {
                    "group" => self.get_motion_group(id).is_none(),
                    _ => self.get_motion_registration(id).is_none(),
                }
            {
                return self.failed(Status::error("DUPLICATE_ID", id));
            }
        }
        let mut candidate = self.motion_authoring_candidate();
        candidate.content.motion_groups = groups;
        candidate.content.motion_roots = roots;
        for id in self.motion_order() {
            if !candidate.motion_registrations().any(|e| e.clip_id == *id) {
                if let Some(entry) = self
                    .motion_roots()
                    .iter()
                    .chain(self.motion_groups().iter().flat_map(|g| &g.entries))
                    .find(|e| e.clip_id == *id)
                {
                    candidate.content.motion_roots.push(entry.clone());
                }
            }
        }
        candidate.ensure_motion_roots();
        let status = candidate.validate_motion_groups(candidate.motion_groups());
        if !status.is_ok() {
            return self.failed(status);
        }
        if let Err(status) = self.validate_motion_changes_unlocked(&candidate) {
            return self.failed(status);
        }
        if self.content.motion_groups == candidate.content.motion_groups
            && self.content.motion_roots == candidate.content.motion_roots
        {
            return self.failed(Status::ok());
        }
        let ids = self
            .motion_groups()
            .iter()
            .map(|g| g.id.clone())
            .chain(candidate.motion_groups().iter().map(|g| g.id.clone()))
            .chain(self.motion_registrations().map(|e| e.id.clone()))
            .chain(candidate.motion_registrations().map(|e| e.id.clone()))
            .collect();
        let flagged: Vec<_> = self
            .editor_state()
            .objects
            .keys()
            .filter(|id| {
                self.get_motion_group(id).is_some() || self.get_motion_registration(id).is_some()
            })
            .cloned()
            .collect();
        self.content.motion_groups = candidate.content.motion_groups;
        self.content.motion_roots = candidate.content.motion_roots;
        for id in flagged {
            if !self.motion_identity_exists(&id) {
                self.content.editor_state.objects.remove(&id);
            }
        }
        self.changed(ChangeKind::Metadata, vec![], ids)
    }
    pub(super) fn validate_motion_groups(&self, groups: &[MotionGroup]) -> Status {
        self.validation_model().validate_motion_groups(groups)
    }
    pub fn motion_order(&self) -> &[String] {
        &self.content.motion_order
    }

    /// Share an immutable clip across host reads without retaining a session borrow.
    pub fn shared_motion(&self, id: &str) -> Option<std::sync::Arc<MotionClip>> {
        self.content.motions.get(id).cloned()
    }

    pub fn get_motion(&self, id: &str) -> Option<&MotionClip> {
        self.content.motions.get(id).map(std::sync::Arc::as_ref)
    }

    pub fn create_motion(&mut self, clip: MotionClip) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.initialized() {
            return self.failed(Status::error("NOT_INITIALIZED", "Initialize first"));
        }
        if self.contains_id(&clip.id) {
            return self.failed(Status::error("DUPLICATE_ID", &clip.id));
        }
        if let Err(status) = self.validate_motion_unlocked(&clip) {
            return self.failed(status);
        }
        let status = self.validate_motion(&clip);
        if !status.is_ok() {
            return self.failed(status);
        }
        let id = clip.id.clone();
        self.content
            .motions
            .insert(id.clone(), std::sync::Arc::new(clip));
        self.content.motion_order.push(id.clone());
        self.ensure_motion_roots();
        self.changed(ChangeKind::Metadata, Vec::new(), vec![id])
    }

    pub fn replace_motion(&mut self, clip: MotionClip) -> EditResult {
        if self.mutation_blocked() {
            return self.failed(Status::error(
                "TRANSACTION_ACTIVE",
                "Commit or cancel first",
            ));
        }
        if !self.content.motions.contains_key(&clip.id) {
            return self.failed(Status::error("MISSING_MOTION", &clip.id));
        }
        if let Err(status) = self.validate_motion_unlocked(&clip) {
            return self.failed(status);
        }
        let status = self.validate_motion(&clip);
        if !status.is_ok() {
            return self.failed(status);
        }
        let id = clip.id.clone();
        if self.get_motion(&id) == Some(&clip) {
            return self.failed(Status::ok());
        }
        let flagged: Vec<_> = self
            .editor_state()
            .objects
            .keys()
            .filter(|key| {
                self.get_motion_track(key).is_some_and(|(c, _)| c.id == id)
                    || self.get_motion_key(key).is_some_and(|(c, _, _)| c.id == id)
                    || self.get_motion_event(key).is_some_and(|(c, _)| c.id == id)
            })
            .cloned()
            .collect();
        self.content
            .motions
            .insert(id.clone(), std::sync::Arc::new(clip));
        self.motion_lookup.take();
        for key in flagged {
            if !self.motion_identity_exists(&key) {
                self.content.editor_state.objects.remove(&key);
            }
        }
        self.changed(ChangeKind::Metadata, Vec::new(), vec![id])
    }

    /// Validate a complete candidate without changing the document or history.
    pub fn validate_motion(&self, clip: &MotionClip) -> Status {
        self.validation_model().validate_motion(clip)
    }

    /// Scalar edits retain track identities, points and source metadata. Only
    /// changing duration/FPS can invalidate existing key or event time bounds.
    pub(super) fn patch_motion_properties(
        &mut self,
        id: &str,
        duration: f32,
        fps: f32,
        fade_in: Option<f32>,
        fade_out: Option<f32>,
    ) -> EditResult {
        #[cfg(feature = "bench-instrumentation")]
        let _span = crate::benchmark::span("core.patch_motion_properties");
        let status = validate_properties(id, duration, fps, fade_in, fade_out);
        if !status.is_ok() {
            return self.failed(status);
        }
        if let Err(status) = self.motion_unlocked(id) {
            return self.failed(status);
        }
        let clip = self.get_motion(id).expect("property target checked");
        if (clip.duration, clip.fps, clip.fade_in, clip.fade_out)
            == (duration, fps, fade_in, fade_out)
        {
            return self.failed(Status::ok());
        }
        if clip.duration != duration || clip.fps != fps {
            #[cfg(feature = "bench-instrumentation")]
            let _span = crate::benchmark::span("core.validate_motion_timing");
            let (minimum, maximum) = match time_range(clip, duration) {
                Ok(range) => range,
                Err(status) => return self.failed(status),
            };
            let tolerance = (0.5 / fps).max(0.0001);
            for (index, track) in clip.tracks.iter().enumerate() {
                if track.initial.time < minimum || track.initial.time > maximum {
                    return self.failed(Status::error(
                        "INVALID_MOTION_POINT",
                        format!("{id}.tracks[{index}].initial"),
                    ));
                }
                // End times are already ordered and validated, so the last
                // endpoint bounds every segment in this immutable track.
                if track
                    .segments
                    .last()
                    .is_some_and(|s| s.end().time > maximum + tolerance)
                {
                    return self.failed(Status::error(
                        "INVALID_MOTION_POINT",
                        format!("{id}.tracks[{index}].segments"),
                    ));
                }
            }
            if clip.events.iter().any(|event| event.time > duration) {
                return self.failed(Status::error("INVALID_MOTION_EVENT_TIME", id));
            }
        }
        let clip = std::sync::Arc::make_mut(self.content.motions.get_mut(id).unwrap());
        clip.duration = duration;
        clip.fps = fps;
        clip.fade_in = fade_in;
        clip.fade_out = fade_out;
        self.changed(ChangeKind::Metadata, Vec::new(), vec![id.to_owned()])
    }

    pub(super) fn validate_motions(&self) -> Vec<StructureIssue> {
        let mut issues = Vec::new();
        for id in &self.content.motion_order {
            if let Some(clip) = self.content.motions.get(id) {
                let status = self.validate_motion(clip);
                if !status.is_ok() {
                    issues.push(StructureIssue {
                        object_id: id.clone(),
                        status,
                    });
                }
            }
        }
        let status = self.validate_motion_groups(&self.content.motion_groups);
        if !status.is_ok() {
            issues.push(StructureIssue {
                object_id: self.content.id.clone(),
                status,
            });
        }
        issues
    }
}

use crate::model_validation::motions::{time_range, validate_properties};
