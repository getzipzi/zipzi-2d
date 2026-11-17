use super::*;
use zipzi_animation::{CompiledSegment, motion_frame_step_boundary, sample_motion_curve};
use zipzi_core::document::{MotionFrameStep, MotionPoint};

impl TimelineProjection {
    /// Visible sample-and-hold geometry. Grid lookup is logarithmic in time;
    /// subpixel steps are coalesced to a bounded number of visible edges.
    /// These points never enter the key list or editing hit targets.
    pub fn frame_step_points(
        &self,
        track: &TimelineTrack,
        index: usize,
        left: f32,
        right: f32,
        max_boundaries: usize,
    ) -> Option<Vec<MotionPoint>> {
        let MotionFrameStep::EveryFrames { frames: 2.. } = track.curve.frame_step(index) else {
            return None;
        };
        let MotionFrameStep::EveryFrames { frames } = track.curve.frame_step(index) else {
            unreachable!()
        };
        if matches!(
            track.curve.segments[index],
            CompiledSegment::Stepped { .. } | CompiledSegment::InverseStepped { .. }
        ) {
            return None;
        }
        let origin = track.keys[index].time as f32;
        let boundary =
            |q| motion_frame_step_boundary(origin, self.fps, frames, q).unwrap_or(f32::INFINITY);
        let lower = |time: f32, inclusive: bool| {
            let (mut lo, mut hi) = (0_u64, (1_u64 << 53) - 1);
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                let t = boundary(mid);
                if t < time || (!inclusive && t == time) {
                    lo = mid + 1;
                } else {
                    hi = mid;
                }
            }
            lo
        };
        let first = lower(left, false);
        let end = lower(right, true);
        let stride = (end.saturating_sub(first))
            .div_ceil(max_boundaries.clamp(1, 16384) as u64)
            .max(1);
        let sample = |time| sample_motion_curve(&track.curve, time, self.restricted_beziers);
        let mut points = vec![MotionPoint {
            time: left,
            value: sample(left),
        }];
        let mut q = first;
        while q < end {
            let time = boundary(q);
            let previous = *points.last().unwrap();
            if time > previous.time && time < right {
                points.push(MotionPoint {
                    time,
                    value: previous.value,
                });
                points.push(MotionPoint {
                    time,
                    value: sample(time),
                });
            }
            q += stride;
        }
        points.push(MotionPoint {
            time: right,
            value: points.last().unwrap().value,
        });
        // Exact right boundaries still follow the outgoing source interval.
        points.push(MotionPoint {
            time: right,
            value: sample(right),
        });
        Some(points)
    }
}
