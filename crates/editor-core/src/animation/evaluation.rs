//! Background-owned SDK evaluation and completed-pose reuse.
use super::*;

#[derive(Clone)]
pub struct AnimationEvaluated {
    pub identity: AnimationIdentity,
    pub scene: SceneSnapshot,
    pub snapshot: Arc<MotionSnapshot>,
}

/// Background-owned SDK adapter. Immutable descriptions make latest-request
/// coalescing safe: each recipe contains the motion and full expression schedule.
#[derive(Default)]
pub struct AnimationEvaluator {
    preview: Option<MotionPreview>,
    identity: Option<AnimationIdentity>,
    motion: Option<MotionSelection>,
    source: Option<Arc<AnimationSource>>,
    editing: Option<kasane_sdk::MotionEditingPreview>,
    completed: Option<(AnimationRequest, AnimationEvaluated)>,
    physics: crate::PhysicsPreviewSettings,
}
impl AnimationEvaluator {
    /// Detached SDK cache accounting for profiling; preview state is unchanged.
    pub fn seek_cache_stats(&self) -> Option<kasane_sdk::SeekCacheStats> {
        self.preview.as_ref().map(MotionPreview::seek_cache_stats)
    }
    pub fn evaluate(
        &mut self,
        request: &AnimationRequest,
        mut progress: impl FnMut(u32, u32) -> bool,
    ) -> Result<AnimationEvaluated> {
        #[cfg(feature = "bench-instrumentation")]
        let _benchmark_span = kasane_core::benchmark::span("editor.animation_evaluate");
        if let Some((previous, frame)) = &self.completed {
            if previous.same_pose(request) {
                if !progress(0, 0) {
                    anyhow::bail!(kasane_sdk::AnimationError::SeekCancelled);
                }
                let mut frame = frame.clone();
                frame.identity.request = request.identity.request;
                frame.identity.acceptance_floor = request.identity.acceptance_floor;
                return Ok(frame);
            }
        }
        self.completed = None;
        let frame = self.evaluate_pose(request, progress)?;
        self.completed = Some((request.clone(), frame.clone()));
        Ok(frame)
    }
    fn evaluate_pose(
        &mut self,
        request: &AnimationRequest,
        progress: impl FnMut(u32, u32) -> bool,
    ) -> Result<AnimationEvaluated> {
        let id = request.identity;
        if id.preview_mode == AnimationPreviewMode::Editing && !request.physics.manual {
            if self.editing.is_none()
                || self.identity.is_none_or(|previous| {
                    previous.document != id.document
                        || previous.model_generation != id.model_generation
                        || previous.preview_mode != id.preview_mode
                        || !self
                            .source
                            .as_ref()
                            .is_some_and(|source| source.same_document(&request.source))
                })
            {
                self.editing = Some({
                    #[cfg(feature = "bench-instrumentation")]
                    let _benchmark_preview =
                        kasane_core::benchmark::span("editor.new_motion_editing_preview");
                    kasane_sdk::MotionEditingPreview::new(request.source.document())
                });
            }
            let mut progress = progress;
            if !progress(0, 1) {
                anyhow::bail!(kasane_sdk::AnimationError::SeekCancelled);
            }
            let preview = self.editing.as_mut().unwrap();
            let clip = request
                .motion
                .as_ref()
                .map(|motion| {
                    request
                        .source
                        .candidate
                        .as_ref()
                        .filter(|candidate| candidate.clip().id == motion.clip_id)
                        .map(|candidate| candidate.clip())
                        .or_else(|| request.source.document().get_motion(&motion.clip_id))
                        .ok_or_else(|| anyhow::anyhow!("Missing motion {}", motion.clip_id))
                })
                .transpose()?;
            preview.sample_clip(clip, id.time)?;
            let frame = Arc::new(preview.evaluate_drawables()?);
            if !progress(1, 1) {
                anyhow::bail!(kasane_sdk::AnimationError::SeekCancelled);
            }
            let snapshot = Arc::new(preview.snapshot().clone());
            self.identity = Some(id);
            self.source = Some(request.source.clone());
            self.preview = None;
            return Ok(AnimationEvaluated {
                identity: id,
                scene: SceneSnapshot {
                    runtime_frame: frame.clone(),
                    viewport_frame: frame,
                    textures: request.source.textures.clone(),
                    model_opacity: snapshot.model_opacity,
                },
                snapshot,
            });
        }
        // Runtime/manual evaluation updates the shared source identity. Any
        // editing cache retained here would no longer have its own provenance.
        self.editing = None;
        let reusable = self.identity.is_some_and(|previous| {
            self.preview.is_some()
                && previous.document == id.document
                && previous.model_generation == id.model_generation
                && self.motion == request.motion
                && self.physics.manual == request.physics.manual
                && self.physics.bypass == request.physics.bypass
                && self.physics.baseline == request.physics.baseline
                && self.physics.stabilized == request.physics.stabilized
                && request.physics.inputs.starts_with(&self.physics.inputs)
                && request.physics.inputs[self.physics.inputs.len()..]
                    .iter()
                    .all(|input| {
                        self.preview
                            .as_ref()
                            .is_some_and(|preview| input.time >= preview.snapshot().time)
                    })
                && previous.candidate == id.candidate
                && previous.preview_mode == id.preview_mode
                && self
                    .source
                    .as_ref()
                    .is_some_and(|source| source.same_document(&request.source))
        });
        let expressions = || {
            request.expressions.iter().map(|trigger| {
                (
                    trigger.expression_id.as_str(),
                    trigger.time,
                    trigger.transition,
                )
            })
        };
        if !reusable {
            let snapshot = request.source.materialized_snapshot()?;
            let document = request
                .source
                .properties
                .as_deref()
                .unwrap_or_else(|| snapshot.document());
            let mut preview = MotionPreview::new(document);
            preview.set_physics_enabled(!request.physics.bypass);
            for (id, value) in &request.physics.baseline {
                preview.set_base_parameter(id, *value)?;
            }
            preview.set_physics_stabilized_start(request.physics.stabilized);
            let version = request.source.snapshot.version();
            preview.bind_session_identity(version.session_id, version.generation);
            if let Some(motion) = &request.motion.as_ref().filter(|_| !request.physics.manual) {
                if let Some(id) = &motion.entry {
                    ensure!(
                        document
                            .get_motion_registration(id)
                            .is_some_and(|(_, e)| e.clip_id == motion.clip_id),
                        "Motion registration changed"
                    );
                    preview.schedule_motion_registration(id, 0.)?;
                } else {
                    preview.schedule_motion(&motion.clip_id, 0.)?;
                }
            }
            if !request.physics.manual {
                preview.replace_expression_schedule_with_transitions(expressions())?;
            }
            for input in &request.physics.inputs {
                preview.schedule_parameter_input(&input.parameter, input.time, input.value)?;
            }
            self.preview = Some(preview);
            self.motion.clone_from(&request.motion);
        } else if !request.physics.manual
            && self
                .identity
                .is_some_and(|previous| previous.preview_generation != id.preview_generation)
        {
            self.preview
                .as_mut()
                .unwrap()
                .replace_expression_schedule_with_transitions(expressions())?;
        }
        if reusable && request.physics.manual {
            for input in &request.physics.inputs[self.physics.inputs.len()..] {
                self.preview.as_mut().unwrap().schedule_parameter_input(
                    &input.parameter,
                    input.time,
                    input.value,
                )?;
            }
        }
        self.physics = request.physics.clone();
        self.identity = Some(id);
        self.source = Some(request.source.clone());
        let preview = self.preview.as_mut().unwrap();
        preview.advance_to_with_progress(id.time, progress)?;
        let mut snapshot = preview.snapshot().clone();
        snapshot.physics = preview.physics_snapshot();
        let snapshot = Arc::new(snapshot);
        let frame = Arc::new(preview.evaluate_drawables()?);
        Ok(AnimationEvaluated {
            identity: AnimationIdentity {
                time: snapshot.time,
                ..id
            },
            scene: SceneSnapshot {
                runtime_frame: frame.clone(),
                viewport_frame: frame,
                textures: request.source.textures.clone(),
                model_opacity: snapshot.model_opacity,
            },
            snapshot,
        })
    }
}
