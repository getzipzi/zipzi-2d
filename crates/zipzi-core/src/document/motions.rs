use crate::document::MotionCurve;
use serde::Serialize;
pub use zipzi_contract::document::MotionTrack;
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
            // The outgoing settings belong to the left survivor. A deleted
            // interior key must not select a different mode or frame count.
            let mut merged = original.segments[left].clone();
            merged.end = all[right].clone();
            if let MotionCurve::Bezier {
                control1,
                control2,
                preset,
            } = &mut merged.curve
            {
                *preset = None;
                if let MotionCurve::Bezier { control2: last, .. } =
                    &original.segments[right - 1].curve
                {
                    *control2 = *last;
                } else {
                    control2.time += all[right].time - all[left + 1].time;
                    control2.value += all[right].value - all[left + 1].value;
                }
                if restricted {
                    let width = f64::from(all[right].time) - f64::from(all[left].time);
                    control1.time = (f64::from(all[left].time) + width / 3.) as f32;
                    control2.time = (f64::from(all[right].time) - width / 3.) as f32;
                } else {
                    control1.time = control1.time.clamp(all[left].time, all[right].time);
                    control2.time = control2.time.clamp(all[left].time, all[right].time);
                }
            }
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
