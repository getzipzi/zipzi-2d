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
    editing: Option<kasane_sdk::MotionEditingPreview>,
    completed: Option<(AnimationRequest, AnimationEvaluated)>,
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
        if let Some((previous, frame)) = &self.completed {
            if previous.same_pose(request) {
                if !progress(0, 0) {
                    anyhow::bail!(kasane_sdk::AnimationError::SeekCancelled);
                }
                let mut frame = frame.clone();
                frame.identity.request = request.identity.request;
                frame.identity.acceptance_floor = request.identity.acceptance_floor;
                frame.identity.viewport_revision = request.identity.viewport_revision;
                frame.identity.canvas_metrics = request.identity.canvas_metrics;
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
        if id.preview_mode == AnimationPreviewMode::Editing {
            if self.identity.is_none_or(|previous| {
                previous.document != id.document
                    || previous.model_generation != id.model_generation
                    || previous.preview_mode != id.preview_mode
            }) {
                self.editing = Some(kasane_sdk::MotionEditingPreview::new(
                    request.source.snapshot.document(),
                ));
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
                        .or_else(|| {
                            request
                                .source
                                .snapshot
                                .document()
                                .get_motion(&motion.clip_id)
                        })
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
        let reusable = self.identity.is_some_and(|previous| {
            previous.document == id.document
                && previous.model_generation == id.model_generation
                && self.motion == request.motion
                && previous.candidate == id.candidate
                && previous.preview_mode == id.preview_mode
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
            let mut preview = MotionPreview::new(snapshot.document());
            let version = request.source.snapshot.version();
            preview.bind_session_identity(version.session_id, version.generation);
            if let Some(motion) = &request.motion {
                if let Some((group, index)) = &motion.entry {
                    let registered = request
                        .source
                        .snapshot
                        .document()
                        .motion_groups()
                        .iter()
                        .find(|item| &item.name == group)
                        .and_then(|item| item.entries.get(*index));
                    ensure!(
                        registered.is_some_and(|entry| entry.clip_id == motion.clip_id),
                        "Motion registration changed"
                    );
                    preview.schedule_motion_entry(group, *index, 0.)?;
                } else {
                    preview.schedule_motion(&motion.clip_id, 0.)?;
                }
            }
            preview.replace_expression_schedule_with_transitions(expressions())?;
            self.preview = Some(preview);
            self.motion.clone_from(&request.motion);
        } else if self
            .identity
            .is_some_and(|previous| previous.preview_generation != id.preview_generation)
        {
            self.preview
                .as_mut()
                .unwrap()
                .replace_expression_schedule_with_transitions(expressions())?;
        }
        self.identity = Some(id);
        let preview = self.preview.as_mut().unwrap();
        preview.advance_to_with_progress(id.time, progress)?;
        let snapshot = Arc::new(preview.snapshot().clone());
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
