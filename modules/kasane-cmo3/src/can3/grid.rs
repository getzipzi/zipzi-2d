//! Infer anchor grids independently of motion metadata. Runtime exporters often
//! round seconds to milliseconds while leaving Meta.Fps at a different rate.
use super::*;

pub(super) fn fits(time: f32, fps: f32) -> bool {
    let time = f64::from(time);
    let snapped = (time * f64::from(fps)).round() / f64::from(fps);
    let noise = f64::from(f32::EPSILON) * time.abs().max(1.);
    (time - snapped).abs() <= noise
        || ((time * 1000. - (time * 1000.).round()).abs() <= noise * 1000.
            && (time * 1000.).round() == (snapped * 1000.).round())
}

fn clip_fits(clip: &MotionClip, fps: f32) -> bool {
    let mut events = BTreeSet::new();
    fits(clip.duration, fps)
        && clip.events.iter().all(|e| {
            fits(e.time, fps) && events.insert((f64::from(e.time) * f64::from(fps)).round() as i64)
        })
        && clip.tracks.iter().all(|track| {
            let mut previous = None;
            track.keys().all(|key| {
                let frame = (f64::from(key.time) * f64::from(fps)).round();
                let increasing = previous.is_none_or(|p| frame > p);
                previous = Some(frame);
                increasing && fits(key.time, fps)
            })
        })
}

pub(super) fn maximum_shift(clip: &MotionClip, fps: f32) -> f64 {
    clip.tracks
        .iter()
        .flat_map(|track| track.keys().map(|key| key.time))
        .chain(clip.events.iter().map(|event| event.time))
        .chain(std::iter::once(clip.duration))
        .map(|time| {
            (f64::from(time) - (f64::from(time) * f64::from(fps)).round() / f64::from(fps)).abs()
        })
        .fold(0., f64::max)
}

/// Keep groups with different declared rates independent. Within each group,
/// prefer the declared rate, then the smallest adequate common rate or integer
/// multiple of the declared rate, bounded to 240 FPS. Sparse millisecond data
/// must not accidentally select an unusual rate such as 58 instead of 60.
pub(super) fn detect(clips: &[&MotionClip]) -> BTreeMap<u32, f32> {
    let mut groups = BTreeMap::<u32, Vec<&MotionClip>>::new();
    for clip in clips {
        groups.entry(clip.fps.to_bits()).or_default().push(clip);
    }
    groups
        .into_iter()
        .filter_map(|(bits, clips)| {
            let declared = f32::from_bits(bits);
            let mut candidates = vec![
                24., 25., 30., 48., 50., 60., 72., 75., 90., 96., 100., 120., 144., 150., 180.,
                192., 200., 240.,
            ];
            candidates.extend(
                (1..=8)
                    .map(|multiple| declared * multiple as f32)
                    .filter(|fps| *fps <= 240.),
            );
            candidates.retain(|fps| *fps >= declared);
            candidates.sort_by(f32::total_cmp);
            candidates.dedup();
            let fps = std::iter::once(declared)
                .chain(candidates)
                .find(|fps| clips.iter().all(|clip| clip_fits(clip, *fps)))?;
            Some((bits, fps))
        })
        .collect()
}

pub(super) fn frame(
    time: f32,
    fps: f32,
    id: &str,
    report: &mut Cmo3Report,
    inferred: bool,
) -> Result<i32, Cmo3Error> {
    if inferred && fits(time, fps) {
        // Overflow validation stays shared with the strict path. Recognized
        // timestamp rounding is reported once per scene rather than per key.
        super::frame(time, fps, id, &mut Cmo3Report::default())
    } else {
        super::frame(time, fps, id, report)
    }
}
