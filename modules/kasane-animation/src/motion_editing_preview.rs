//! Stateless raw-clip inspection on a default model baseline. Pose is resolved
//! immediately for the sampled controls; motion fades, expressions and physics
//! are absent. This has no activation queue, replay clock or fired-event state.
use crate::pose::{real_parameter_for_part, PoseRuntime};
use crate::{
    sample_motion_curve, AnimationError, CompiledCurve, MotionPlaybackPosition, MotionSnapshot,
};
use kasane_core::{
    document::MotionTrackTarget, Document, DrawableFrame, FrameEvaluator, PreviewValues,
};
use std::{collections::BTreeMap, sync::Mutex};

pub struct MotionEditingPreview {
    document: Document,
    curves: BTreeMap<String, CompiledCurve>,
    snapshot: MotionSnapshot,
    geometry: Mutex<(PreviewValues, FrameEvaluator)>,
    base_parameters: BTreeMap<String, f32>,
    base_controls: BTreeMap<String, f32>,
    base_opacities: BTreeMap<String, f32>,
    pose: PoseRuntime,
}

impl MotionEditingPreview {
    pub fn new(document: &Document) -> Self {
        let curves = document
            .motion_order()
            .iter()
            .flat_map(|id| &document.get_motion(id).unwrap().tracks)
            .map(|track| (track.id.clone(), CompiledCurve::from(track)))
            .collect();
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
            document: document.clone(),
            curves,
            snapshot: MotionSnapshot {
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
        let document = &self.document;
        let clip = clip_id
            .map(|id| {
                document
                    .get_motion(id)
                    .ok_or_else(|| AnimationError::MissingMotion(id.into()))
            })
            .transpose()?;
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
                    sample_motion_curve(&self.curves[&track.id], time, clip.restricted_beziers);
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
