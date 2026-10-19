//! Editable animation export. CAN3 references a sibling CMO3 rather than
//! embedding a second model; both codecs use the same source object GUIDs.
mod attributes;
mod bake;
mod grid;
mod inverse;
mod scene;
mod schema;

use crate::{
    xml::{Graph, Node as N},
    *,
};
use kasane_core::{
    document::{
        ExpressionAsset, ExpressionBlend, ExpressionTarget, MotionClip, MotionKey, MotionSegment,
        MotionTrack, MotionTrackTarget,
    },
    types::Canvas,
    Document,
};
use std::collections::{BTreeMap, BTreeSet};

// A reserved playlist identity namespace distinguishes model3 registrations
// from ordinary Editor playback playlists. UUIDs survive Editor save/reopen;
// the visible playlist name and ordered scene references remain native CAN3.
pub(crate) const MOTION_GROUP_PLAYLIST_PREFIX: &str = "ca5a4d47-0001-4000-8000-";

#[derive(Debug, Clone)]
pub struct Can3Options {
    /// Relative sibling filename, including `.cmo3`. Directories are rejected.
    pub model_file_name: String,
    pub animation_name: String,
    pub loss_policy: LossPolicy,
    pub obfuscation_key: i32,
    /// Allow inverse-step boundary repair on inferred grids, or resample
    /// incompatible curves at the declared rate when no grid is inferred.
    /// Conversions are Info; unsupported features still obey loss_policy.
    /// Runtime expression operations are always preserved alongside scene poses.
    pub bake_animation: bool,
    /// Infer a common anchor grid per declared-FPS group before any fallback
    /// baking. Millisecond-rounded anchors may be restored within 0.5 ms.
    pub auto_frame_rate: bool,
}
impl Default for Can3Options {
    fn default() -> Self {
        Self {
            model_file_name: "model.cmo3".into(),
            animation_name: "Kasane Animation".into(),
            loss_policy: LossPolicy::Reject,
            obfuscation_key: 0x4b617361,
            bake_animation: false,
            auto_frame_rate: false,
        }
    }
}
#[derive(Debug, Clone)]
pub struct Can3Artifact {
    pub bytes: Vec<u8>,
    pub report: Cmo3Report,
    /// Standard exp3 files and their registration manifest. Paired publication
    /// writes these beside CAN3; standalone CAN3 still embeds the operations.
    pub expression_files: BTreeMap<String, Vec<u8>>,
}
/// A shared basename ensures that animation links and published model names agree.
#[derive(Debug, Clone)]
pub struct CubismOptions {
    pub base_name: String,
    pub model: Cmo3Options,
    /// See Can3Options::bake_animation. Disabled by default for strict callers.
    pub bake_animation: bool,
    pub auto_frame_rate: bool,
}
impl Default for CubismOptions {
    fn default() -> Self {
        Self {
            base_name: "model".into(),
            model: Cmo3Options::default(),
            bake_animation: false,
            auto_frame_rate: false,
        }
    }
}
#[derive(Debug, Clone)]
pub struct CubismArtifact {
    pub model_file_name: String,
    pub animation_file_name: String,
    pub cmo3: Cmo3Artifact,
    pub can3: Can3Artifact,
    /// Combined model/animation diagnostics, without suppressing other sidecars.
    pub report: Cmo3Report,
}

