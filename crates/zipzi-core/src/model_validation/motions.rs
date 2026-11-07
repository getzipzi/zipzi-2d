use super::*;
use zipzi_contract::document::MotionSourceRange;
impl Model<'_> {
    pub(crate) fn validate_motion(&self, clip: &MotionClip) -> Status {
        self.validate_motion_clip(clip, false)
    }

    /// The complete document has already checked all record and nested IDs.
    pub(crate) fn validate_admitted_motion(&self, clip: &MotionClip) -> Status {
        self.validate_motion_clip(clip, true)
    }

    fn validate_motion_clip(&self, clip: &MotionClip, identities_validated: bool) -> Status {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = crate::benchmark::span("core.validate_motion");
        if !valid_uuid(&clip.id) {
            return Status::error("INVALID_MOTION_ID", &clip.id);
        }
        let mut nested_elsewhere = HashSet::new();
        if !identities_validated {
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
        let base_ids = (!identities_validated).then(|| self.base_identity_set());
        for (index, track) in clip.tracks.iter().enumerate() {
            if !identities_validated
                && (!valid_uuid(&track.id) || !ids.insert(track.id.as_str()) || track.id == clip.id)
            {
                return Status::error(
                    "INVALID_MOTION_TRACK_ID",
                    format!("{}.tracks[{index}]", clip.id),
                );
            }
            if base_ids.is_some_and(|ids| ids.contains(track.id.as_str()))
                || nested_elsewhere.contains(track.id.as_str())
            {
                return Status::error("DUPLICATE_ID", &track.id);
            }
            if let Some(base_ids) = base_ids {
                for key in track.keys() {
                    if !valid_uuid(&key.id) || !ids.insert(key.id.as_str()) {
                        return Status::error("INVALID_MOTION_KEY_ID", &key.id);
                    }
                    if key.id == clip.id
                        || base_ids.contains(key.id.as_str())
                        || nested_elsewhere.contains(key.id.as_str())
                    {
                        return Status::error("DUPLICATE_ID", &key.id);
                    }
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
            if !identities_validated
                && (!valid_uuid(&event.id) || !ids.insert(event.id.as_str()) || event.id == clip.id)
            {
                return Status::error(
                    "INVALID_MOTION_EVENT_ID",
                    format!("{}.events[{index}]", clip.id),
                );
            }
            if base_ids.is_some_and(|ids| ids.contains(event.id.as_str()))
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
    pub(crate) fn validate_motion_groups(&self, groups: &[MotionGroup]) -> Status {
        let mut ids = HashSet::new();
        let mut names = HashSet::new();
        for g in groups {
            if !valid_uuid(&g.id)
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
            if !valid_uuid(&e.id)
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
                    .is_none_or(|p| !valid_attachment_path(p))
                {
                    return Status::error("INVALID_MOTION_SOUND", &e.id);
                }
            }
        }
        Status::ok()
    }
}
const SOURCE_RANGE: &str = "ZipziSourceKeyRange";
pub(crate) fn validate_properties(
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

pub(crate) fn time_range(clip: &MotionClip, duration: f32) -> Result<(f32, f32), Status> {
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
