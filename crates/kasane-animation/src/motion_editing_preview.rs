//! Stateless raw-clip inspection on a default model baseline. Pose is resolved
//! immediately for the sampled controls; motion fades, expressions and physics
//! are absent. This has no activation queue, replay clock or fired-event state.
use crate::pose::{real_parameter_for_part, PoseRuntime};
use crate::{
    sample_motion_curve, AnimationError, CompiledCurve, MotionPlaybackPosition, MotionSnapshot,
};
use kasane_core::{
    document::{MotionClip, MotionTrack, MotionTrackTarget},
    Document, DrawableFrame, FrameEvaluator, PreviewValues,
};
use std::{
    collections::{BTreeMap, HashSet},
    sync::{Arc, Mutex},
};

pub struct MotionEditingPreview {
    document: Arc<Document>,
    curves: BTreeMap<String, (MotionTrack, CompiledCurve)>,
    #[cfg(test)]
    curve_compilations: usize,
    snapshot: MotionSnapshot,
    geometry: Mutex<(PreviewValues, FrameEvaluator)>,
    base_parameters: BTreeMap<String, f32>,
    base_controls: BTreeMap<String, f32>,
    base_opacities: BTreeMap<String, f32>,
    pose: PoseRuntime,
}

impl MotionEditingPreview {
    pub fn new(document: &Document) -> Self {
        let mut parameters: BTreeMap<_, _> = document
            .parameter_order()
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    document.get_parameter(id).unwrap().default_value,
                )
            })
            .collect();
        let mut controls = BTreeMap::new();
        let pose = PoseRuntime::new(document, &mut parameters, &mut controls);
        let opacities = pose.opacities.clone();
        Self {
            base_parameters: parameters.clone(),
            base_controls: controls.clone(),
            base_opacities: pose.opacities.clone(),
            pose,
            document: Arc::new(document.clone()),
            curves: BTreeMap::new(),
            #[cfg(test)]
            curve_compilations: 0,
            snapshot: MotionSnapshot {
                physics: Vec::new(),
                time: 0.,
                parameters,
                part_opacity_channels: controls,
                part_opacities: opacities,
                model_opacity: 1.,
                active_motions: vec![],
                active_expressions: vec![],
                fired_events: vec![],
                coverage: vec![],
                motion_positions: vec![],
            },
            geometry: Mutex::new((PreviewValues::new(), FrameEvaluator::default())),
        }
    }

    pub fn snapshot(&self) -> &MotionSnapshot {
        &self.snapshot
    }

    pub fn sample(
        &mut self,
        clip_id: Option<&str>,
        time: f32,
    ) -> Result<&MotionSnapshot, AnimationError> {
        if !time.is_finite() || time < 0. {
            return Err(AnimationError::InvalidTime);
        }
        let document = self.document.clone();
        let clip = clip_id
            .map(|id| {
                document
                    .get_motion(id)
                    .ok_or_else(|| AnimationError::MissingMotion(id.into()))
            })
            .transpose()?;
        self.sample_clip(clip, time)
    }

    /// Sample a validated clip overlay against this preview's immutable model.
    /// The model and geometry workspace survive candidate revisions; only changed
    /// tracks are compiled. The caller must validate the clip against this model.
    pub fn sample_clip(
        &mut self,
        clip: Option<&MotionClip>,
        time: f32,
    ) -> Result<&MotionSnapshot, AnimationError> {
        if !time.is_finite() || time < 0. {
            return Err(AnimationError::InvalidTime);
        }
        let document = &self.document;
        let mut active = HashSet::new();
        if let Some(clip) = clip {
            for track in &clip.tracks {
                active.insert(track.id.as_str());
                let changed = self.curves.get(&track.id).is_none_or(|cached| {
                    cached.0.initial != track.initial
                        || !Arc::ptr_eq(&cached.0.segments, &track.segments)
                });
                if changed {
                    self.curves.insert(
                        track.id.clone(),
                        (track.clone(), CompiledCurve::from(track)),
                    );
                    #[cfg(test)]
                    {
                        self.curve_compilations += 1;
                    }
                }
            }
        }
        self.curves.retain(|id, _| active.contains(id.as_str()));
        let snapshot = &mut self.snapshot;
        crate::parameter_values::copy_values(&mut snapshot.parameters, &self.base_parameters);
        crate::parameter_values::copy_values(
            &mut snapshot.part_opacity_channels,
            &self.base_controls,
        );
        crate::parameter_values::copy_values(&mut self.pose.opacities, &self.base_opacities);
        snapshot.time = time;
        snapshot.model_opacity = 1.;
        snapshot.active_motions.clear();
        snapshot.active_expressions.clear();
        snapshot.fired_events.clear();
        snapshot.coverage.clear();
        snapshot.motion_positions.clear();
        if let Some(clip) = clip {
            let id = clip.id.as_str();
            for track in &clip.tracks {
                let value =
                    sample_motion_curve(&self.curves[&track.id].1, time, clip.restricted_beziers);
                match &track.target {
                    MotionTrackTarget::Parameter { parameter_id } => {
                        let parameter = document.get_parameter(parameter_id).unwrap();
                        crate::parameter_values::set_value(
                            &mut snapshot.parameters,
                            parameter_id,
                            value.clamp(parameter.minimum, parameter.maximum),
                        );
                    }
                    MotionTrackTarget::PartOpacity { part_id } => {
                        if let Some(parameter_id) = real_parameter_for_part(document, part_id) {
                            let parameter = document.get_parameter(&parameter_id).unwrap();
                            crate::parameter_values::set_value(
                                &mut snapshot.parameters,
                                &parameter_id,
                                value.clamp(parameter.minimum, parameter.maximum),
                            );
                        } else {
                            crate::parameter_values::set_value(
                                &mut snapshot.part_opacity_channels,
                                part_id,
                                value,
                            );
                        }
                        if document.pose().is_none() {
                            snapshot.coverage.push(format!(
                                "{}: PartOpacity {part_id} has no Pose asset",
                                clip.id
                            ));
                        }
                    }
                    MotionTrackTarget::Model { runtime_id } if runtime_id == "Opacity" => {
                        snapshot.model_opacity = value
                    }
                    MotionTrackTarget::Model { runtime_id } => snapshot.coverage.push(format!(
                        "{}: Model {runtime_id} mapping requires model3 Groups",
                        clip.id
                    )),
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
            snapshot.active_motions.push(id.into());
            snapshot.motion_positions.push(MotionPlaybackPosition {
                motion_id: id.into(),
                local_time: time,
                period: clip.duration,
                loop_index: 0,
                cycle_origin: 0.,
            });
        }
        // Editing resolves the currently sampled pose rather than replaying its fade.
        self.pose.update(
            document,
            &snapshot.parameters,
            &snapshot.part_opacity_channels,
            f32::MAX,
            &mut snapshot.coverage,
        );
        crate::parameter_values::copy_values(&mut snapshot.part_opacities, &self.pose.opacities);
        Ok(&self.snapshot)
    }

    pub fn evaluate_drawables(&self) -> Result<DrawableFrame, AnimationError> {
        let mut workspace = self
            .geometry
            .lock()
            .map_err(|_| AnimationError::Evaluation("Geometry lock poisoned".into()))?;
        let (values, evaluator) = &mut *workspace;
        for (id, value) in &self.snapshot.parameters {
            if let Some(slot) = values.get_mut(id) {
                *slot = *value;
            } else {
                values.insert(id.clone(), *value);
            }
        }
        let mut frame = DrawableFrame::default();
        let status = evaluator.evaluate(&self.document, values, &mut frame);
        if !status.is_ok() {
            return Err(AnimationError::Evaluation(status.message));
        }
        for drawable in &mut frame.drawables {
            let mut id = drawable.part_id.as_str();
            while !id.is_empty() {
                drawable.opacity *= self.snapshot.part_opacities.get(id).copied().unwrap_or(1.);
                id = self
                    .document
                    .get_part(id)
                    .map_or("", |part| part.parent_id.as_str());
            }
        }
        Ok(frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kasane_core::{Canvas, Parameter, Part, Vec2};

    #[test]
    fn candidate_sampling_recompiles_only_changed_tracks_and_restores_the_base_clip() {
        let id = |n| format!("00000000-0000-4000-8000-{n:012x}");
        let mut document = Document::new();
        assert!(document
            .initialize(&id(1), Canvas::new(100., 100., Vec2::default(), 1.))
            .is_ok());
        for (n, runtime) in [(2, "ParamX"), (3, "ParamY")] {
            assert!(document
                .create_parameter(Parameter {
                    id: id(n),
                    runtime_id: runtime.into(),
                    minimum: 0.,
                    maximum: 1.,
                    ..Default::default()
                })
                .status
                .is_ok());
        }
        for (n, runtime) in [(4, "Part0"), (5, "Part1")] {
            assert!(document
                .create_part(Part {
                    id: id(n),
                    runtime_id: runtime.into(),
                    ..Default::default()
                })
                .status
                .is_ok());
        }
        let document = kasane_project::import_motion3(
            &document,
            &id(6),
            "Motion",
            include_str!("../../../fixtures/animation_cpu/minimal.motion3.json"),
        )
        .unwrap()
        .candidate;
        let original = document.get_motion(&id(6)).unwrap();
        let mut preview = MotionEditingPreview::new(&document);
        let model = preview.document.clone();
        let baseline = preview.sample(Some(&id(6)), 0.5).unwrap().clone();
        preview.evaluate_drawables().unwrap();
        let tracks = original.tracks.len();
        assert_eq!(preview.curve_compilations, tracks);
        preview.sample(Some(&id(6)), 0.6).unwrap();
        assert_eq!(preview.curve_compilations, tracks);
        let mut candidate = original.clone();
        candidate.tracks[0].initial.value += 0.1;
        let edited = preview.sample_clip(Some(&candidate), 0.5).unwrap().clone();
        assert_eq!(preview.curve_compilations, tracks + 1);
        let mut rebuilt = MotionEditingPreview::new(&document);
        assert_eq!(&edited, rebuilt.sample_clip(Some(&candidate), 0.5).unwrap());
        assert_eq!(
            preview.evaluate_drawables().unwrap(),
            rebuilt.evaluate_drawables().unwrap()
        );
        preview.sample_clip(Some(&candidate), 0.6).unwrap();
        assert_eq!(preview.curve_compilations, tracks + 1);
        assert_eq!(preview.sample(Some(&id(6)), 0.5).unwrap(), &baseline);
        assert_eq!(preview.curve_compilations, tracks + 2);
        assert!(Arc::ptr_eq(&model, &preview.document));
        preview.sample(None, 0.5).unwrap();
        assert!(preview.curves.is_empty());
        assert_eq!(document.get_motion(&id(6)).unwrap(), original);
    }
}
