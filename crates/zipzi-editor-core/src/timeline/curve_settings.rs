//! One selection calculation for toolbar, inspector, menus and edit admission.
use super::*;
use zipzi_core::document::{MotionEasingPreset, MotionFrameStep};
use zipzi_sdk::MotionInterpolation;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TimelineMixed<T> {
    Empty,
    Mixed,
    Uniform(T),
}
impl<T: Clone + PartialEq> TimelineMixed<T> {
    fn from_values(values: impl Iterator<Item = T>) -> Self {
        let mut values = values;
        let Some(first) = values.next() else {
            return Self::Empty;
        };
        if values.all(|value| value == first) {
            Self::Uniform(first)
        } else {
            Self::Mixed
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimelineCurveDisabled {
    NoSelection,
    EventsSelected,
    MissingKey,
    NoOutgoing,
    ReadOnly,
    Locked,
}

#[derive(Clone, Debug)]
pub struct TimelineCurveSettings {
    /// Only starting keys with an outgoing interval; terminal keys are excluded.
    pub keys: Vec<TimelineKeyRef>,
    pub trailing_keys: usize,
    pub interpolation: TimelineMixed<MotionInterpolation>,
    /// Uniform(None) means Custom, while Mixed also includes non-Bezier modes.
    pub preset: TimelineMixed<Option<MotionEasingPreset>>,
    pub frame_step: TimelineMixed<MotionFrameStep>,
    pub disabled: Option<TimelineCurveDisabled>,
    pub restricted_conversion_locked: bool,
}
impl TimelineCurveSettings {
    pub fn enabled(&self) -> bool {
        self.disabled.is_none()
    }
    pub fn bezier(&self) -> bool {
        self.interpolation == TimelineMixed::Uniform(MotionInterpolation::Bezier)
    }
    pub fn constant(&self) -> bool {
        matches!(
            self.interpolation,
            TimelineMixed::Uniform(
                MotionInterpolation::Stepped | MotionInterpolation::InverseStepped
            )
        )
    }
    pub fn handles(&self) -> bool {
        matches!(
            self.interpolation,
            TimelineMixed::Uniform(MotionInterpolation::Bezier | MotionInterpolation::AutoSmooth)
        )
    }
}

impl TimelineProjection {
    pub fn curve_settings<'a>(
        &self,
        keys: impl IntoIterator<Item = &'a TimelineKeyRef>,
        events_selected: bool,
    ) -> TimelineCurveSettings {
        let selected: Vec<_> = keys.into_iter().collect();
        let missing = selected.iter().any(|key| self.key(key).is_none());
        let intervals: Vec<_> = selected
            .iter()
            .filter_map(|key| self.key(key))
            .filter(|key| key.outgoing.is_some())
            .collect();
        let disabled = if events_selected {
            Some(TimelineCurveDisabled::EventsSelected)
        } else if selected.is_empty() {
            Some(TimelineCurveDisabled::NoSelection)
        } else if missing {
            Some(TimelineCurveDisabled::MissingKey)
        } else if intervals.is_empty() {
            Some(TimelineCurveDisabled::NoOutgoing)
        } else if !self.editable
            || intervals
                .iter()
                .any(|key| !self.tracks[&key.reference.track_id].editable)
        {
            Some(TimelineCurveDisabled::ReadOnly)
        } else if intervals.iter().any(|key| key.outgoing_locked) {
            Some(TimelineCurveDisabled::Locked)
        } else {
            None
        };
        TimelineCurveSettings {
            keys: intervals.iter().map(|key| key.reference.clone()).collect(),
            trailing_keys: selected.len() - intervals.len(),
            interpolation: TimelineMixed::from_values(
                intervals.iter().map(|key| key.outgoing.unwrap()),
            ),
            preset: if intervals
                .iter()
                .all(|key| key.outgoing == Some(MotionInterpolation::Bezier))
            {
                TimelineMixed::from_values(intervals.iter().map(|key| key.preset))
            } else {
                TimelineMixed::Mixed
            },
            frame_step: TimelineMixed::from_values(intervals.iter().map(|key| key.frame_step)),
            disabled,
            restricted_conversion_locked: self.restricted_beziers
                && self.tracks.values().any(|track| {
                    track.keys.iter().any(|key| {
                        key.outgoing == Some(MotionInterpolation::Bezier) && key.outgoing_locked
                    })
                }),
        }
    }
}
