//! Session-only range controls. The clip's nominal duration bounds playback.
use crate::{Editor, TimelineBinding};
use anyhow::{Result, ensure};

#[derive(Debug, thiserror::Error)]
pub enum TimelineLoopRangeError {
    #[error("Loop endpoints must be within the motion duration")]
    Outside,
    #[error("Loop end must be after its start")]
    Order,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TimelineLoopRange {
    pub start: f64,
    pub end: f64,
    pub enabled: bool,
}

impl TimelineLoopRange {
    pub(super) fn intersect_duration(self, duration: f64) -> Option<Self> {
        let end = self.end.min(duration);
        ((self.start as f32) < end as f32).then_some(Self { end, ..self })
    }

    fn validate(self, duration: f64) -> Result<Self> {
        ensure!(
            self.start.is_finite()
                && self.end.is_finite()
                && self.start >= 0.
                && self.end <= duration,
            TimelineLoopRangeError::Outside
        );
        ensure!(
            self.start < self.end && (self.start as f32) < self.end as f32,
            TimelineLoopRangeError::Order
        );
        Ok(self)
    }
}

impl Editor {
    /// Snap only the endpoint(s) being moved. An existing endpoint can be a
    /// subframe time even after the snapping preference changes.
    pub fn set_timeline_loop_range(
        &mut self,
        binding: TimelineBinding,
        start: f64,
        end: f64,
    ) -> Result<()> {
        let range = self.timeline_loop_candidate(binding, start, end)?;
        self.install_timeline_loop_range(binding, Some(range), false)
    }

    pub fn resize_timeline_loop_range(
        &mut self,
        binding: TimelineBinding,
        start: f64,
        end: f64,
    ) -> Result<()> {
        let current = self
            .timeline_loop_target(binding)?
            .loop_range
            .ok_or_else(|| anyhow::anyhow!("Set a loop range first"))?;
        let mut range = self.timeline_loop_candidate(binding, start, end)?;
        range.enabled = current.enabled;
        self.install_timeline_loop_range(binding, Some(range), false)
    }

    pub fn timeline_loop_candidate(
        &self,
        binding: TimelineBinding,
        start: f64,
        end: f64,
    ) -> Result<TimelineLoopRange> {
        let view = self.timeline_loop_target(binding)?;
        let projection = self.timeline_projection().unwrap();
        for time in [start, end] {
            ensure!(
                time.is_finite() && time >= 0. && time as f32 <= projection.duration as f32,
                TimelineLoopRangeError::Outside
            );
        }
        let previous = view.loop_range;
        let endpoint = |time: f64, old: Option<f64>| {
            if time >= projection.duration {
                return projection.duration;
            }
            if !view.snap || old == Some(time) {
                return time;
            }
            let fps = f64::from(projection.fps);
            let snapped = (time * fps).round() / fps;
            // A nominal last frame can round to the authored f32 duration.
            if snapped as f32 == projection.duration as f32 {
                projection.duration
            } else {
                snapped
            }
        };
        TimelineLoopRange {
            start: endpoint(start, previous.map(|range| range.start)),
            end: endpoint(end, previous.map(|range| range.end)),
            enabled: true,
        }
        .validate(projection.duration)
    }

    pub fn enable_timeline_loop_range(
        &mut self,
        binding: TimelineBinding,
        enabled: bool,
    ) -> Result<()> {
        let mut range = self
            .timeline_loop_target(binding)?
            .loop_range
            .ok_or_else(|| anyhow::anyhow!("Set a loop range first"))?;
        range.enabled = enabled;
        self.install_timeline_loop_range(binding, Some(range), true)
    }

    pub fn clear_timeline_loop_range(&mut self, binding: TimelineBinding) -> Result<()> {
        self.install_timeline_loop_range(binding, None, true)
    }

    fn install_timeline_loop_range(
        &mut self,
        binding: TimelineBinding,
        range: Option<TimelineLoopRange>,
        keep_playing: bool,
    ) -> Result<()> {
        let old = self.timeline_loop_target(binding)?.loop_range;
        if old == range {
            return Ok(());
        }
        let enabled = |range: Option<TimelineLoopRange>| range.filter(|range| range.enabled);
        if !keep_playing || enabled(old) != enabled(range) {
            self.change_range_playback(enabled(range), keep_playing)?;
        }
        self.active_session_mut()
            .unwrap()
            .animation
            .timeline
            .current_mut()
            .unwrap()
            .view
            .loop_range = range;
        Ok(())
    }

    fn timeline_loop_target(&self, binding: TimelineBinding) -> Result<&super::TimelineViewState> {
        ensure!(
            self.timeline_binding_valid(binding),
            "Timeline panel binding changed"
        );
        self.ensure_animation_target(binding.document)?;
        self.timeline_view()
            .ok_or_else(|| anyhow::anyhow!("Select a motion"))
    }
}
