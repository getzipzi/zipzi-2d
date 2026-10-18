//! motion3 conversion between runtime IDs and persistent document UUIDs.
use std::collections::{BTreeMap, HashMap};

use kasane_core::document::{
    MotionClip, MotionEvent, MotionKey, MotionPoint, MotionSegment, MotionTrack, MotionTrackTarget,
};
use kasane_core::Document;
use kasane_live2d::motion3::{
    decode_motion3, encode_motion3, Motion3, Motion3Curve, Motion3Event, Motion3Meta,
    MotionPoint as WirePoint, MotionSegment as WireSegment, MotionTarget,
};
use sha2::{Digest, Sha256};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MotionDiagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug)]
pub struct MotionImport {
    pub candidate: Document,
    pub diagnostics: Vec<MotionDiagnostic>,
}

#[derive(Debug, Error)]
#[error("Motion {code} at {path}: {message}")]
pub struct MotionProjectError {
    pub code: String,
    pub path: String,
    pub message: String,
}

fn error(
    code: impl Into<String>,
    path: impl Into<String>,
    message: impl Into<String>,
) -> MotionProjectError {
    MotionProjectError {
        code: code.into(),
        path: path.into(),
        message: message.into(),
    }
}

fn child_id(parent: &str, kind: &str, index: usize) -> String {
    let mut hash = Sha256::new();
    hash.update(parent.as_bytes());
    hash.update(kind.as_bytes());
    hash.update(index.to_le_bytes());
    let mut bytes: [u8; 16] = hash.finalize()[..16]
        .try_into()
        .expect("SHA-256 has 16 bytes");
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    )
}

fn point(point: WirePoint) -> MotionPoint {
    MotionPoint {
        time: point.time,
        value: point.value,
    }
}
fn key(point: WirePoint) -> MotionKey {
    MotionKey {
        id: uuid::Uuid::new_v4().to_string(),
        time: point.time,
        value: point.value,
    }
}
fn wire_point(point: MotionPoint) -> WirePoint {
    WirePoint {
        time: point.time,
        value: point.value,
    }
}

fn segment(segment: WireSegment) -> MotionSegment {
    match segment {
        WireSegment::Linear { end } => MotionSegment::Linear { end: key(end) },
        WireSegment::Bezier {
            control1,
            control2,
            end,
        } => MotionSegment::Bezier {
            control1: point(control1),
            control2: point(control2),
            end: key(end),
        },
        WireSegment::Stepped { end } => MotionSegment::Stepped { end: key(end) },
        WireSegment::InverseStepped { end } => MotionSegment::InverseStepped { end: key(end) },
    }
}

fn wire_segment(segment: &MotionSegment) -> WireSegment {
    match segment {
        MotionSegment::Linear { end } => WireSegment::Linear {
            end: wire_point(end.point()),
        },
        MotionSegment::Bezier {
            control1,
            control2,
            end,
        } => WireSegment::Bezier {
            control1: wire_point(*control1),
            control2: wire_point(*control2),
            end: wire_point(end.point()),
        },
        MotionSegment::Stepped { end } => WireSegment::Stepped {
            end: wire_point(end.point()),
        },
        MotionSegment::InverseStepped { end } => WireSegment::InverseStepped {
            end: wire_point(end.point()),
        },
    }
}

fn namespace(document: &Document, clip: Option<&MotionClip>) -> BTreeMap<String, String> {
    let mut ids = BTreeMap::new();
    for id in document.parameter_order() {
        ids.insert(
            format!("parameter:{id}"),
            document
                .get_parameter(id)
                .expect("ordered parameter")
                .runtime_id
                .clone(),
        );
    }
    for id in document.part_order() {
        ids.insert(
            format!("part:{id}"),
            document
                .get_part(id)
                .expect("ordered part")
                .runtime_id
                .clone(),
        );
    }
    if let Some(clip) = clip {
        for track in &clip.tracks {
            if let MotionTrackTarget::Unresolved {
                category,
                runtime_id,
            } = &track.target
            {
                ids.insert(
                    format!("unresolved:{category}:{runtime_id}"),
                    runtime_id.clone(),
                );
            }
        }
    }
    ids
}

