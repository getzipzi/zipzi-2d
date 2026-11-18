//! motion3 conversion between runtime IDs and persistent document UUIDs.
use crate::export_preflight::OpaqueExportProblemCode;
use std::collections::{BTreeMap, HashMap};
use zipzi_core::document::MotionCurve;

use sha2::{Digest, Sha256};
use thiserror::Error;
use zipzi_core::document::{
    MotionClip, MotionEvent, MotionKey, MotionPoint, MotionSegment, MotionTrack, MotionTrackTarget,
};
use zipzi_core::state::DocumentState;
use zipzi_live2d::motion3::{
    decode_motion3, encode_motion3, Motion3, Motion3Curve, Motion3Event, Motion3Meta,
    MotionPoint as WirePoint, MotionSegment as WireSegment, MotionTarget,
};

mod bake;

/// Admission bounds for numeric curve compilation, Step expansion and encoding.
#[derive(Debug, Clone, Copy)]
pub struct MotionExportLimits {
    pub max_output_segments: usize,
    pub max_workspace_bytes: usize,
}
impl Default for MotionExportLimits {
    fn default() -> Self {
        Self {
            max_output_segments: 1_000_000,
            max_workspace_bytes: 256 << 20,
        }
    }
}

/// Curve-lowering evidence returned alongside motion3 bytes and package reports.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct MotionExportSummary {
    pub source_segments: usize,
    pub estimated_output_segments: usize,
    pub output_segments: usize,
    pub auto_smooth_segments: usize,
    pub frame_step_intervals: usize,
    /// Generated hold spans before boundary-kind repairs.
    pub frame_step_segments: usize,
    pub boundary_segments: usize,
    pub estimated_workspace_bytes: usize,
}
impl MotionExportSummary {
    pub fn notice(&self, name: &str) -> Option<String> {
        (self.frame_step_intervals > 0 || self.auto_smooth_segments > 0).then(|| format!(
            "{name}: compiled {} Auto Smooth segments and baked {} Frame Step intervals; motion3 contains {} segments ({} boundary segments)",
            self.auto_smooth_segments, self.frame_step_intervals, self.output_segments, self.boundary_segments,
        ))
    }
}

#[derive(Debug)]
pub struct MotionExport {
    pub json: String,
    pub summary: MotionExportSummary,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MotionDiagnostic {
    pub code: String,
    pub path: String,
    pub message: String,
}

#[derive(Debug)]
pub struct MotionImport {
    pub candidate: DocumentState,
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
        WireSegment::Linear { end } => MotionSegment::new(key(end), MotionCurve::Linear {}),
        WireSegment::Bezier {
            control1,
            control2,
            end,
        } => MotionSegment::new(
            key(end),
            MotionCurve::Bezier {
                control1: point(control1),
                control2: point(control2),
                preset: None,
            },
        ),
        WireSegment::Stepped { end } => MotionSegment::new(key(end), MotionCurve::Stepped {}),
        WireSegment::InverseStepped { end } => {
            MotionSegment::new(key(end), MotionCurve::InverseStepped {})
        }
    }
}

fn wire_segment(segment: &zipzi_animation::CompiledSegment) -> WireSegment {
    use zipzi_animation::CompiledSegment as S;
    match segment {
        S::Linear { end } => WireSegment::Linear {
            end: wire_point(*end),
        },
        S::Bezier {
            control1,
            control2,
            end,
        } => WireSegment::Bezier {
            control1: wire_point(*control1),
            control2: wire_point(*control2),
            end: wire_point(*end),
        },
        S::Stepped { end } => WireSegment::Stepped {
            end: wire_point(*end),
        },
        S::InverseStepped { end } => WireSegment::InverseStepped {
            end: wire_point(*end),
        },
    }
}

