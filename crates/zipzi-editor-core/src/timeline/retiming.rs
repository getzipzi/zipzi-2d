//! Resolve the explicit selection and snap its moving end on the baseline.
use super::*;
use anyhow::{Result, ensure};
use zipzi_sdk::MotionCurveEdit;
pub use zipzi_sdk::MotionRetimeFixedEnd;

#[derive(Clone, Debug)]
pub struct TimelineRetimeSelection {
    pub keys: Vec<TimelineKeyRef>,
    pub events: Vec<String>,
    pub first: f64,
    pub last: f64,
}

impl TimelineRetimeSelection {
    pub fn duration(&self) -> f64 {
        self.last - self.first
    }

    pub fn command(&self, fixed_end: MotionRetimeFixedEnd, moving_time: f64) -> TimelineCommand {
        TimelineCommand::RetimeSelection {
            keys: self.keys.clone(),
            events: self.events.clone(),
            fixed_end,
            moving_time,
        }
    }
}

impl TimelineProjection {
    pub fn retime_selection(&self, view: &TimelineViewState) -> Option<TimelineRetimeSelection> {
        self.retime_span(view.keys.iter(), view.events.iter())
            .ok()
            .map(|(first, last)| TimelineRetimeSelection {
                keys: view.keys.iter().cloned().collect(),
                events: view.events.iter().cloned().collect(),
                first,
                last,
            })
    }

    fn retime_span<'a>(
        &self,
        keys: impl IntoIterator<Item = &'a TimelineKeyRef>,
        events: impl IntoIterator<Item = &'a String>,
    ) -> Result<(f64, f64)> {
        let mut first = f64::INFINITY;
        let mut last = f64::NEG_INFINITY;
        let mut include = |time: f64| {
            first = first.min(time);
            last = last.max(time);
        };
        for key in keys {
            let key = self
                .key(key)
                .ok_or_else(|| anyhow::anyhow!("Key no longer exists in this clip"))?;
            include(key.time);
        }
        for id in events {
            let event = self
                .events
                .iter()
                .find(|event| event.id == *id)
                .ok_or_else(|| anyhow::anyhow!("Event no longer exists"))?;
            include(f64::from(event.time));
        }
        ensure!(first < last, "Select at least two distinct times");
        Ok((first, last))
    }

    pub(super) fn retime_command(
        &self,
        keys: &[TimelineKeyRef],
        events: &[String],
        fixed_end: MotionRetimeFixedEnd,
        moving_time: f64,
        snap: bool,
    ) -> Result<MotionCurveEdit> {
        let (first, last) = self.retime_span(keys, events)?;
        let original = match fixed_end {
            MotionRetimeFixedEnd::Earliest => last,
            MotionRetimeFixedEnd::Latest => first,
        };
        let fps = f64::from(self.fps);
        let mut target = moving_time;
        if (moving_time as f32) != (original as f32)
            && snap
            && moving_time >= 0.
            && moving_time as f32 <= self.duration as f32
        {
            target = (moving_time * fps).round() / fps;
            if target as f32 == self.duration as f32 {
                target = self.duration;
            }
        }
        Ok(MotionCurveEdit::RetimeSelection {
            keys: keys.iter().map(TimelineKeyRef::sdk).collect(),
            events: events.to_vec(),
            fixed_end,
            moving_time: target,
        })
    }
}