fn content_hash(clip: &MotionClip) -> String {
    // Key UUIDs are project identity, not unknown motion3 content. Replacing
    // their identities alone must not invalidate an opaque import baseline.
    let tracks: Vec<_> = clip
        .tracks
        .iter()
        .map(|track| {
            (
                &track.id,
                &track.target,
                track.initial.point(),
                track.segments.iter().map(wire_segment).collect::<Vec<_>>(),
                track.fade_in,
                track.fade_out,
                &track.extensions,
            )
        })
        .collect();
    let bytes = serde_json::to_vec(&(
        &clip.name,
        clip.duration,
        clip.fps,
        clip.looping,
        clip.restricted_beziers,
        clip.fade_in,
        clip.fade_out,
        &tracks,
        &clip.events,
        &clip.extensions,
        &clip.meta_extensions,
    ))
    .expect("validated motion content serializes");
    crate::content_sha256(&bytes)
}

pub fn import_motion3(
    document: &Document,
    id: &str,
    name: &str,
    text: &str,
) -> Result<MotionImport, MotionProjectError> {
    if !document.initialized() {
        return Err(error(
            "NOT_INITIALIZED",
            "$",
            "initialize the document first",
        ));
    }
    let wire = decode_motion3(text).map_err(|e| error(e.code, e.path, e.message))?;
    let mut diagnostics = Vec::new();
    if !wire.counts_match() {
        diagnostics.push(MotionDiagnostic {
            code: "MOTION_COUNT_MISMATCH".into(),
            path: "$.Meta".into(),
            message: "stored counts differ from curve and event data; export will recompute them"
                .into(),
        });
    }
    let parameters: HashMap<_, _> = document
        .parameter_order()
        .iter()
        .map(|id| {
            (
                document
                    .get_parameter(id)
                    .expect("ordered parameter")
                    .runtime_id
                    .as_str(),
                id.as_str(),
            )
        })
        .collect();
    let parts: HashMap<_, _> = document
        .part_order()
        .iter()
        .map(|id| {
            (
                document
                    .get_part(id)
                    .expect("ordered part")
                    .runtime_id
                    .as_str(),
                id.as_str(),
            )
        })
        .collect();
    let tracks = wire
        .curves
        .into_iter()
        .enumerate()
        .map(|(index, curve)| {
            let target = match curve.target {
                MotionTarget::Model => MotionTrackTarget::Model {
                    runtime_id: curve.id,
                },
                MotionTarget::Parameter => {
                    if let Some(id) = parameters.get(curve.id.as_str()) {
                        MotionTrackTarget::Parameter {
                            parameter_id: (*id).into(),
                        }
                    } else {
                        diagnostics.push(MotionDiagnostic {
                            code: "UNRESOLVED_PARAMETER".into(),
                            path: format!("$.Curves[{index}].Id"),
                            message: format!("{} is absent from the model", curve.id),
                        });
                        MotionTrackTarget::Unresolved {
                            category: "Parameter".into(),
                            runtime_id: curve.id,
                        }
                    }
                }
                MotionTarget::PartOpacity => {
                    if let Some(id) = parts.get(curve.id.as_str()) {
                        MotionTrackTarget::PartOpacity {
                            part_id: (*id).into(),
                        }
                    } else {
                        diagnostics.push(MotionDiagnostic {
                            code: "UNRESOLVED_PART".into(),
                            path: format!("$.Curves[{index}].Id"),
                            message: format!("{} is absent from the model", curve.id),
                        });
                        MotionTrackTarget::Unresolved {
                            category: "PartOpacity".into(),
                            runtime_id: curve.id,
                        }
                    }
                }
            };
            MotionTrack {
                id: child_id(id, "track", index),
                target,
                initial: key(curve.initial),
                segments: std::sync::Arc::new(curve.segments.into_iter().map(segment).collect()),
                fade_in: curve.fade_in,
                fade_out: curve.fade_out,
                extensions: curve.extensions,
            }
        })
        .collect();
    let events = wire
        .user_data
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, event)| MotionEvent {
            id: child_id(id, "event", index),
            time: event.time,
            value: event.value,
            extensions: event.extensions,
        })
        .collect();
    let mut clip = MotionClip {
        id: id.into(),
        name: name.into(),
        duration: wire.meta.duration,
        fps: wire.meta.fps,
        looping: wire.meta.looping,
        restricted_beziers: wire.meta.restricted_beziers,
        fade_in: wire.meta.fade_in,
        fade_out: wire.meta.fade_out,
        tracks,
        events,
        extensions: wire.extensions,
        meta_extensions: wire.meta.extensions,
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    };
    if clip.has_extensions() {
        clip.opaque_source_ids = Some(namespace(document, Some(&clip)));
        clip.opaque_source_content_hash = Some(content_hash(&clip));
    }
    let mut candidate = document.fork_candidate();
    let result = if candidate.get_motion(id).is_some() {
        candidate.replace_motion(clip)
    } else {
        candidate.create_motion(clip)
    };
    if !result.status.is_ok() {
        return Err(error(result.status.code, "$", result.status.message));
    }
    Ok(MotionImport {
        candidate,
        diagnostics,
    })
}