fn namespace(document: &DocumentState, clip: Option<&MotionClip>) -> BTreeMap<String, String> {
    let mut ids = BTreeMap::new();
    for id in document.parameter_order() {
        ids.insert(
            format!("parameter:{id}"),
            document
                .parameter(id)
                .expect("ordered parameter")
                .runtime_id
                .clone(),
        );
    }
    for id in document.part_order() {
        ids.insert(
            format!("part:{id}"),
            document.part(id).expect("ordered part").runtime_id.clone(),
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
    content_hash_with(clip, |track| {
        track
            .segments
            .iter()
            .map(|s| (&s.curve, s.frame_step, s.end.point()))
            .collect()
    })
}

const V22_HASH_PREFIX: &str = "v22:";

pub(crate) fn upgrade_v22_content_hash(clip: &mut MotionClip) {
    let Some(stored) = clip.opaque_source_content_hash.take() else {
        return;
    };
    // A matching baseline can be written in the standard v23 representation.
    // Preserve an older, stale baseline so restoring the source still permits export.
    clip.opaque_source_content_hash = Some(if stored == v22_content_hash(clip) {
        content_hash(clip)
    } else {
        format!("{V22_HASH_PREFIX}{stored}")
    });
}

fn v22_content_hash(clip: &MotionClip) -> String {
    content_hash_with(clip, |track| {
        zipzi_animation::CompiledCurve::from_track(track, clip.fps)
            .segments
            .iter()
            .map(wire_segment)
            .collect()
    })
}

fn matches_content_hash(clip: &MotionClip) -> bool {
    let Some(stored) = clip.opaque_source_content_hash.as_deref() else {
        return false;
    };
    let Some(legacy) = stored.strip_prefix(V22_HASH_PREFIX) else {
        return stored == content_hash(clip);
    };
    // v22 hashed the motion3 projection. A v23-only setting is always an edit,
    // even when it happens to produce the same numeric curve.
    if clip
        .tracks
        .iter()
        .flat_map(|track| track.segments.iter())
        .any(|segment| {
            segment.frame_step != Default::default()
                || matches!(
                    segment.curve,
                    MotionCurve::AutoSmooth {}
                        | MotionCurve::Bezier {
                            preset: Some(_),
                            ..
                        }
                )
        })
    {
        return false;
    }
    legacy == v22_content_hash(clip)
}

fn content_hash_with<'a, S: serde::Serialize>(
    clip: &'a MotionClip,
    segments: impl Fn(&'a MotionTrack) -> Vec<S>,
) -> String {
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
                segments(track),
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
    document: &DocumentState,
    id: &str,
    name: &str,
    text: &str,
) -> Result<MotionImport, MotionProjectError> {
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
                    .parameter(id)
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
                document.part(id).expect("ordered part").runtime_id.as_str(),
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
        track_order: Vec::new(),
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
    let mut candidate = document.builder();
    let result = if candidate.motion(id).is_some() {
        candidate.replace_motion(clip)
    } else {
        candidate.create_authored_motion(clip)
    };
    if let Err(status) = result {
        return Err(error(status.code, "$", status.message));
    }
    Ok(MotionImport {
        candidate: candidate
            .freeze()
            .map_err(|s| error(s.code, "$", s.message))?
            .state,
        diagnostics,
    })
}

pub fn export_motion3(document: &DocumentState, id: &str) -> Result<String, MotionProjectError> {
    Ok(export_motion3_with_summary(document, id, &MotionExportLimits::default())?.json)
}

/// Estimate expansion without compiling or creating baked output points.
pub fn preflight_motion3(
    document: &DocumentState,
    id: &str,
    limits: &MotionExportLimits,
) -> Result<MotionExportSummary, MotionProjectError> {
    let clip = document
        .motion(id)
        .ok_or_else(|| error("MISSING_MOTION", "$", id))?;
    Ok(export_estimate(document, clip, limits)?.summary)
}

/// Aggregate admission for packages, before any curve or model output is built.
pub fn preflight_motion_exports(
    document: &DocumentState,
    limits: &MotionExportLimits,
) -> Result<MotionExportSummary, MotionProjectError> {
    let mut total = MotionExportSummary::default();
    for id in document.motion_order() {
        let remaining = MotionExportLimits {
            max_output_segments: limits
                .max_output_segments
                .saturating_sub(total.estimated_output_segments),
            max_workspace_bytes: limits
                .max_workspace_bytes
                .saturating_sub(total.estimated_workspace_bytes),
        };
        let current = preflight_motion3(document, id, &remaining)?;
        macro_rules! add { ($($field:ident),*) => { $(total.$field = total.$field.checked_add(current.$field).ok_or_else(|| error("MOTION_EXPORT_LIMIT", "$.Curves", "Package motion estimate overflow"))?;)* }; }
        add!(
            source_segments,
            estimated_output_segments,
            auto_smooth_segments,
            frame_step_intervals,
            estimated_workspace_bytes
        );
    }
    Ok(total)
}

fn export_estimate(
    document: &DocumentState,
    clip: &MotionClip,
    limits: &MotionExportLimits,
) -> Result<bake::Estimate, MotionProjectError> {
    let mut estimate = bake::estimate(clip, limits)?;
    for track in &clip.tracks {
        let length = match &track.target {
            MotionTrackTarget::Parameter { parameter_id } => document
                .parameter(parameter_id)
                .map_or(0, |p| p.runtime_id.len()),
            MotionTrackTarget::PartOpacity { part_id } => {
                document.part(part_id).map_or(0, |p| p.runtime_id.len())
            }
            MotionTrackTarget::Model { runtime_id }
            | MotionTrackTarget::Unresolved { runtime_id, .. } => runtime_id.len(),
            MotionTrackTarget::Unspecified => 0,
        };
        estimate.summary.estimated_workspace_bytes = length
            .checked_mul(16)
            .and_then(|n| n.checked_add(estimate.summary.estimated_workspace_bytes))
            .filter(|&n| n <= limits.max_workspace_bytes)
            .ok_or_else(|| {
                error(
                    "MOTION_EXPORT_LIMIT",
                    "$.Curves",
                    "Motion target names exceed the export workspace",
                )
            })?;
    }
    Ok(estimate)
}

pub fn export_motion3_with_summary(
    document: &DocumentState,
    id: &str,
    limits: &MotionExportLimits,
) -> Result<MotionExport, MotionProjectError> {
    let clip = document
        .motion(id)
        .ok_or_else(|| error("MISSING_MOTION", "$", id))?;
    export_clip_motion3(document, clip, limits)
}

pub(crate) fn opaque_export_problems(
    document: &DocumentState,
    clip: &MotionClip,
) -> Vec<crate::OpaqueExportProblem> {
    use crate::OpaqueExportProblem as P;
    if !(clip.has_extensions()) {
        return vec![];
    }
    let mut problems = vec![];
    if clip.opaque_source_ids.as_ref() != Some(&namespace(document, Some(clip))) {
        problems.push(P::NamespaceChanged);
    }
    if !matches_content_hash(clip) {
        problems.push(P::ContentChanged);
    }
    problems
}

fn export_clip_motion3(
    document: &DocumentState,
    clip: &MotionClip,
    limits: &MotionExportLimits,
) -> Result<MotionExport, MotionProjectError> {
    if let Some(problem) = opaque_export_problems(document, clip).first() {
        return Err(error(
            problem.code(),
            "$",
            "Imported opaque fields no longer match their provenance",
        ));
    }
    let mut estimate = export_estimate(document, clip, limits)?;
    let curves = clip
        .tracks
        .iter()
        .enumerate()
        .map(|(index, track)| {
            let path = format!("$.Curves[{index}].Id");
            let (target, runtime_id) = match &track.target {
                MotionTrackTarget::Unspecified => {
                    return Err(MotionProjectError {
                        code: "NEEDS_CONFIGURATION".into(),
                        path: "Curves".into(),
                        message: format!("Track {} has no target", track.id),
                    })
                }
                MotionTrackTarget::Model { runtime_id } => {
                    (MotionTarget::Model, runtime_id.clone())
                }
                MotionTrackTarget::Parameter { parameter_id } => (
                    MotionTarget::Parameter,
                    document
                        .parameter(parameter_id)
                        .ok_or_else(|| error("MISSING_PARAMETER", &path, parameter_id))?
                        .runtime_id
                        .clone(),
                ),
                MotionTrackTarget::PartOpacity { part_id } => (
                    MotionTarget::PartOpacity,
                    document
                        .part(part_id)
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
            let (initial, segments) =
                bake::track(track, clip, estimate.tracks[index], &mut estimate.summary)?;
            Ok(Motion3Curve {
                target,
                id: runtime_id,
                initial: wire_point(initial),
                segments: segments.iter().map(wire_segment).collect(),
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
    let json = encode_motion3(&wire).map_err(|e| error(e.code, e.path, e.message))?;
    Ok(MotionExport {
        json,
        summary: estimate.summary,
    })
}

#[cfg(test)]
mod source_range_tests {
    use super::*;
    fn runtime_curve(
        track: &MotionTrack,
        duration: f32,
        restricted: bool,
    ) -> (WirePoint, Vec<WireSegment>) {
        let clip: MotionClip = serde_json::from_value(serde_json::json!({
            "id":"clip", "name":"test", "duration":duration, "fps":30., "looping":false,
            "restricted_beziers":restricted, "fade_in":null, "fade_out":null, "tracks":[track],
            "events":[], "track_order":[], "extensions":{},
            "meta_extensions":{"ZipziSourceKeyRange":{"start":-1.,"end":2.}}
        }))
        .unwrap();
        let estimate = bake::estimate(&clip, &MotionExportLimits::default()).unwrap();
        let (initial, segments) = bake::track(
            track,
            &clip,
            estimate.tracks[0],
            &mut MotionExportSummary::default(),
        )
        .unwrap();
        (
            wire_point(initial),
            segments.iter().map(wire_segment).collect(),
        )
    }

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
        let source = track(MotionSegment::new(
            MotionKey {
                id: "last".into(),
                time: 2.,
                value: 1.,
            },
            MotionCurve::Bezier {
                control1: MotionPoint {
                    time: 0.,
                    value: 0.,
                },
                control2: MotionPoint {
                    time: 1.,
                    value: 1. / 3.,
                },
                preset: None,
            },
        ));
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
            &track(MotionSegment::new(end.clone(), MotionCurve::Stepped {})),
            1.,
            true,
        );
        assert_eq!(initial.value, 0.);
        assert_eq!(segments[0].end().value, 0.);
        let (initial, segments) = runtime_curve(
            &track(MotionSegment::new(end, MotionCurve::InverseStepped {})),
            1.,
            true,
        );
        assert_eq!(initial.value, 9.);
        assert_eq!(segments[0].end().value, 9.);
    }
}

/// Uses the same authored-slot projection as motion preview. Opaque source
/// protection still applies after omission; accepting a loss never bypasses it.
pub fn export_available_motion3(
    document: &DocumentState,
    id: &str,
) -> Result<String, MotionProjectError> {
    let projection = document
        .motion_projection(id)
        .map_err(|s| error(&s.code, "$", s.message))?;
    if projection.disabled.is_empty() {
        return export_motion3(document, id);
    }
    let mut clip = document.motion(id).unwrap().clone();
    clip.tracks
        .retain(|t| !projection.disabled.iter().any(|d| d.track_id == t.id));
    clip.track_order
        .retain(|id| clip.tracks.iter().any(|t| &t.id == id));
    Ok(export_clip_motion3(document, &clip, &MotionExportLimits::default())?.json)
}
