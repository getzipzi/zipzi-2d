//! Persistent motion clips. Playback time and queue state live outside Document.
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::{Document, StructureIssue};
use crate::types::{ChangeKind, EditResult, Status};
mod authoring;
pub(super) use authoring::MotionLookup;
pub use authoring::{
    DisabledMotionTrack, MotionContainer, MotionDisabledReason, MotionEdit, MotionPlan,
    MotionProjection,
};

#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Serialize,
    Deserialize,
    zipzi_api_derive::ApiShape,
    zipzi_api_derive::BinaryWire,
)]
pub struct MotionPoint {
    /// Absolute time in seconds within the owning motion clip.
    pub time: f32,
    /// Channel value or replacement payload in the domain declared by this field type.
    pub value: f32,
}

/// Persistent identity belongs to keys, never to Bezier control points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MotionKey {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Absolute time in seconds within the owning motion clip.
    pub time: f32,
    /// Channel value or replacement payload in the domain declared by this field type.
    pub value: f32,
}

impl MotionKey {
    pub fn point(&self) -> MotionPoint {
        MotionPoint {
            time: self.time,
            value: self.value,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum MotionSegment {
    Linear {
        /// Segment endpoint key or source-range end time, as declared by the enclosing type.
        end: MotionKey,
    },
    Bezier {
        /// First Bezier control point in seconds/channel-value coordinates; it has no key identity.
        control1: MotionPoint,
        /// Second Bezier control point in seconds/channel-value coordinates; it has no key identity.
        control2: MotionPoint,
        /// Segment endpoint key or source-range end time, as declared by the enclosing type.
        end: MotionKey,
    },
    Stepped {
        /// Segment endpoint key or source-range end time, as declared by the enclosing type.
        end: MotionKey,
    },
    InverseStepped {
        /// Segment endpoint key or source-range end time, as declared by the enclosing type.
        end: MotionKey,
    },
}

impl MotionSegment {
    pub fn end(&self) -> &MotionKey {
        match self {
            Self::Linear { end }
            | Self::Bezier { end, .. }
            | Self::Stepped { end }
            | Self::InverseStepped { end } => end,
        }
    }

    pub fn end_mut(&mut self) -> &mut MotionKey {
        match self {
            Self::Linear { end }
            | Self::Bezier { end, .. }
            | Self::Stepped { end }
            | Self::InverseStepped { end } => end,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub enum MotionTrackTarget {
    Unspecified,
    Model {
        /// Imported/exported runtime label, distinct from the document object UUID.
        runtime_id: String,
    },
    Parameter {
        /// Stable document parameter UUID; evaluation uses its declared range and default.
        parameter_id: String,
    },
    PartOpacity {
        /// Stable document Part UUID for this opacity or visibility reference.
        part_id: String,
    },
    Unresolved {
        /// Imported Physics target category retained by this wire metadata record.
        category: String,
        /// Imported/exported runtime label, distinct from the document object UUID.
        runtime_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MotionTrack {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Typed target reference; unresolved imported runtime labels remain explicit.
    pub target: MotionTrackTarget,
    /// First persistent key of the track; following segment endpoints carry their own key IDs.
    pub initial: MotionKey,
    /// Ordered interpolation segments after the initial key, with stable endpoint identities.
    pub segments: std::sync::Arc<Vec<MotionSegment>>,
    /// Optional fade-in duration in seconds; None retains the format/runtime default.
    pub fade_in: Option<f32>,
    /// Optional fade-out duration in seconds; None retains the format/runtime default.
    pub fade_out: Option<f32>,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
}

impl MotionTrack {
    /// Ordered projection; there is only one persistent representation of a key.
    pub fn keys(&self) -> impl Iterator<Item = &MotionKey> {
        std::iter::once(&self.initial).chain(self.segments.iter().map(MotionSegment::end))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MotionEvent {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Absolute time in seconds within the owning motion clip.
    pub time: f32,
    /// Channel value or replacement payload in the domain declared by this field type.
    pub value: String,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MotionClip {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Human-readable display or registration name; distinct from persistent object identity.
    pub name: String,
    /// Motion playback duration in seconds; authoring keys may retain source-range provenance.
    pub duration: f32,
    /// Nominal motion frames per second or explicit Physics fixed-step frequency, as declared by the owner.
    pub fps: f32,
    /// Whether motion playback repeats after its duration.
    pub looping: bool,
    /// Whether motion Bezier export follows the restricted control-time convention.
    pub restricted_beziers: bool,
    /// Optional fade-in duration in seconds; None retains the format/runtime default.
    pub fade_in: Option<f32>,
    /// Optional fade-out duration in seconds; None retains the format/runtime default.
    pub fade_out: Option<f32>,
    /// Display order only; tracks retain their execution order. Empty uses execution order.
    pub track_order: Vec<String>,
    /// Ordered persistent motion tracks; playback state is owned separately.
    pub tracks: crate::SharedVec<MotionTrack>,
    /// Ordered owned events in the motion clip; reading them does not advance playback.
    pub events: crate::SharedVec<MotionEvent>,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
    /// Owned opaque motion metadata fields preserved for format roundtrips.
    pub meta_extensions: BTreeMap<String, Value>,
    /// Original runtime-ID namespace used to validate preservation of opaque imported fields.
    pub opaque_source_ids: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    /// Original imported-content hash used to detect unsupported changes to opaque fields.
    pub opaque_source_content_hash: Option<String>,
}

/// model3 registration is separate from a clip: one file may appear in multiple groups.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MotionRegistration {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Stable motion clip ID referenced by this registration.
    pub clip_id: String,
    /// Optional fade-in duration in seconds; None retains the format/runtime default.
    pub fade_in: Option<f32>,
    /// Optional fade-out duration in seconds; None retains the format/runtime default.
    pub fade_out: Option<f32>,
    /// Optional attachment reference for the registered motion sound.
    pub sound: Option<super::AttachmentRef>,
    /// Owned opaque imported fields preserved for format roundtrips; declared fields cannot be shadowed.
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MotionGroup {
    /// Stable identity in the owning authoring domain; exported metadata IDs retain their source text.
    pub id: String,
    /// Human-readable display or registration name; distinct from persistent object identity.
    pub name: String,
    /// Ordered owned registrations or expression/Pose entries, with stable identities.
    pub entries: Vec<MotionRegistration>,
}

/// Source-authoring key storage can extend outside the scene playback range.
/// Native projects preserve this recognized provenance; runtime export splits
/// curves to the playback range and omits the source-only metadata.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[derive(zipzi_api_derive::ApiShape, zipzi_api_derive::BinaryWire)]
pub struct MotionSourceRange {
    /// Start time in seconds of the retained source-authoring range.
    pub start: f32,
    /// Segment endpoint key or source-range end time, as declared by the enclosing type.
    pub end: f32,
}
const SOURCE_RANGE: &str = "ZipziSourceKeyRange";
impl MotionClip {
    pub fn runtime_meta_extensions(&self) -> BTreeMap<String, Value> {
        self.meta_extensions
            .iter()
            .filter(|(key, _)| key.as_str() != SOURCE_RANGE)
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect()
    }
    pub fn source_key_range(&self) -> Option<MotionSourceRange> {
        self.meta_extensions
            .get(SOURCE_RANGE)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
    pub fn preserve_source_key_range(&mut self) {
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
    pub fn has_extensions(&self) -> bool {
        !self.extensions.is_empty()
            || self.meta_extensions.keys().any(|key| key != SOURCE_RANGE)
            || self.tracks.iter().any(|track| !track.extensions.is_empty())
            || self.events.iter().any(|event| !event.extensions.is_empty())
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
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for g in groups {
            if !super::valid_uuid(&g.id)
                || !ids.insert(&g.id)
                || (self.contains_id(&g.id) && self.get_motion_group(&g.id).is_none())
            {
                return Status::error("INVALID_MOTION_GROUP_ID", &g.id);
            }
            if g.name.contains('\0') || !names.insert(&g.name) {
                return Status::error("INVALID_MOTION_GROUP", &g.name);
            }
        }
        for e in groups
            .iter()
            .flat_map(|g| &g.entries)
            .chain(self.motion_roots())
        {
            if !super::valid_uuid(&e.id)
                || !ids.insert(&e.id)
                || (self.contains_id(&e.id) && self.get_motion_registration(&e.id).is_none())
            {
                return Status::error("INVALID_MOTION_REGISTRATION_ID", &e.id);
            }
            if self.get_motion(&e.clip_id).is_none() {
                return Status::error("MISSING_MOTION", &e.clip_id);
            }
            if [e.fade_in, e.fade_out]
                .into_iter()
                .flatten()
                .any(|v| !v.is_finite() || v < 0.)
            {
                return Status::error("INVALID_MOTION_FADE", &e.id);
            }
            if let Some(reference) = &e.sound {
                if self
                    .attachment_path(reference)
                    .is_none_or(|p| !super::valid_attachment_path(p))
                {
                    return Status::error("INVALID_MOTION_SOUND", &e.id);
                }
            }
        }
        Status::ok()
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
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = crate::benchmark::span("core.validate_motion");
        if !super::valid_uuid(&clip.id) {
            return Status::error("INVALID_MOTION_ID", &clip.id);
        }
        let mut nested_elsewhere = HashSet::new();
        for other in self
            .content
            .motions
            .values()
            .filter(|other| other.id != clip.id)
        {
            for track in &other.tracks {
                nested_elsewhere.insert(track.id.as_str());
                nested_elsewhere.extend(track.keys().map(|key| key.id.as_str()));
            }
            nested_elsewhere.extend(other.events.iter().map(|event| event.id.as_str()));
        }
        if nested_elsewhere.contains(clip.id.as_str()) {
            return Status::error("DUPLICATE_ID", &clip.id);
        }
        if clip.name.is_empty() || clip.name.contains('\0') {
            return Status::error("INVALID_MOTION_NAME", &clip.id);
        }
        if self
            .content
            .motions
            .values()
            .any(|other| other.id != clip.id && other.name == clip.name)
        {
            return Status::error("DUPLICATE_MOTION_NAME", &clip.name);
        }
        let status = validate_properties(
            &clip.id,
            clip.duration,
            clip.fps,
            clip.fade_in,
            clip.fade_out,
        );
        if !status.is_ok() {
            return status;
        }
        let (minimum_time, maximum_time) = match time_range(clip, clip.duration) {
            Ok(range) => range,
            Err(status) => return status,
        };
        if !clip.track_order.is_empty()
            && (clip.track_order.len() != clip.tracks.len()
                || clip.track_order.iter().collect::<HashSet<_>>()
                    != clip.tracks.iter().map(|t| &t.id).collect::<HashSet<_>>())
        {
            return Status::error("INVALID_TRACK_ORDER", &clip.id);
        }
        let mut ids = HashSet::new();
        for (index, track) in clip.tracks.iter().enumerate() {
            if !super::valid_uuid(&track.id)
                || !ids.insert(track.id.as_str())
                || track.id == clip.id
            {
                return Status::error(
                    "INVALID_MOTION_TRACK_ID",
                    format!("{}.tracks[{index}]", clip.id),
                );
            }
            if self.contains_base_identity(&track.id)
                || nested_elsewhere.contains(track.id.as_str())
            {
                return Status::error("DUPLICATE_ID", &track.id);
            }
            for key in track.keys() {
                if !super::valid_uuid(&key.id) || !ids.insert(key.id.as_str()) {
                    return Status::error("INVALID_MOTION_KEY_ID", &key.id);
                }
                if key.id == clip.id
                    || self.contains_base_identity(&key.id)
                    || nested_elsewhere.contains(key.id.as_str())
                {
                    return Status::error("DUPLICATE_ID", &key.id);
                }
            }
            if [track.fade_in, track.fade_out]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite() || value < 0.0)
            {
                return Status::error(
                    "INVALID_MOTION_FADE",
                    format!("{}.tracks[{index}]", clip.id),
                );
            }
            match &track.target {
                MotionTrackTarget::Parameter { parameter_id }
                    if self.get_parameter(parameter_id).is_none() =>
                {
                    return Status::error(
                        "MISSING_PARAMETER",
                        format!("{}.tracks[{index}]", clip.id),
                    );
                }
                MotionTrackTarget::PartOpacity { part_id } if self.get_part(part_id).is_none() => {
                    return Status::error("MISSING_PART", format!("{}.tracks[{index}]", clip.id));
                }
                MotionTrackTarget::Model { runtime_id }
                    if runtime_id.is_empty() || runtime_id.contains('\0') =>
                {
                    return Status::error(
                        "INVALID_MOTION_TARGET",
                        format!("{}.tracks[{index}]", clip.id),
                    );
                }
                MotionTrackTarget::Unresolved {
                    category,
                    runtime_id,
                } if !matches!(category.as_str(), "Parameter" | "PartOpacity")
                    || runtime_id.is_empty()
                    || runtime_id.contains('\0') =>
                {
                    return Status::error(
                        "INVALID_MOTION_TARGET",
                        format!("{}.tracks[{index}]", clip.id),
                    );
                }
                _ => {}
            }
            if !track.initial.time.is_finite()
                || !track.initial.value.is_finite()
                || track.initial.time < minimum_time
                || track.initial.time > maximum_time
            {
                return Status::error(
                    "INVALID_MOTION_POINT",
                    format!("{}.tracks[{index}].initial", clip.id),
                );
            }
            // Ordinary motion3 retains its half-frame rounding tolerance;
            // editable Cubism sources explicitly preserve their wider key range.
            let end_tolerance = (0.5 / clip.fps).max(0.0001);
            let mut previous = track.initial.time;
            for (segment_index, segment) in track.segments.iter().enumerate() {
                let end = segment.end();
                if !end.time.is_finite()
                    || !end.value.is_finite()
                    || end.time <= previous
                    || end.time > maximum_time + end_tolerance
                {
                    return Status::error(
                        "INVALID_MOTION_POINT",
                        format!("{}.tracks[{index}].segments[{segment_index}]", clip.id),
                    );
                }
                if let MotionSegment::Bezier {
                    control1, control2, ..
                } = segment
                {
                    if ![control1.time, control1.value, control2.time, control2.value]
                        .into_iter()
                        .all(f32::is_finite)
                        || (clip.restricted_beziers
                            && !(previous <= control1.time
                                && control1.time <= control2.time
                                && control2.time <= end.time))
                    {
                        return Status::error(
                            "INVALID_MOTION_BEZIER",
                            format!("{}.tracks[{index}].segments[{segment_index}]", clip.id),
                        );
                    }
                }
                previous = end.time;
            }
        }
        for (index, event) in clip.events.iter().enumerate() {
            if !super::valid_uuid(&event.id)
                || !ids.insert(event.id.as_str())
                || event.id == clip.id
            {
                return Status::error(
                    "INVALID_MOTION_EVENT_ID",
                    format!("{}.events[{index}]", clip.id),
                );
            }
            if self.contains_base_identity(&event.id)
                || nested_elsewhere.contains(event.id.as_str())
            {
                return Status::error("DUPLICATE_ID", &event.id);
            }
            if !event.time.is_finite() || event.time < 0.0 || event.time > clip.duration {
                return Status::error(
                    "INVALID_MOTION_EVENT_TIME",
                    format!("{}.events[{index}]", clip.id),
                );
            }
        }
        Status::ok()
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

fn validate_properties(
    id: &str,
    duration: f32,
    fps: f32,
    fade_in: Option<f32>,
    fade_out: Option<f32>,
) -> Status {
    if !duration.is_finite() || duration <= 0.0 {
        return Status::error("INVALID_MOTION_DURATION", id);
    }
    if !fps.is_finite() || fps <= 0.0 {
        return Status::error("INVALID_MOTION_FPS", id);
    }
    if [fade_in, fade_out]
        .into_iter()
        .flatten()
        .any(|value| !value.is_finite() || value < 0.0)
    {
        return Status::error("INVALID_MOTION_FADE", id);
    }
    Status::ok()
}

fn time_range(clip: &MotionClip, duration: f32) -> Result<(f32, f32), Status> {
    let source_range = match clip.meta_extensions.get(SOURCE_RANGE) {
        Some(value) => match serde_json::from_value::<MotionSourceRange>(value.clone()) {
            Ok(range)
                if range.start.is_finite()
                    && range.end.is_finite()
                    && range.start <= 0.
                    && range.end >= 0. =>
            {
                Some(range)
            }
            _ => return Err(Status::error("INVALID_MOTION_SOURCE_RANGE", &clip.id)),
        },
        None => None,
    };
    Ok((
        source_range.map_or(0., |r| r.start),
        source_range.map_or(duration, |r| r.end.max(duration)),
    ))
}