pub(crate) fn validate_name(name: &str) -> Result<(), Cmo3Error> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.chars().any(|c| {
            c.is_control() || matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*')
        })
        || name.ends_with([' ', '.'])
    {
        return Err(Cmo3Error::InvalidDocument(
            "Cubism output name must be a portable sibling filename".into(),
        ));
    }
    let stem = name.split('.').next().unwrap().to_ascii_uppercase();
    if matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ["COM", "LPT"].iter().any(|prefix| {
            stem.strip_prefix(prefix)
                .is_some_and(|s| matches!(s, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9"))
        })
    {
        return Err(Cmo3Error::InvalidDocument(
            "Cubism output name is a reserved device name".into(),
        ));
    }
    Ok(())
}

/// Inspect animation losses without resources, archive creation or input mutation.
pub fn inspect_can3(doc: &Document) -> Result<Cmo3Report, Cmo3Error> {
    prepare(doc, false, false).map(|(_, _, report)| report)
}
/// Inspect the paired export; motions are carried by CAN3 rather than lost in CMO3.
pub fn inspect_cubism(doc: &Document) -> Result<Cmo3Report, Cmo3Error> {
    inspect_cubism_with_baking(doc, false, false)
}
fn inspect_cubism_with_baking(
    doc: &Document,
    bake: bool,
    auto: bool,
) -> Result<Cmo3Report, Cmo3Error> {
    let mut report = crate::export::inspect_with_animation(doc, true)?;
    report
        .diagnostics
        .extend(prepare(doc, bake, auto)?.2.diagnostics);
    Ok(report)
}
/// Encode animation only, linking the CMO3 filename supplied by the caller.
pub fn encode_can3(doc: &Document, options: &Can3Options) -> Result<Can3Artifact, Cmo3Error> {
    validate_name(&options.model_file_name)?;
    if !options.model_file_name.ends_with(".cmo3") {
        return Err(Cmo3Error::InvalidDocument(
            "CAN3 model_file_name must end with .cmo3".into(),
        ));
    }
    let (scenes, canvas, report) = prepare(doc, options.bake_animation, options.auto_frame_rate)?;
    reject(&report, options.loss_policy)?;
    let xml = Animation::new(doc, canvas, options).build(&scenes)?;
    Ok(Can3Artifact {
        bytes: crate::caff::encode(Vec::new(), &xml, options.obfuscation_key)?,
        report,
        expression_files: crate::expression_semantics::files(doc)?,
    })
}
/// Encode a linked pair entirely in memory. All loss checks precede image I/O.
pub fn encode_cubism(
    doc: &Document,
    assets: &dyn AssetResolver,
    options: &CubismOptions,
) -> Result<CubismArtifact, Cmo3Error> {
    validate_name(&options.base_name)?;
    let report = inspect_cubism_with_baking(doc, options.bake_animation, options.auto_frame_rate)?;
    reject(&report, options.model.loss_policy)?;
    let model_file_name = format!("{}.cmo3", options.base_name);
    let animation_file_name = format!("{}.can3", options.base_name);
    let can3 = encode_can3(
        doc,
        &Can3Options {
            model_file_name: model_file_name.clone(),
            animation_name: options.model.model_name.clone(),
            loss_policy: options.model.loss_policy,
            obfuscation_key: options.model.obfuscation_key,
            bake_animation: options.bake_animation,
            auto_frame_rate: options.auto_frame_rate,
        },
    )?;
    let cmo3 = crate::export::encode_with_animation(doc, assets, &options.model, true)?;
    let mut report = cmo3.report.clone();
    report.diagnostics.extend(can3.report.diagnostics.clone());
    Ok(CubismArtifact {
        model_file_name,
        animation_file_name,
        cmo3,
        can3,
        report,
    })
}
fn reject(report: &Cmo3Report, policy: LossPolicy) -> Result<(), Cmo3Error> {
    if policy == LossPolicy::Reject && report.has_loss() {
        return Err(Cmo3Error::Unsupported(report.clone()));
    }
    Ok(())
}
fn loss(report: &mut Cmo3Report, id: &str, code: &str, message: &str) {
    report.diagnostics.push(Cmo3Diagnostic {
        severity: DiagnosticSeverity::Loss,
        code: code.into(),
        object_id: id.into(),
        message: message.into(),
    });
}
fn frame(time: f32, fps: f32, id: &str, report: &mut Cmo3Report) -> Result<i32, Cmo3Error> {
    let value = f64::from(time) * f64::from(fps);
    if !value.is_finite()
        || value.round() < f64::from(i32::MIN)
        || value.round() > f64::from(i32::MAX)
    {
        return Err(Cmo3Error::InvalidDocument(format!(
            "{id}: CAN3 frame exceeds int32 range"
        )));
    }
    if (value - value.round()).abs() > 0.0001 {
        loss(report, id, "CAN3_FRAME_QUANTIZATION", "CAN3 anchors, duration and events require integer frames; times are rounded to the nearest frame");
    }
    Ok(value.round() as i32)
}
fn millis(value: Option<f32>, id: &str, report: &mut Cmo3Report) -> Result<Option<i32>, Cmo3Error> {
    value
        .map(|time| {
            let ms = f64::from(time) * 1000.;
            if ms.round() > f64::from(i32::MAX) {
                return Err(Cmo3Error::InvalidDocument(format!(
                    "{id}: fade exceeds int32 milliseconds"
                )));
            }
            if (ms - ms.round()).abs() > 0.0001 {
                loss(
                    report,
                    id,
                    "CAN3_FADE_QUANTIZATION",
                    "CAN3 fades use integer milliseconds; fade duration is rounded",
                );
            }
            Ok(ms.round() as i32)
        })
        .transpose()
}
#[derive(Clone)]
struct Key {
    frame: i32,
    value: f64,
    curve: &'static str,
    prev: (f64, f64),
    next: (f64, f64),
}
struct Track {
    target: MotionTrackTarget,
    keys: Vec<Key>,
    fade_in: Option<i32>,
    fade_out: Option<i32>,
}
struct Scene {
    name: String,
    guid: String,
    frames: i32,
    fps: f64,
    looping: bool,
    restricted: bool,
    fade_in: i32,
    fade_out: i32,
    tracks: Vec<Track>,
    events: BTreeMap<i32, String>,
}
fn keys(
    track: &MotionTrack,
    fps: f32,
    report: &mut Cmo3Report,
    inferred: bool,
    restricted: bool,
) -> Result<Vec<Key>, Cmo3Error> {
    let mut keys: Vec<_> = track
        .keys()
        .map(|key| {
            let frame = grid::frame(key.time, fps, &key.id, report, inferred)?;
            Ok(Key {
                frame,
                value: f64::from(key.value),
                curve: "LINEAR",
                prev: (f64::from(frame), f64::from(key.value)),
                next: (f64::from(frame), f64::from(key.value)),
            })
        })
        .collect::<Result<_, Cmo3Error>>()?;
    for (index, segment) in track.segments.iter().enumerate() {
        keys[index].curve = match segment {
            MotionSegment::Linear { .. } => "LINEAR",
            MotionSegment::Stepped { .. } => "STEP",
            MotionSegment::InverseStepped { .. } => "INVERSE_STEP",
            MotionSegment::Bezier {
                control1, control2, ..
            } => {
                let start_time = if index == 0 {
                    track.initial.time
                } else {
                    track.segments[index - 1].end().time
                };
                let end_time = segment.end().time;
                let start_frame = f64::from(keys[index].frame);
                let span = f64::from(keys[index + 1].frame) - start_frame;
                let handle = |time: f32, fraction: f64| {
                    if !inferred {
                        f64::from(time) * f64::from(fps)
                    } else if restricted {
                        start_frame + span * fraction
                    } else {
                        start_frame
                            + span * (f64::from(time) - f64::from(start_time))
                                / (f64::from(end_time) - f64::from(start_time))
                    }
                };
                keys[index].next = (handle(control1.time, 1. / 3.), f64::from(control1.value));
                keys[index + 1].prev = (handle(control2.time, 2. / 3.), f64::from(control2.value));
                "BEZIER"
            }
        };
    }
    // Quantization may collapse distinct anchors. Keep the last authored key,
    // explicitly reporting the loss rather than emitting ambiguous sequences.
    let mut result: Vec<Key> = Vec::new();
    for key in keys {
        if result.last().is_some_and(|prev| prev.frame == key.frame) {
            loss(
                report,
                &track.id,
                "CAN3_KEY_COLLISION",
                "Multiple keys round to one frame; the last key at that frame is retained",
            );
            result.pop();
        }
        result.push(key);
    }
    Ok(result)
}

fn expression_clip(doc: &Document, asset: &ExpressionAsset, report: &mut Cmo3Report) -> MotionClip {
    let mut clip = asset.source_scene().unwrap_or_else(|| MotionClip {
        id: asset.id.clone(),
        name: asset.name.clone(),
        duration: 1. / 30.,
        fps: 30.,
        looping: false,
        restricted_beziers: true,
        fade_in: asset.fade_in,
        fade_out: asset.fade_out,
        tracks: Vec::new(),
        events: Vec::new(),
        extensions: Default::default(),
        meta_extensions: Default::default(),
        opaque_source_ids: None,
        opaque_source_content_hash: None,
    });
    clip.name = asset.name.clone();
    clip.fade_in = asset.fade_in;
    clip.fade_out = asset.fade_out;
    let source_tracks = std::mem::take(&mut clip.tracks);
    clip.tracks.extend(
        source_tracks
            .iter()
            .filter(|track| !matches!(track.target, MotionTrackTarget::Parameter { .. }))
            .cloned(),
    );
    if asset.has_extensions() {
        loss(
            report,
            &asset.id,
            "CAN3_EXPRESSION_EXTENSIONS",
            "Opaque expression extensions cannot be represented in CAN3",
        );
    }
    let mut targets = BTreeSet::new();
    for entry in &asset.entries {
        let ExpressionTarget::Resolved { parameter_id } = &entry.target else {
            loss(
                report,
                &asset.id,
                "CAN3_EXPRESSION_TARGET",
                "Unresolved expression parameters cannot be represented in CAN3",
            );
            continue;
        };
        let blend = entry.blend.unwrap_or(ExpressionBlend::Add);
        // CAN3 stores absolute poses; Add expressions store an offset from the
        // model default. Reconstruct the pose using the current parameter.
        let default = doc.get_parameter(parameter_id).unwrap().default_value;
        let value = match blend {
            ExpressionBlend::Add => default + entry.value,
            ExpressionBlend::Multiply => default * entry.value,
            ExpressionBlend::Overwrite => entry.value,
        };
        if !value.is_finite() {
            loss(
                report,
                &asset.id,
                "CAN3_EXPRESSION_VALUE",
                "Expression offset plus parameter default exceeds the finite CAN3 value range",
            );
            continue;
        }
        // Match expression playback: clamp the target before applying its fade.
        let parameter = doc.get_parameter(parameter_id).unwrap();
        let value = value.clamp(parameter.minimum, parameter.maximum);
        if !targets.insert(parameter_id) {
            loss(report, &asset.id, "CAN3_EXPRESSION_COLLISION", "Repeated expression parameters cannot be represented by a single CAN3 initial value");
            continue;
        }
        let target = MotionTrackTarget::Parameter {
            parameter_id: parameter_id.clone(),
        };
        let mut track = source_tracks
            .iter()
            .find(|track| track.target == target)
            .cloned()
            .unwrap_or_else(|| MotionTrack {
                id: uuid::Uuid::new_v4().to_string(),
                target,
                initial: MotionKey {
                    id: uuid::Uuid::new_v4().to_string(),
                    time: 0.,
                    value,
                },
                segments: Default::default(),
                fade_in: None,
                fade_out: None,
                extensions: Default::default(),
            });
        track.initial.value = value;
        clip.tracks.push(track);
    }
    if asset
        .entries
        .iter()
        .any(|entry| entry.blend.unwrap_or(ExpressionBlend::Add) != ExpressionBlend::Add)
    {
        bake::info(report, &asset.id, "CAN3_EXPRESSION_SEMANTICS", "Original expression operations are embedded for reimport and exported as standard exp3 companions for Viewer/SDK motion blending. Animator scenes preview default-model poses; native CAN3 scene blending does not implement per-parameter Multiply");
    }
    if clip
        .tracks
        .iter()
        .all(|track| !matches!(track.target, MotionTrackTarget::Parameter { .. }))
    {
        loss(
            report,
            &asset.id,
            "CAN3_EMPTY_EXPRESSION",
            "An expression needs a parameter initial value to retain its classification on import",
        );
    }
    clip
}

fn prepare(
    doc: &Document,
    bake: bool,
    auto: bool,
) -> Result<(Vec<Scene>, Canvas, Cmo3Report), Cmo3Error> {
    if let Some(issue) = doc.validate_structure().first() {
        return Err(Cmo3Error::InvalidDocument(format!(
            "{}: {}",
            issue.object_id, issue.status.message
        )));
    }
    let mut report = Cmo3Report::default();
    let canvas = crate::canvas::for_document(doc)?;
    if [canvas.width, canvas.height]
        .iter()
        .any(|&v| f64::from(v).round() > f64::from(i32::MAX))
    {
        return Err(Cmo3Error::InvalidDocument(
            "CAN3 canvas exceeds int32 range".into(),
        ));
    }
    if canvas.width.fract() != 0. || canvas.height.fract() != 0. {
        loss(
            &mut report,
            doc.id(),
            "CAN3_CANVAS_QUANTIZATION",
            "CAN3 scene dimensions require integer pixels; canvas dimensions are rounded",
        );
    }
    let mut scenes = Vec::new();
    let mut sample_budget = 10_000_000usize;
    let expressions: Vec<_> = doc
        .expression_order()
        .iter()
        .map(|id| expression_clip(doc, doc.get_expression(id).unwrap(), &mut report))
        .collect();
    let clips: Vec<_> = doc
        .motion_order()
        .iter()
        .map(|id| doc.get_motion(id).unwrap())
        .chain(expressions.iter())
        .collect();
    let grids = if auto {
        grid::detect(&clips)
    } else {
        BTreeMap::new()
    };
    for clip in clips {
        let inferred = grids.contains_key(&clip.fps.to_bits());
        let fps = grids.get(&clip.fps.to_bits()).copied().unwrap_or(clip.fps);
        let id = &clip.id;
        let mut timing = Cmo3Report::default();
        let frames = grid::frame(clip.duration, fps, id, &mut timing, inferred)?;
        if inferred {
            bake::info(&mut report, id, "CAN3_TIME_GRID", &format!("Detected {fps} FPS anchor grid from declared {} FPS; maximum timestamp adjustment {:.6} ms. Authored keys and Bezier shapes are retained; loop transitions use the inferred scene FPS", clip.fps, grid::maximum_shift(clip,fps)*1000.));
        } else if auto {
            bake::info(&mut report, id, "CAN3_TIME_GRID_UNRESOLVED", "No collision-free common time grid up to 240 FPS was found; explicit fallback baking or loss rejection is used at the declared frame rate");
        }
        if bake {
            for diagnostic in &mut timing.diagnostics {
                diagnostic.severity = DiagnosticSeverity::Info;
            }
        }
        report.diagnostics.extend(timing.diagnostics);
        if frames == 0 {
            loss(
                &mut report,
                id,
                "CAN3_MINIMUM_DURATION",
                "CAN3 scenes require at least one frame; duration is extended to one frame",
            );
        }
        let mut scene = Scene {
            name: clip.name.clone(),
            guid: clip.id.clone(),
            frames: frames.max(1),
            fps: f64::from(fps),
            looping: clip.looping,
            restricted: clip.restricted_beziers,
            fade_in: millis(clip.fade_in, id, &mut report)?.unwrap_or(-1),
            fade_out: millis(clip.fade_out, id, &mut report)?.unwrap_or(-1),
            tracks: Vec::new(),
            events: BTreeMap::new(),
        };
        if clip.has_extensions() {
            loss(
                &mut report,
                id,
                "CAN3_EXTENSIONS",
                "Opaque motion JSON extensions are omitted",
            );
        }
        let mut targets = BTreeSet::new();
        for track in &clip.tracks {
            if !matches!(
                &track.target,
                MotionTrackTarget::Parameter { .. } | MotionTrackTarget::PartOpacity { .. }
            ) && !matches!(&track.target, MotionTrackTarget::Model { runtime_id } if runtime_id == "Opacity")
            {
                loss(&mut report, &track.id, "CAN3_UNSUPPORTED_TARGET", "Unresolved, EyeBlink, LipSync and other model effect curves are omitted; use parameter curves for editable animation");
                continue;
            }
            if !bake
                && track.segments.iter().enumerate().any(|(index, segment)| {
                    matches!(segment, MotionSegment::InverseStepped { .. })
                        && segment.end().value
                            != if index == 0 {
                                track.initial.value
                            } else {
                                track.segments[index - 1].end().value
                            }
                })
            {
                loss(&mut report, &track.id, "CAN3_INVERSE_STEP_BOUNDARY", "Animator displays the anchor value at the first frame of an inverse-step segment, while Kasane samples its endpoint; authored keys and segment types are retained");
            }
            let target_key = match &track.target {
                MotionTrackTarget::Parameter { parameter_id } => {
                    ("parameter", parameter_id.clone())
                }
                MotionTrackTarget::PartOpacity { part_id } => (
                    "part",
                    scene::part_key(&doc.get_part(part_id).unwrap().runtime_id),
                ),
                _ => ("model", "Opacity".into()),
            };
            if !targets.insert(target_key) {
                loss(&mut report, &track.id, "CAN3_TRACK_COLLISION", "Multiple tracks share an Animator attribute or normalized part ID; later tracks are omitted");
                continue;
            }
            scene.tracks.push(Track {
                target: track.target.clone(),
                keys: if inferred && bake {
                    inverse::repair(
                        keys(track, fps, &mut report, true, clip.restricted_beziers)?,
                        &track.id,
                        &mut report,
                    )
                } else if bake && bake::needs_baking(track, clip.fps) {
                    bake::keys(track, clip, &mut report, &mut sample_budget)?
                } else {
                    keys(track, fps, &mut report, inferred, clip.restricted_beziers)?
                },
                fade_in: millis(track.fade_in, &track.id, &mut report)?,
                fade_out: millis(track.fade_out, &track.id, &mut report)?,
            });
        }
        for event in &clip.events {
            let frame = grid::frame(event.time, fps, &event.id, &mut report, inferred)?;
            if scene.events.insert(frame, event.value.clone()).is_some() {
                loss(&mut report, &event.id, "CAN3_EVENT_COLLISION", "CAN3 permits one user-data event per frame; the last event at that frame is retained");
            }
        }
        scenes.push(scene);
    }
    if doc.motion_groups().iter().any(|group| {
        group.entries.iter().any(|entry| {
            entry.fade_in.is_some()
                || entry.fade_out.is_some()
                || entry.sound.is_some()
                || !entry.extensions.is_empty()
        })
    }) {
        loss(&mut report, doc.id(), "CAN3_MOTION_REGISTRATIONS", "Registration-specific fades, sound references and opaque metadata cannot be represented by CAN3 playlists");
    }
    if scenes.is_empty() {
        scenes.push(Scene {
            name: "Scene".into(),
            guid: doc.id().into(),
            frames: 1,
            fps: 30.,
            looping: false,
            restricted: true,
            fade_in: -1,
            fade_out: -1,
            tracks: Vec::new(),
            events: BTreeMap::new(),
        });
    }
    Ok((scenes, canvas, report))
}
fn s(value: impl ToString) -> N {
    N::scalar("s", value)
}
fn i(value: impl ToString) -> N {
    N::scalar("i", value)
}
fn d(value: impl ToString) -> N {
    N::scalar("d", value)
}
fn f(value: impl ToString) -> N {
    N::scalar("f", value)
}
fn b(value: bool) -> N {
    N::scalar("b", value)
}
fn list(items: impl IntoIterator<Item = N>) -> N {
    N::list("carray_list", items)
}
fn array(item_type: &str, items: impl IntoIterator<Item = N>) -> N {
    N::list("array", items).attr("type", item_type)
}
fn map(items: impl IntoIterator<Item = N>) -> N {
    let n = N::list("hash_map", items);
    if n.children.is_empty() {
        n.attr("keyType", "string")
    } else {
        n
    }
}
fn rect(width: f32, height: f32) -> N {
    N::new("GRectF")
        .field("x", f(0))
        .field("y", f(0))
        .field("width", f(width))
        .field("height", f(height))
}

struct Animation<'a> {
    doc: &'a Document,
    canvas: Canvas,
    options: &'a Can3Options,
    graph: Graph,
    root: N,
    resources: N,
    resource: N,
    group_guid: N,
    resource_data: N,
    serial: u64,
}
impl<'a> Animation<'a> {
    fn new(doc: &'a Document, canvas: Canvas, options: &'a Can3Options) -> Self {
        let mut graph = Graph::default();
        let root = graph.reserve("CAnimation");
        let resources = graph.reserve("CResourceManager");
        let resource = graph.add(
            N::new("CResourceGuid")
                .attr("uuid", "ca5a0001-0000-4000-8000-000000000000")
                .attr("note", &options.model_file_name),
        );
        let group_guid = graph.add(
            N::new("CResourceGroupGuid")
                .attr("uuid", "2d05fd78-9ac1-4eb0-bb6a-5cf21147e7b7")
                .attr("note", "Root Resource Group"),
        );
        let resource_data = graph.reserve("CResourceData");
        Self {
            doc,
            canvas,
            options,
            graph,
            root,
            resources,
            resource,
            group_guid,
            resource_data,
            serial: 1,
        }
    }
    fn guid(&mut self, tag: &str, note: &str) -> N {
        let uuid = loop {
            let candidate = format!("ca5a0001-0000-4000-8000-{:012x}", self.serial);
            self.serial += 1;
            if !self.doc.contains_id(&candidate) {
                break candidate;
            }
        };
        self.graph
            .add(N::new(tag).attr("uuid", uuid).attr("note", note))
    }
    fn build(mut self, scenes: &[Scene]) -> Result<Vec<u8>, Cmo3Error> {
        let built: Vec<_> = scenes.iter().map(|scene| self.scene(scene)).collect();
        let playlist_guid = self.guid("CPlaylistGuid", "Scenes");
        let blend_guid = self.guid("CSceneBlendingSettingsGuid", "Scene blending");
        let playlist = N::new("PlaylistData")
            .field("guid", playlist_guid)
            .field("name", s("Scenes"))
            .field(
                "list",
                list(built.iter().map(|(_, guid)| {
                    N::new("PlaylistItemData")
                        .field("super", N::new("ASceneBlendingData"))
                        .field("guid", guid.clone())
                })),
            );
        let mut playlists = vec![playlist];
        for group in self.doc.motion_groups() {
            let uuid = loop {
                if self.serial >= 1 << 48 {
                    return Err(Cmo3Error::TooLarge);
                }
                let uuid = format!("{MOTION_GROUP_PLAYLIST_PREFIX}{:012x}", self.serial);
                self.serial += 1;
                if !self.doc.contains_id(&uuid) {
                    break uuid;
                }
            };
            let guid = self.graph.add(
                N::new("CPlaylistGuid")
                    .attr("uuid", uuid)
                    .attr("note", "Model3 motion group"),
            );
            let entries = group.entries.iter().map(|entry| {
                let index = scenes
                    .iter()
                    .position(|scene| scene.guid == entry.clip_id)
                    .expect("validated registration scene exists");
                N::new("PlaylistItemData")
                    .field("super", N::new("ASceneBlendingData"))
                    .field("guid", built[index].1.clone())
            });
            playlists.push(
                N::new("PlaylistData")
                    .field("guid", guid)
                    .field("name", s(&group.name))
                    .field("list", list(entries)),
            );
        }
        self.graph.set(
            &self.root,
            N::new("CAnimation")
                .field("name", s(&self.options.animation_name))
                .field(
                    "file",
                    N::scalar(
                        "file",
                        self.options
                            .model_file_name
                            .strip_suffix(".cmo3")
                            .unwrap()
                            .to_owned()
                            + ".can3",
                    ),
                )
                .field(
                    "_scenes",
                    list(built.iter().map(|(source, _)| source.clone())),
                )
                .field("currentScene", built[0].0.clone())
                .field("resourceManager", self.resources.clone())
                .field(
                    "editorEdition",
                    N::new("EditorEdition").field("edition", i(15)),
                )
                .field(
                    "sceneBlendingSettings",
                    N::new("CSceneBlendingSettingsSource")
                        .field("guid", blend_guid)
                        .field("playlists", list(playlists))
                        .field("sceneGroups", N::empty_list()),
                )
                .field("hideAbsolutePathIfLinkError", b(false))
                .field(
                    "targetVersion",
                    N::enumeration("Animation", "FOR_UNITY_SDK"),
                ),
        );
        let entry = N::new("ACResourceEntry")
            .field("parentGuid", N::null())
            .field("_resourceManager", self.resources.clone());
        self.graph.set(
            &self.resources,
            N::new("CResourceManager")
                .field(
                    "rootGroup",
                    N::new("CResourceGroup")
                        .field("super", entry)
                        .field("guid", self.group_guid.clone())
                        .field("_childGuids", list([self.resource.clone()]))
                        .field("name", s("Resources")),
                )
                .field("_resourceRefList", list([self.resource_data.clone()]))
                .field(
                    "resourceGuidMap",
                    map([N::new("entry")
                        .field("key", self.resource.clone())
                        .field("value", self.resource_data.clone())]),
                )
                .field("_resourceGroupList", N::empty_list())
                .field("resourceGroupGuidMap", N::empty_map())
                .field("_sceneSource", built[0].0.clone()),
        );
        self.graph.set(
            &self.resource_data,
            N::new("CResourceData")
                .field(
                    "super",
                    N::new("ACResourceEntry")
                        .field("parentGuid", self.group_guid.clone())
                        .field("_resourceManager", self.resources.clone()),
                )
                .field("customName", N::null())
                .field(
                    "resourceRef",
                    N::new("CResource_Linked_Model").field(
                        "super",
                        N::new("ACResource_File")
                            .field("srcFile", N::scalar("file", &self.options.model_file_name))
                            .field("guid", self.resource.clone())
                            .field("name", s(&self.options.model_file_name)),
                    ),
                ),
        );
        self.graph.document_using(
            self.root,
            schema::class_name,
            schema::version,
            &[("SerializeFormatVersion", 2), ("CMvParameter_Group", 1)],
        )
    }
}
