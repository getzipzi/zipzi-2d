//! Session-only range controls. The clip's nominal duration bounds playback.
use crate::{Editor, TimelineBinding};
use anyhow::{Result, ensure};

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
            "Loop endpoints must be within the motion duration"
        );
        ensure!(
            self.start < self.end && (self.start as f32) < self.end as f32,
            "Loop end must be after its start"
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
        let view = self.timeline_loop_target(binding)?;
        let projection = self.timeline_projection().unwrap();
        ensure!(
            start.is_finite()
                && end.is_finite()
                && start >= 0.
                && (end <= projection.duration || end as f32 == projection.duration as f32),
            "Loop endpoints must be within the motion duration"
        );
        let previous = view.loop_range;
        let endpoint = |time: f64, old: Option<f64>| {
            if !view.snap || old == Some(time) || time == projection.duration || !time.is_finite() {
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
        let range = TimelineLoopRange {
            start: endpoint(start, previous.map(|range| range.start)),
            end: endpoint(end, previous.map(|range| range.end)),
            enabled: true,
        };
        self.restore_timeline_loop_range(binding, Some(range))
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
        self.restore_timeline_loop_range(binding, Some(range))
    }

    pub fn clear_timeline_loop_range(&mut self, binding: TimelineBinding) -> Result<()> {
        self.restore_timeline_loop_range(binding, None)
    }

    /// Also used to restore the exact session range when a gesture is cancelled.
    pub fn restore_timeline_loop_range(
        &mut self,
        binding: TimelineBinding,
        range: Option<TimelineLoopRange>,
    ) -> Result<()> {
        let old = self.timeline_loop_target(binding)?.loop_range;
        let duration = self.timeline_projection().unwrap().duration;
        let range = range.map(|range| range.validate(duration)).transpose()?;
        if old == range {
            return Ok(());
        }
        self.pause_animation();
        let animation = &mut self.active_session_mut().unwrap().animation;
        animation.playback.reset_range_playback();
        animation.timeline.current_mut().unwrap().view.loop_range = range;
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
