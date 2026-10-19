//! Animator has integer anchors. Sample incompatible tracks before quantizing,
//! rather than throwing away keys that happen to round to the same frame.
use super::*;
use kasane_animation::{sample_motion_curve, CompiledCurve};

pub(super) fn info(report: &mut Cmo3Report, id: &str, code: &str, message: &str) {
    report.diagnostics.push(Cmo3Diagnostic {
        severity: DiagnosticSeverity::Info,
        object_id: id.into(),
        code: code.into(),
        message: message.into(),
    });
}

pub(super) fn needs_baking(track: &MotionTrack, fps: f32) -> bool {
    let mut previous = None;
    track.keys().any(|key| {
        let frame = f64::from(key.time) * f64::from(fps);
        let collision = previous == Some(frame.round());
        previous = Some(frame.round());
        collision || (frame - frame.round()).abs() > 0.0001
    }) || track.segments.iter().enumerate().any(|(index, segment)| {
        matches!(segment, MotionSegment::InverseStepped { .. })
            && segment.end().value
                != if index == 0 {
                    track.initial.value
                } else {
                    track.segments[index - 1].end().value
                }
    })
}

pub(super) fn keys(
    track: &MotionTrack,
    clip: &MotionClip,
    report: &mut Cmo3Report,
    budget: &mut usize,
) -> Result<Vec<Key>, Cmo3Error> {
    let fps = f64::from(clip.fps);
    let start = (f64::from(track.initial.time) * fps).floor();
    let end = (f64::from(track.keys().last().unwrap().time) * fps).ceil();
    // Bound work before allocating, including keys outside the scene range.
    if start < f64::from(i32::MIN) || end > f64::from(i32::MAX) || end - start > 1_000_000. {
        return Err(Cmo3Error::InvalidDocument(format!(
            "{}: CAN3 frame baking exceeds the supported range or one million samples",
            track.id
        )));
    }
    let count = (end - start) as usize + 1;
    *budget = budget.checked_sub(count).ok_or_else(|| {
        Cmo3Error::InvalidDocument("CAN3 animation baking exceeds ten million samples".into())
    })?;
    let curve = CompiledCurve::from(track);
    let mut result: Vec<Key> = Vec::new();
    for frame in start as i32..=end as i32 {
        let value = f64::from(sample_motion_curve(
            &curve,
            (f64::from(frame) / fps) as f32,
            clip.restricted_beziers,
        ));
        // Collapse only exactly constant runs; every nonconstant displayed
        // frame remains an explicit key, so no fitting error can accumulate.
        if result.len() >= 2
            && result[result.len() - 2].value == value
            && result[result.len() - 1].value == value
        {
            result.pop();
        }
        result.push(Key {
            frame,
            value,
            curve: "LINEAR",
            prev: (f64::from(frame), value),
            next: (f64::from(frame), value),
        });
    }
    info(report, &track.id, "CAN3_BAKED_TRACK", "Curve sampled at the original FPS to retain every displayed frame without merging source keys; subframe interpolation and original curve editing may differ");
    Ok(result)
}