pub fn export_motion3(document: &Document, id: &str) -> Result<String, MotionProjectError> {
    let clip = document
        .get_motion(id)
        .ok_or_else(|| error("MISSING_MOTION", "$", id))?;
    if clip.has_extensions() {
        if clip.opaque_source_ids.as_ref() != Some(&namespace(document, Some(clip))) {
            return Err(error(
                "OPAQUE_NAMESPACE_CHANGED",
                "$",
                "runtime ID namespace changed since import",
            ));
        }
        if clip.opaque_source_content_hash.as_deref() != Some(content_hash(clip).as_str()) {
            return Err(error("OPAQUE_CONTENT_CHANGED", "$", "motion content changed since import; reimport to establish an opaque-field baseline"));
        }
    }
    let curves = clip
        .tracks
        .iter()
        .enumerate()
        .map(|(index, track)| {
            let path = format!("$.Curves[{index}].Id");
            let (target, runtime_id) = match &track.target {
                MotionTrackTarget::Model { runtime_id } => {
                    (MotionTarget::Model, runtime_id.clone())
                }
                MotionTrackTarget::Parameter { parameter_id } => (
                    MotionTarget::Parameter,
                    document
                        .get_parameter(parameter_id)
                        .ok_or_else(|| error("MISSING_PARAMETER", &path, parameter_id))?
                        .runtime_id
                        .clone(),
                ),
                MotionTrackTarget::PartOpacity { part_id } => (
                    MotionTarget::PartOpacity,
                    document
                        .get_part(part_id)
                        .ok_or_else(|| error("MISSING_PART", &path, part_id))?
                        .runtime_id
                        .clone(),
                ),
                MotionTrackTarget::Unresolved {
                    category,
                    runtime_id,
                } => (
                    match category.as_str() {
                        "Parameter" => MotionTarget::Parameter,
                        "PartOpacity" => MotionTarget::PartOpacity,
                        _ => return Err(error("INVALID_TARGET", path, category)),
                    },
                    runtime_id.clone(),
                ),
            };
            let (initial, segments) = if clip.source_key_range().is_some() {
                runtime_curve(track, clip.duration, clip.restricted_beziers)
            } else {
                (
                    wire_point(track.initial.point()),
                    track.segments.iter().map(wire_segment).collect(),
                )
            };
            Ok(Motion3Curve {
                target,
                id: runtime_id,
                initial,
                segments,
                fade_in: track.fade_in,
                fade_out: track.fade_out,
                extensions: track.extensions.clone(),
            })
        })
        .collect::<Result<Vec<_>, MotionProjectError>>()?;
    let events = clip
        .events
        .iter()
        .map(|event| Motion3Event {
            time: event.time,
            value: event.value.clone(),
            extensions: event.extensions.clone(),
        })
        .collect();
    let wire = Motion3 {
        version: 3,
        meta: Motion3Meta {
            duration: clip.duration,
            fps: clip.fps,
            looping: clip.looping,
            restricted_beziers: clip.restricted_beziers,
            curve_count: 0,
            total_segment_count: 0,
            total_point_count: 0,
            user_data_count: 0,
            total_user_data_size: 0,
            fade_in: clip.fade_in,
            fade_out: clip.fade_out,
            extensions: clip.runtime_meta_extensions(),
        },
        curves,
        user_data: Some(events),
        extensions: clip.extensions.clone(),
    };
    encode_motion3(&wire).map_err(|e| error(e.code, e.path, e.message))
}

