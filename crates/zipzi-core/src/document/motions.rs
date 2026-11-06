use serde::Serialize;
pub use zipzi_contract::document::{MotionSegment, MotionTrack};
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

pub use zipzi_contract::document::{MotionClip, MotionSourceRange};
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum MotionDisabledReason {
    NeedsConfiguration,
    UnresolvedTarget,
    UnsupportedModelChannel,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DisabledMotionTrack {
    pub track_id: String,
    pub authored_slot: usize,
    pub reason: MotionDisabledReason,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MotionProjection {
    pub clip_id: String,
    pub runtime_to_authored: Vec<usize>,
    pub authored_to_runtime: Vec<Option<usize>>,
    pub disabled: Vec<DisabledMotionTrack>,
}
