use crate::parameter_values::{repeat_value, set_value};
use crate::pose::real_parameter_for_part;
use crate::MotionSnapshot;
use zipzi_core::{
    document::{MotionClip, MotionTrack, MotionTrackTarget, MOTION_EFFECT_GROUPS},
    state::DocumentState,
    Parameter,
};

pub(crate) fn apply_motion_tracks(
    document: &DocumentState,
    clip: &MotionClip,
    sample: impl Fn(&MotionTrack) -> f32,
    motion_weight: f32,
    track_weight: impl Fn(&MotionTrack) -> f32,
    snapshot: &mut MotionSnapshot,
) {
    let settings = document.model3_settings();
    let mut effects = [None, None];
    for track in &clip.tracks {
        if let MotionTrackTarget::Model { runtime_id } = &track.target {
            if runtime_id == "Opacity" {
                snapshot.model_opacity = sample(track);
            } else if let Some(index) = MOTION_EFFECT_GROUPS.iter().position(|id| id == runtime_id)
            {
                if settings
                    .motion_effect_parameters(runtime_id)
                    .next()
                    .is_some()
                {
                    effects[index] = Some(sample(track));
                } else {
                    snapshot.coverage.push(format!(
                        "{}: Model {runtime_id} mapping requires model3 Groups",
                        clip.id
                    ));
                }
            } else {
                snapshot.coverage.push(format!(
                    "{}: Model {runtime_id} mapping requires model3 Groups",
                    clip.id
                ));
            }
        }
    }
    let [blink, lip_sync] = effects;
    let has_effects = blink.is_some() || lip_sync.is_some();
    for track in &clip.tracks {
        match &track.target {
            MotionTrackTarget::Parameter { parameter_id } => {
                let parameter = document
                    .parameter(parameter_id)
                    .expect("resolved parameter");
                let blink = blink.filter(|_| {
                    settings
                        .motion_effect_parameters("EyeBlink")
                        .any(|id| id == parameter_id)
                });
                let lip_sync = lip_sync.filter(|_| {
                    settings
                        .motion_effect_parameters("LipSync")
                        .any(|id| id == parameter_id)
                });
                let mut value = sample(track);
                if let Some(blink) = blink {
                    value *= blink;
                }
                if let Some(lip_sync) = lip_sync {
                    value += lip_sync;
                }
                let driven = blink.is_some() || lip_sync.is_some();
                if driven {
                    value = repeat_value(parameter, value);
                }
                apply_value(snapshot, parameter, value, track_weight(track), driven);
            }
            MotionTrackTarget::PartOpacity { part_id } if !has_effects => {
                apply_part_opacity(document, clip, part_id, sample(track), snapshot);
            }
            MotionTrackTarget::Model { .. } | MotionTrackTarget::PartOpacity { .. } => {}
            MotionTrackTarget::Unspecified => snapshot.coverage.push(format!(
                "motion {} track {} needs configuration",
                clip.id, track.id
            )),
            MotionTrackTarget::Unresolved {
                category,
                runtime_id,
            } => snapshot.coverage.push(format!(
                "{}: virtual {category} {runtime_id} has no drawable target",
                clip.id
            )),
        }
    }
    if has_effects {
        // Native SDK applies unkeyed members in EyeBlink, LipSync order, then Parts.
        for (group, effect) in MOTION_EFFECT_GROUPS.into_iter().zip(effects) {
            if let Some(value) = effect {
                for id in settings.motion_effect_parameters(group) {
                    let keyed = clip.tracks.iter().any(|track| {
                        matches!(&track.target, MotionTrackTarget::Parameter { parameter_id } if parameter_id == id)
                    });
                    if !keyed {
                        let parameter = document.parameter(id).expect("resolved effect member");
                        apply_value(snapshot, parameter, value, motion_weight, true);
                    }
                }
            }
        }
        for track in &clip.tracks {
            if let MotionTrackTarget::PartOpacity { part_id } = &track.target {
                apply_part_opacity(document, clip, part_id, sample(track), snapshot);
            }
        }
    }
}

fn apply_value(
    snapshot: &mut MotionSnapshot,
    parameter: &Parameter,
    value: f32,
    weight: f32,
    effect: bool,
) {
    let current = snapshot.parameters[&parameter.id];
    let value = if weight == 1. {
        value
    } else if weight == 0. {
        current
    } else {
        current + (value - current) * weight
    };
    let value = if effect && parameter.repeat {
        repeat_value(parameter, value)
    } else {
        value.clamp(parameter.minimum, parameter.maximum)
    };
    set_value(&mut snapshot.parameters, &parameter.id, value);
}

fn apply_part_opacity(
    document: &DocumentState,
    clip: &MotionClip,
    part_id: &str,
    value: f32,
    snapshot: &mut MotionSnapshot,
) {
    if let Some(parameter_id) = real_parameter_for_part(document, part_id) {
        let parameter = document
            .parameter(&parameter_id)
            .expect("resolved parameter");
        set_value(
            &mut snapshot.parameters,
            &parameter_id,
            value.clamp(parameter.minimum, parameter.maximum),
        );
    } else {
        set_value(&mut snapshot.part_opacity_channels, part_id, value);
    }
    if document.pose().is_none() {
        snapshot.coverage.push(format!(
            "{}: PartOpacity {part_id} has no Pose asset",
            clip.id
        ));
    }
}