/// Runtime motion files contain the playback interval. Keep source-authoring
/// pre-roll/tail keys in Document and split their curves exactly at the boundary.
fn runtime_curve(
    track: &MotionTrack,
    duration: f32,
    restricted: bool,
) -> (WirePoint, Vec<WireSegment>) {
    let mut initial = wire_point(track.initial.point());
    let mut previous = track.initial.point();
    let mut segments = Vec::new();
    let lerp = |a: MotionPoint, b: MotionPoint, t: f32| MotionPoint {
        time: a.time + (b.time - a.time) * t,
        value: a.value + (b.value - a.value) * t,
    };
    let split = |p: [MotionPoint; 4], t: f32| {
        let a = lerp(p[0], p[1], t);
        let b = lerp(p[1], p[2], t);
        let c = lerp(p[2], p[3], t);
        let d = lerp(a, b, t);
        let e = lerp(b, c, t);
        let mid = lerp(d, e, t);
        ([p[0], a, d, mid], [mid, e, c, p[3]])
    };
    let parameter = |p: [MotionPoint; 4], time: f32| {
        if restricted {
            return ((time - p[0].time) / (p[3].time - p[0].time)).clamp(0., 1.);
        }
        let mut lo = 0.;
        let mut hi = 1.;
        for _ in 0..32 {
            let t = (lo + hi) * 0.5;
            let value = split(p, t).0[3].time;
            if value < time {
                lo = t;
            } else {
                hi = t;
            }
        }
        (lo + hi) * 0.5
    };
    for segment in track.segments.iter() {
        let end = segment.end().point();
        if end.time <= 0. {
            previous = end;
            initial = wire_point(end);
            continue;
        }
        if previous.time >= duration {
            break;
        }
        let left = previous.time.max(0.);
        let right = end.time.min(duration);
        if right < left {
            previous = end;
            continue;
        }
        let (start, wire) = match segment {
            MotionSegment::Bezier {
                control1, control2, ..
            } => {
                let mut p = [previous, *control1, *control2, end];
                if restricted {
                    let span = end.time - previous.time;
                    p[1].time = previous.time + span / 3.;
                    p[2].time = previous.time + 2. * span / 3.;
                }
                if right < end.time {
                    p = split(p, parameter(p, right)).0;
                }
                if left > previous.time {
                    p = split(p, parameter(p, left)).1;
                }
                p[0].time = left;
                p[3].time = right;
                (
                    p[0],
                    WireSegment::Bezier {
                        control1: wire_point(p[1]),
                        control2: wire_point(p[2]),
                        end: wire_point(p[3]),
                    },
                )
            }
            MotionSegment::Linear { .. } => {
                let value = |time: f32| {
                    lerp(
                        previous,
                        end,
                        ((time - previous.time) / (end.time - previous.time)).clamp(0., 1.),
                    )
                    .value
                };
                (
                    MotionPoint {
                        time: left,
                        value: value(left),
                    },
                    WireSegment::Linear {
                        end: WirePoint {
                            time: right,
                            value: value(right),
                        },
                    },
                )
            }
            MotionSegment::Stepped { .. } => (
                MotionPoint {
                    time: left,
                    value: previous.value,
                },
                WireSegment::Stepped {
                    end: WirePoint {
                        time: right,
                        value: if right == end.time {
                            end.value
                        } else {
                            previous.value
                        },
                    },
                },
            ),
            MotionSegment::InverseStepped { .. } => (
                MotionPoint {
                    time: left,
                    value: if left == previous.time {
                        previous.value
                    } else {
                        end.value
                    },
                },
                WireSegment::InverseStepped {
                    end: WirePoint {
                        time: right,
                        value: end.value,
                    },
                },
            ),
        };
        if segments.is_empty() {
            initial = wire_point(start);
        }
        if right > left {
            segments.push(wire);
        }
        previous = end;
        if end.time >= duration {
            break;
        }
    }
    if initial.time < 0. || initial.time > duration {
        initial.time = 0.;
    }
    (initial, segments)
}

