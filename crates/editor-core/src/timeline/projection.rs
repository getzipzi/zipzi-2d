use super::*;
use anyhow::{Result, ensure};
use kasane_core::document::{MotionClip, MotionEvent, MotionSegment};
use kasane_sdk::{
    CompiledCurve, MotionInterpolation, motion_bezier_time_is_monotonic, sample_motion_curve,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimelineDiagnostic {
    OpaqueClip,
    UnresolvedTarget,
    UnsupportedModelChannel,
    PartOpacityWithoutPose,
    NonMonotonicBezier,
}

#[derive(Debug, Clone)]
pub struct TimelineChannel {
    pub id: TimelineChannelId,
    pub name: String,
    pub runtime_id: String,
    pub range: Option<(f32, f32)>,
    pub default_value: f32,
    pub tracks: Vec<String>,
    pub editable: bool,
    pub diagnostics: Vec<TimelineDiagnostic>,
}

#[derive(Debug, Clone)]
pub struct TimelineKey {
    pub reference: TimelineKeyRef,
    pub time: f64,
    pub value: f32,
    pub outgoing: Option<MotionInterpolation>,
    pub time_editable: bool,
    pub outgoing_split_editable: bool,
}

#[derive(Debug, Clone, Copy)]
pub struct TimelineBezierHandles {
    pub control1: kasane_core::document::MotionPoint,
    pub control2: kasane_core::document::MotionPoint,
    pub horizontal_editable: bool,
    pub value_editable: bool,
}
#[derive(Debug, Clone)]
pub struct TimelineTrack {
    pub id: String,
    pub channel: TimelineChannelId,
    pub keys: Vec<TimelineKey>,
    pub editable: bool,
    pub curve: CompiledCurve,
    key_indices: HashMap<String, usize>,
}
impl TimelineTrack {
    pub fn visible_keys(&self, start: f64, end: f64) -> &[TimelineKey] {
        let left = self.keys.partition_point(|key| key.time < start);
        let right = self.keys.partition_point(|key| key.time <= end);
        &self.keys[left.min(right)..right]
    }
}

#[derive(Debug, Clone)]
pub struct TimelineSummary {
    pub time: f64,
    pub keys: Vec<TimelineKeyRef>,
}
#[derive(Debug, Clone)]
pub struct TimelineRow {
    pub id: TimelineRowId,
    pub name: String,
    pub depth: usize,
    pub channel: Option<TimelineChannelId>,
    pub track_id: Option<String>,
    pub keys: Vec<TimelineKeyRef>,
    pub summary: Vec<TimelineSummary>,
}
impl TimelineRow {
    fn group(id: TimelineRowId, name: String, depth: usize) -> Self {
        Self {
            id,
            name,
            depth,
            channel: None,
            track_id: None,
            keys: vec![],
            summary: vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct TimelineProjection {
    pub clip_id: String,
    pub duration: f64,
    pub display_end: f64,
    pub fps: f32,
    pub restricted_beziers: bool,
    pub editable: bool,
    pub channels: Arc<Vec<TimelineChannel>>,
    pub tracks: HashMap<String, Arc<TimelineTrack>>,
    pub events: Vec<MotionEvent>,
    pub rows: Arc<Vec<TimelineRow>>,
}

impl TimelineProjection {
    /// Numeric channel edits keep key identities, times and row layout intact.
    /// Share that structure and every unchanged track with earlier projections.
    pub(crate) fn with_value_changes(
        &self,
        previous: &MotionClip,
        next: &MotionClip,
    ) -> Option<Self> {
        if previous.id != next.id
            || previous.tracks.len() != next.tracks.len()
            || previous.restricted_beziers != next.restricted_beziers
        {
            return None;
        }
        let mut result = self.clone();
        for (before, track) in previous.tracks.iter().zip(&next.tracks) {
            if before.id != track.id || before.target != track.target {
                return None;
            }
            if before.initial == track.initial && Arc::ptr_eq(&before.segments, &track.segments) {
                continue;
            }
            let existing = self.tracks.get(&track.id)?;
            let points: Vec<_> = track.keys().collect();
            if points.len() != existing.keys.len()
                || points.iter().zip(&existing.keys).any(|(point, key)| {
                    point.id != key.reference.key_id || f64::from(point.time) != key.time
                })
            {
                return None;
            }
            let mut changed = (**existing).clone();
            for (point, key) in points.iter().zip(&mut changed.keys) {
                key.value = point.value;
            }
            changed.curve = CompiledCurve::from(track);
            result.tracks.insert(track.id.clone(), Arc::new(changed));
        }
        Some(result)
    }

    pub fn new(document: &crate::Document, clip_id: &str) -> Result<Self> {
        let clip = document
            .motion_definition(clip_id)
            .ok_or_else(|| anyhow::anyhow!("Motion no longer exists"))?;
        Self::from_clip(document, clip)
    }
    pub(crate) fn from_clip(document: &crate::Document, clip: &MotionClip) -> Result<Self> {
        let model = document.model();
        let mut result = Self {
            clip_id: clip.id.clone(),
            duration: f64::from(clip.duration),
            display_end: f64::from(clip.duration),
            fps: clip.fps,
            restricted_beziers: clip.restricted_beziers,
            editable: !clip.has_extensions(),
            channels: Arc::new(vec![]),
            tracks: HashMap::new(),
            events: clip.events.clone(),
            rows: Arc::new(vec![]),
        };
        let parameters = document.parameters();
        for parameter in &parameters {
            let runtime_id = model
                .get_parameter(&parameter.id)
                .unwrap()
                .runtime_id
                .clone();
            result.add_channel(
                clip,
                TimelineChannelId::Parameter(parameter.id.clone()),
                parameter.name.clone(),
                runtime_id,
                Some((parameter.minimum, parameter.maximum)),
                parameter.default_value,
                vec![],
            );
        }
        result.add_channel(
            clip,
            TimelineChannelId::Model("Opacity".into()),
            "Opacity".into(),
            "Opacity".into(),
            Some((0., 1.)),
            1.,
            vec![],
        );
        for id in model.part_order() {
            let part = model.get_part(id).unwrap();
            result.add_channel(
                clip,
                TimelineChannelId::PartOpacity(id.clone()),
                part.name.clone(),
                part.runtime_id.clone(),
                Some((0., 1.)),
                1.,
                if model.pose().is_none() {
                    vec![TimelineDiagnostic::PartOpacityWithoutPose]
                } else {
                    vec![]
                },
            );
        }
        for track in &clip.tracks {
            let id = TimelineChannelId::from_target(&track.target);
            if result.channel(&id).is_none() {
                let (name, diagnostic) = match &id {
                    TimelineChannelId::Model(id) => {
                        (id.clone(), TimelineDiagnostic::UnsupportedModelChannel)
                    }
                    TimelineChannelId::Unresolved { runtime_id, .. } => {
                        (runtime_id.clone(), TimelineDiagnostic::UnresolvedTarget)
                    }
                    _ => unreachable!("validated model target"),
                };
                result.add_channel(clip, id, name.clone(), name, None, 0., vec![diagnostic]);
            }
        }
        for track in &clip.tracks {
            let channel = TimelineChannelId::from_target(&track.target);
            let editable = result.channel(&channel).unwrap().editable;
            let all: Vec<_> = track.keys().collect();
            let monotonic: Vec<_> = track
                .segments
                .iter()
                .enumerate()
                .map(|(i, segment)| match segment {
                    MotionSegment::Bezier {
                        control1,
                        control2,
                        end,
                    } if !clip.restricted_beziers => motion_bezier_time_is_monotonic(
                        all[i].time,
                        control1.time,
                        control2.time,
                        end.time,
                    ),
                    _ => true,
                })
                .collect();
            let keys: Vec<_> = all
                .iter()
                .enumerate()
                .map(|(i, key)| TimelineKey {
                    reference: TimelineKeyRef {
                        clip_id: clip.id.clone(),
                        track_id: track.id.clone(),
                        key_id: key.id.clone(),
                    },
                    time: f64::from(key.time),
                    value: key.value,
                    outgoing: track.segments.get(i).map(|segment| match segment {
                        MotionSegment::Linear { .. } => MotionInterpolation::Linear,
                        MotionSegment::Bezier { .. } => MotionInterpolation::Bezier,
                        MotionSegment::Stepped { .. } => MotionInterpolation::Stepped,
                        MotionSegment::InverseStepped { .. } => MotionInterpolation::InverseStepped,
                    }),
                    time_editable: editable
                        && (i == 0 || monotonic[i - 1])
                        && monotonic.get(i).copied().unwrap_or(true),
                    outgoing_split_editable: editable && monotonic.get(i).copied().unwrap_or(false),
                })
                .collect();
            result.display_end = result.display_end.max(f64::from(all.last().unwrap().time));
            if monotonic.iter().any(|value| !value) {
                Arc::make_mut(&mut result.channels)
                    .iter_mut()
                    .find(|c| c.id == channel)
                    .unwrap()
                    .diagnostics
                    .push(TimelineDiagnostic::NonMonotonicBezier);
            }
            result.tracks.insert(
                track.id.clone(),
                Arc::new(TimelineTrack {
                    id: track.id.clone(),
                    channel,
                    key_indices: keys
                        .iter()
                        .enumerate()
                        .map(|(index, key)| (key.reference.key_id.clone(), index))
                        .collect(),
                    keys,
                    editable,
                    curve: CompiledCurve::from(track),
                }),
            );
        }
        let layout =
            crate::ParameterLayout::new(&document.parameter_groups(), &parameters, "Ungrouped");
        for row in layout.rows() {
            match row {
                crate::ParameterRow::Group {
                    id, name, depth, ..
                } => Arc::make_mut(&mut result.rows).push(TimelineRow::group(
                    TimelineRowId::Group(id.clone()),
                    name.clone(),
                    *depth,
                )),
                crate::ParameterRow::Parameter { index, depth } => result.append_channel(
                    &TimelineChannelId::Parameter(parameters[*index].id.clone()),
                    *depth,
                ),
            }
        }
        for (category, name) in [
            (TimelineCategory::Model, "Model channels"),
            (TimelineCategory::Parts, "Part opacity"),
            (TimelineCategory::Unresolved, "Unresolved channels"),
        ] {
            let ids: Vec<_> = result
                .channels
                .iter()
                .filter(|c| {
                    matches!(
                        (&c.id, category),
                        (TimelineChannelId::Model(_), TimelineCategory::Model)
                            | (TimelineChannelId::PartOpacity(_), TimelineCategory::Parts)
                            | (
                                TimelineChannelId::Unresolved { .. },
                                TimelineCategory::Unresolved
                            )
                    )
                })
                .map(|c| c.id.clone())
                .collect();
            if !ids.is_empty() {
                Arc::make_mut(&mut result.rows).push(TimelineRow::group(
                    TimelineRowId::Category(category),
                    name.into(),
                    0,
                ));
                for id in ids {
                    result.append_channel(&id, 1);
                }
            }
        }
        Arc::make_mut(&mut result.rows).push(TimelineRow::group(
            TimelineRowId::Events,
            "Events".into(),
            0,
        ));
        result.events.sort_by(|a, b| a.time.total_cmp(&b.time));
        Ok(result)
    }

    fn add_channel(
        &mut self,
        clip: &MotionClip,
        id: TimelineChannelId,
        name: String,
        runtime_id: String,
        range: Option<(f32, f32)>,
        default_value: f32,
        mut diagnostics: Vec<TimelineDiagnostic>,
    ) {
        if clip.has_extensions() {
            diagnostics.push(TimelineDiagnostic::OpaqueClip);
        }
        let editable = !clip.has_extensions()
            && !matches!(&id, TimelineChannelId::Unresolved { .. })
            && !matches!(&id, TimelineChannelId::Model(id) if id != "Opacity");
        let tracks = clip
            .tracks
            .iter()
            .filter(|track| TimelineChannelId::from_target(&track.target) == id)
            .map(|track| track.id.clone())
            .collect();
        Arc::make_mut(&mut self.channels).push(TimelineChannel {
            id,
            name,
            runtime_id,
            range,
            default_value,
            tracks,
            editable,
            diagnostics,
        });
    }
    fn append_channel(&mut self, id: &TimelineChannelId, depth: usize) {
        let channel = self.channel(id).unwrap();
        let name = channel.name.clone();
        let tracks = channel.tracks.clone();
        let track_id = (tracks.len() == 1).then(|| tracks[0].clone());
        let keys = track_id.as_ref().map_or_else(Vec::new, |id| {
            self.tracks[id]
                .keys
                .iter()
                .map(|k| k.reference.clone())
                .collect()
        });
        Arc::make_mut(&mut self.rows).push(TimelineRow {
            id: TimelineRowId::Channel(id.clone()),
            name,
            depth,
            channel: Some(id.clone()),
            track_id,
            keys,
            summary: vec![],
        });
        if tracks.len() > 1 {
            for (index, track) in tracks.into_iter().enumerate() {
                Arc::make_mut(&mut self.rows).push(TimelineRow {
                    id: TimelineRowId::Track(track.clone()),
                    name: format!("Track {}", index + 1),
                    depth: depth + 1,
                    channel: Some(id.clone()),
                    track_id: Some(track.clone()),
                    keys: self.tracks[&track]
                        .keys
                        .iter()
                        .map(|key| key.reference.clone())
                        .collect(),
                    summary: vec![],
                });
            }
        }
    }
    pub fn channel(&self, id: &TimelineChannelId) -> Option<&TimelineChannel> {
        self.channels.iter().find(|channel| &channel.id == id)
    }
    pub fn display_handles(&self, end: &TimelineKeyRef) -> Option<TimelineBezierHandles> {
        use kasane_sdk::CompiledCurve;
        let track = self.tracks.get(&end.track_id)?;
        self.key(end)?;
        let index = track.key_indices[&end.key_id].checked_sub(1)?;
        let CompiledCurve { segments, .. } = &track.curve;
        let kasane_sdk::CompiledSegment::Bezier {
            control1,
            control2,
            end: point,
        } = segments[index]
        else {
            return None;
        };
        let (mut control1, mut control2) = (control1, control2);
        if self.restricted_beziers {
            let start = track.keys[index].time;
            let span = f64::from(point.time) - start;
            control1.time = (start + span / 3.) as f32;
            control2.time = (f64::from(point.time) - span / 3.) as f32;
        }
        Some(TimelineBezierHandles {
            control1,
            control2,
            horizontal_editable: !self.restricted_beziers
                && track.keys[index].outgoing_split_editable,
            value_editable: track.keys[index].outgoing_split_editable,
        })
    }
    pub fn key(&self, key: &TimelineKeyRef) -> Option<&TimelineKey> {
        if key.clip_id != self.clip_id {
            return None;
        }
        let track = self.tracks.get(&key.track_id)?;
        track.keys.get(*track.key_indices.get(&key.key_id)?)
    }
    pub fn raw_value(
        &self,
        channel_id: &TimelineChannelId,
        track_id: Option<&str>,
        time: f64,
    ) -> Result<f32> {
        ensure!(
            time.is_finite() && (time as f32).is_finite(),
            "Invalid sample time"
        );
        let channel = self
            .channel(channel_id)
            .ok_or_else(|| anyhow::anyhow!("Missing channel"))?;
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
                    "Select a concrete track for a duplicate target"
                );
                channel.tracks.first().map(String::as_str)
            }
        };
        Ok(track_id.map_or(channel.default_value, |id| {
            sample_motion_curve(&self.tracks[id].curve, time as f32, self.restricted_beziers)
        }))
    }
    pub fn visible_rows(&self, view: &TimelineViewState) -> Vec<TimelineRow> {
        let search = view.search.trim().to_lowercase();
        let empty = self.tracks.is_empty();
        let matches: HashSet<_> = self
            .channels
            .iter()
            .filter(|channel| {
                if !search.is_empty() {
                    channel.name.to_lowercase().contains(&search)
                        || channel.runtime_id.to_lowercase().contains(&search)
                } else {
                    empty || view.filter == TimelineFilter::All || !channel.tracks.is_empty()
                }
            })
            .map(|channel| &channel.id)
            .collect();
        let mut included: Vec<_> = self
            .rows
            .iter()
            .map(|row| {
                row.id == TimelineRowId::Events
                    || row.channel.as_ref().is_some_and(|id| matches.contains(id))
            })
            .collect();
        let mut parents: Vec<usize> = Vec::new();
        for (i, row) in self.rows.iter().enumerate() {
            while parents
                .last()
                .is_some_and(|&p| self.rows[p].depth >= row.depth)
            {
                parents.pop();
            }
            if included[i] {
                for &p in &parents {
                    included[p] = true;
                }
            }
            parents.push(i);
        }
        let mut result = Vec::new();
        let mut hidden_depth = None;
        for (i, row) in self.rows.iter().enumerate() {
            if !included[i] {
                continue;
            }
            if hidden_depth.is_some_and(|depth| row.depth > depth) {
                continue;
            }
            hidden_depth = None;
            let mut row = row.clone();
            if search.is_empty() && view.collapsed.contains(&row.id) {
                hidden_depth = Some(row.depth);
                let mut points: Vec<_> = self.rows[i + 1..]
                    .iter()
                    .enumerate()
                    .take_while(|(_, child)| child.depth > row.depth)
                    .filter(|(offset, _)| included[i + 1 + offset])
                    .flat_map(|(_, child)| &child.keys)
                    .filter_map(|key| self.key(key))
                    .collect();
                points.sort_by(|a, b| a.time.total_cmp(&b.time));
                for point in points {
                    if let Some(last) = row
                        .summary
                        .last_mut()
                        .filter(|last| last.time == point.time)
                    {
                        last.keys.push(point.reference.clone());
                    } else {
                        row.summary.push(TimelineSummary {
                            time: point.time,
                            keys: vec![point.reference.clone()],
                        });
                    }
                }
            }
            result.push(row);
        }
        result
    }
}
