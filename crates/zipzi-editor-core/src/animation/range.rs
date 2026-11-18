//! Range playback uses a single clip-local phase and an independent event cursor.
use super::*;
use crate::TimelineLoopRange;
use zipzi_core::{SharedVec, document::MotionEvent};

#[derive(Clone, Copy, Debug, PartialEq, PartialOrd)]
pub struct AnimationRangeProgress {
    pub cycle: u64,
    pub local: f32,
}

/// Events crossed between accepted frames, in playback order. Full cycles
/// retain one copy of the interval's event sequence even during a long stall.
#[derive(Default, Debug)]
pub struct AnimationRangeEvents {
    pub tail: Vec<MotionEvent>,
    pub full_cycles: u64,
    pub cycle_events: Vec<MotionEvent>,
    pub head: Vec<MotionEvent>,
}

impl AnimationRangeEvents {
    pub fn count(&self) -> u128 {
        self.tail.len() as u128
            + u128::from(self.full_cycles) * self.cycle_events.len() as u128
            + self.head.len() as u128
    }
}

pub(super) struct RangePlayback {
    bounds: TimelineLoopRange,
    offset: f64,
    pub elapsed: f64,
    pub progress: AnimationRangeProgress,
    pub shown: Option<AnimationRangeProgress>,
    sdk_cycle: u64,
    include_first: bool,
    events: SharedVec<MotionEvent>,
    pub clip: Arc<zipzi_core::document::MotionClip>,
}

impl RangePlayback {
    pub fn new(
        bounds: TimelineLoopRange,
        head: f32,
        sdk_cycle: u64,
        resume: bool,
        clip: Arc<zipzi_core::document::MotionClip>,
    ) -> Self {
        let start = bounds.start as f32;
        let end = bounds.end as f32;
        let inside = start <= head && head < end;
        let local = if inside { head } else { start };
        let offset = if local == start {
            0.
        } else {
            f64::from(local) - bounds.start
        };
        let mut events: Vec<_> = clip
            .events
            .iter()
            .filter(|e| start <= e.time && e.time < end)
            .cloned()
            .collect();
        events.sort_by(|a, b| a.time.total_cmp(&b.time));
        Self {
            bounds,
            offset,
            elapsed: 0.,
            progress: AnimationRangeProgress { cycle: 0, local },
            shown: None,
            sdk_cycle,
            include_first: !(inside && resume),
            events: events.into(),
            clip,
        }
    }

    pub fn update(&mut self, elapsed: f64) {
        self.elapsed = self.elapsed.max(elapsed);
        let width = self.bounds.end - self.bounds.start;
        let phase = self.offset + self.elapsed;
        let remainder = phase.rem_euclid(width);
        // Recover the integral quotient from the remainder. A rounded
        // division can otherwise select the next cycle while % stays in this one.
        let cycle = ((phase - remainder) / width).round() as u64;
        self.progress = AnimationRangeProgress {
            cycle,
            local: ((self.bounds.start + remainder) as f32).clamp(
                self.bounds.start as f32,
                (self.bounds.end as f32).next_down(),
            ),
        };
    }

    pub fn sample_time(&self, mode: AnimationPreviewMode) -> Result<f32> {
        if mode == AnimationPreviewMode::Editing {
            return Ok(self.progress.local);
        }
        Ok(zipzi_animation::canonical_motion_seek_time(
            &self.clip,
            self.sdk_cycle,
            self.progress.local,
        )?)
    }

    pub fn present(&mut self, progress: AnimationRangeProgress) -> AnimationRangeEvents {
        let from = self
            .shown
            .replace(progress)
            .unwrap_or(AnimationRangeProgress {
                cycle: 0,
                local: (self.bounds.start + self.offset) as f32,
            });
        let first = self.events.partition_point(|e| {
            e.time < from.local || (e.time == from.local && !self.include_first)
        });
        self.include_first = false;
        let last = self.events.partition_point(|e| e.time <= progress.local);
        if from.cycle == progress.cycle {
            return AnimationRangeEvents {
                head: self.events[first..last].to_vec(),
                ..Default::default()
            };
        }
        let full_cycles = progress.cycle - from.cycle - 1;
        AnimationRangeEvents {
            tail: self.events[first..].to_vec(),
            full_cycles,
            cycle_events: if full_cycles == 0 {
                Vec::new()
            } else {
                self.events.to_vec()
            },
            head: self.events[..last].to_vec(),
        }
    }
}

impl AnimationSession {
    pub fn is_range_playing(&self) -> bool {
        self.range_playback.is_some()
    }

    /// The desktop wall clock anchors this value, applying playback speed once.
    pub fn playback_clock_time(&self) -> f64 {
        self.range_playback
            .as_ref()
            .map_or(f64::from(self.time), |run| run.elapsed)
    }

    pub fn range_events(&self) -> &AnimationRangeEvents {
        &self.range_events
    }

    pub(crate) fn reset_range_playback(&mut self) {
        self.range_playback = None;
        self.range_resume = None;
        self.range_events = AnimationRangeEvents::default();
        self.barrier();
    }
}