#[cfg(test)]
mod source_range_tests {
    use super::*;
    fn track(segment: MotionSegment) -> MotionTrack {
        MotionTrack {
            id: "track".into(),
            target: MotionTrackTarget::Model {
                runtime_id: "Opacity".into(),
            },
            initial: MotionKey {
                id: "first".into(),
                time: -1.,
                value: 0.,
            },
            segments: vec![segment].into(),
            fade_in: None,
            fade_out: None,
            extensions: Default::default(),
        }
    }
    #[test]
    fn source_preroll_and_tail_cubic_are_split_without_changing_playback_values() {
        // y(u)=u², time(u)=-1+3u. After cropping, y(t)=((t+1)/3)².
        let source = track(MotionSegment::Bezier {
            control1: MotionPoint {
                time: 0.,
                value: 0.,
            },
            control2: MotionPoint {
                time: 1.,
                value: 1. / 3.,
            },
            end: MotionKey {
                id: "last".into(),
                time: 2.,
                value: 1.,
            },
        });
        for restricted in [true, false] {
            let (initial, segments) = runtime_curve(&source, 1., restricted);
            assert_eq!(initial.time, 0.);
            let WireSegment::Bezier {
                control1,
                control2,
                end,
            } = &segments[0]
            else {
                panic!("cubic expected")
            };
            assert_eq!(end.time, 1.);
            for t in [0f32, 0.25, 0.5, 0.75, 1.] {
                let actual = (1. - t).powi(3) * initial.value
                    + 3. * (1. - t).powi(2) * t * control1.value
                    + 3. * (1. - t) * t * t * control2.value
                    + t.powi(3) * end.value;
                assert!((actual - ((t + 1.) / 3.).powi(2)).abs() < 1e-5);
            }
        }
        assert_eq!(source.initial.time, -1.);
        assert_eq!(source.segments[0].end().time, 2.);
    }
    #[test]
    fn cropped_steps_preserve_the_value_inside_the_playback_interval() {
        let end = MotionKey {
            id: "last".into(),
            time: 2.,
            value: 9.,
        };
        let (initial, segments) = runtime_curve(
            &track(MotionSegment::Stepped { end: end.clone() }),
            1.,
            true,
        );
        assert_eq!(initial.value, 0.);
        assert_eq!(segments[0].end().value, 0.);
        let (initial, segments) =
            runtime_curve(&track(MotionSegment::InverseStepped { end }), 1., true);
        assert_eq!(initial.value, 9.);
        assert_eq!(segments[0].end().value, 9.);
    }
}
