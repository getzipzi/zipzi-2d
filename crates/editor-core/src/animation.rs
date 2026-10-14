//! Detached playback descriptions and frame provenance. No authoring edits.
use crate::{
    AnimationFailure, AnimationPreviewMode, Document, DocumentId, Editor, SceneSnapshot,
    SceneTexture, TimelineCandidateIdentity, WorkspaceKind,
};
use anyhow::{Result, ensure};
use kasane_sdk::{AuthoringSnapshot, ExpressionTransition, MotionPreview, MotionSnapshot};
use std::{collections::HashSet, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MotionSelection {
    pub clip_id: String,
    /// Keep both registration position and clip identity. A reordered entry
    /// must never silently play a different clip.
    pub entry: Option<(String, usize)>,
}

#[derive(Clone, Debug)]
pub struct MotionResource {
    pub selection: MotionSelection,
    pub name: String,
    pub duration: f32,
    pub looping: bool,
    pub sound: Option<String>,
    pub fps: f32,
    /// Effective registration → clip → SDK default fades, in seconds.
    pub fade_in: f32,
    pub fade_out: f32,
    pub tracks: usize,
    pub events: usize,
}
impl MotionResource {
    pub fn playable(&self) -> bool {
        self.duration.is_finite() && self.duration > 0. && self.duration * 60. <= 1_000_000.
    }
}

#[derive(Clone, Debug)]
pub struct ExpressionResource {
    pub id: String,
    pub name: String,
    pub unresolved_targets: Vec<String>,
    pub fade_in: f32,
    pub fade_out: f32,
    pub parameters: usize,
    pub blends: Vec<kasane_core::document::ExpressionBlend>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ExpressionTrigger {
    pub expression_id: String,
    pub time: f32,
    pub transition: ExpressionTransition,
}

impl Document {
    pub fn expression_resources(&self) -> Vec<ExpressionResource> {
        use kasane_core::document::ExpressionTarget;
        let model = self.model();
        model
            .expression_order()
            .iter()
            .filter_map(|id| {
                let expression = model.get_expression(id)?;
                Some(ExpressionResource {
                    id: id.clone(),
                    name: expression.name.clone(),
                    fade_in: expression.fade_in.unwrap_or(1.),
                    fade_out: expression.fade_out.unwrap_or(1.),
                    parameters: expression.entries.len(),
                    blends: [
                        kasane_core::document::ExpressionBlend::Add,
                        kasane_core::document::ExpressionBlend::Multiply,
                        kasane_core::document::ExpressionBlend::Overwrite,
                    ]
                    .into_iter()
                    .filter(|blend| {
                        expression.entries.iter().any(|entry| {
                            entry
                                .blend
                                .unwrap_or(kasane_core::document::ExpressionBlend::Add)
                                == *blend
                        })
                    })
                    .collect(),
                    unresolved_targets: expression
                        .entries
                        .iter()
                        .filter_map(|entry| match &entry.target {
                            ExpressionTarget::Unresolved { runtime_id } => Some(runtime_id.clone()),
                            _ => None,
                        })
                        .collect(),
                })
            })
            .collect()
    }
    pub fn motion_resources(&self) -> Vec<MotionResource> {
        let model = self.model();
        let mut result = Vec::new();
        let mut registered = HashSet::new();
        for group in model.motion_groups() {
            for (index, entry) in group.entries.iter().enumerate() {
                if let Some(clip) = model.get_motion(&entry.clip_id) {
                    registered.insert(clip.id.clone());
                    result.push(MotionResource {
                        selection: MotionSelection {
                            clip_id: clip.id.clone(),
                            entry: Some((group.name.clone(), index)),
                        },
                        name: clip.name.clone(),
                        duration: clip.duration,
                        looping: clip.looping,
                        sound: entry.sound.clone(),
                        fps: clip.fps,
                        fade_in: entry.fade_in.or(clip.fade_in).unwrap_or(1.),
                        fade_out: entry.fade_out.or(clip.fade_out).unwrap_or(1.),
                        tracks: clip.tracks.len(),
                        events: clip.events.len(),
                    });
                }
            }
        }
        for id in model.motion_order() {
            if !registered.contains(id)
                && let Some(clip) = model.get_motion(id)
            {
                result.push(MotionResource {
                    selection: MotionSelection {
                        clip_id: id.clone(),
                        entry: None,
                    },
                    name: clip.name.clone(),
                    duration: clip.duration,
                    looping: clip.looping,
                    sound: None,
                    fps: clip.fps,
                    fade_in: clip.fade_in.unwrap_or(1.),
                    fade_out: clip.fade_out.unwrap_or(1.),
                    tracks: clip.tracks.len(),
                    events: clip.events.len(),
                });
            }
        }
        result
    }
}

pub struct AnimationSource {
    pub snapshot: AuthoringSnapshot,
    pub textures: Arc<[SceneTexture]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationIdentity {
    pub document: DocumentId,
    pub workspace: WorkspaceKind,
    pub workspace_revision: u64,
    pub model_generation: u64,
    pub preview_generation: u64,
    pub request: u64,
    pub acceptance_floor: u64,
    pub viewport_revision: u64,
    pub time: f32,
    pub preview_mode: AnimationPreviewMode,
    pub candidate: Option<TimelineCandidateIdentity>,
}

pub struct AnimationRequest {
    pub identity: AnimationIdentity,
    pub source: Arc<AnimationSource>,
    pub motion: Option<MotionSelection>,
    /// Complete ordered schedule, including future events after a backward seek.
    pub expressions: Vec<ExpressionTrigger>,
}

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
}
impl AnimationEvaluator {
    /// Detached SDK cache accounting for profiling; preview state is unchanged.
    pub fn seek_cache_stats(&self) -> Option<kasane_sdk::SeekCacheStats> {
        self.preview.as_ref().map(MotionPreview::seek_cache_stats)
    }
    pub fn evaluate(
        &mut self,
        request: &AnimationRequest,
        progress: impl FnMut(u32, u32) -> bool,
    ) -> Result<AnimationEvaluated> {
        let id = request.identity;
        if id.preview_mode == AnimationPreviewMode::Editing {
            if self.identity.is_none_or(|previous| {
                previous.document != id.document
                    || previous.model_generation != id.model_generation
                    || previous.candidate != id.candidate
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
            preview.sample(
                request
                    .motion
                    .as_ref()
                    .map(|motion| motion.clip_id.as_str()),
                id.time,
            )?;
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
            let mut preview = MotionPreview::new(request.source.snapshot.document());
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

pub struct AnimationSession {
    pub motion: Option<MotionSelection>,
    pub expression: Option<String>,
    pub expressions: Vec<ExpressionTrigger>,
    pub time: f32,
    pub edit_time: f32,
    pub(crate) head_located: bool,
    pub preview_mode: AnimationPreviewMode,
    pub preview_loop: bool,
    pub candidate: Option<TimelineCandidateIdentity>,
    result_position: Option<kasane_sdk::MotionPlaybackPosition>,
    pub playing: bool,
    /// A paused loop scrub denotes the first cycle, including its right endpoint.
    pub loop_position: bool,
    pub presented: Option<Arc<MotionSnapshot>>,
    pub error: Option<AnimationFailure>,
    pub model_generation: u64,
    pub preview_generation: u64,
    pub(crate) source: Option<Arc<AnimationSource>>,
    requested: Option<(u64, u64, f32)>,
    floor: u64,
    shown: u64,
    scrubbing: bool,
}
impl AnimationSession {
    /// Last SDK result coordinates, retained while inspecting raw clip values.
    pub fn result_playback_position(&self) -> Option<&kasane_sdk::MotionPlaybackPosition> {
        self.result_position.as_ref()
    }
    pub(crate) fn new(document: &Document) -> Self {
        Self {
            motion: None,
            expression: None,
            expressions: Vec::new(),
            time: 0.,
            edit_time: 0.,
            head_located: true,
            preview_mode: AnimationPreviewMode::Editing,
            preview_loop: false,
            candidate: None,
            result_position: None,
            playing: false,
            loop_position: false,
            presented: None,
            error: None,
            model_generation: document.model_generation(),
            preview_generation: 1,
            source: None,
            requested: None,
            floor: 0,
            shown: 0,
            scrubbing: false,
        }
    }
    pub(crate) fn pause(&mut self) {
        self.playing = false;
        // An explicit pause needs a fresh request even at an unchanged time.
        // Requests already in flight cannot replace this target frame.
        self.barrier();
    }
    pub(crate) fn set_candidate(&mut self, candidate: Option<TimelineCandidateIdentity>) {
        self.pause();
        self.candidate = candidate;
        self.preview_generation += 1;
        self.source = None;
        self.error = None;
    }
    fn barrier(&mut self) {
        if self.scrubbing {
            // A completed drag sample can be ahead of the final pointer target.
            self.presented = None;
        }
        self.scrubbing = false;
        self.floor = self.floor.max(
            self.requested
                .map_or(self.shown + 1, |(sequence, _, _)| sequence + 1),
        );
        self.requested = None;
    }
    fn rebuild(&mut self) {
        self.pause();
        self.preview_generation += 1;
        self.presented = None;
        self.error = None;
    }
    fn new_run(&mut self) {
        self.edit_time = 0.;
        self.head_located = true;
        self.result_position = None;
        self.loop_position = false;
        self.expressions.clear();
        if let Some(id) = &self.expression {
            self.expressions.push(ExpressionTrigger {
                expression_id: id.clone(),
                time: 0.,
                transition: ExpressionTransition::Fade,
            });
        }
        self.rebuild();
    }
    fn locate(&mut self, time: f32) {
        self.head_located = true;
        self.loop_position = false;
        self.time = time;
        self.pause();
        // Permit a backward frame in this recipe without dropping the SDK seek cache.
        self.presented = None;
        self.error = None;
    }
    pub(crate) fn reconcile(&mut self, document: &Document) {
        let generation = document.model_generation();
        if generation == self.model_generation {
            return;
        }
        self.model_generation = generation;
        self.source = None;
        self.rebuild();
        if self.motion.is_none() && self.expression.is_none() && self.expressions.is_empty() {
            return;
        }
        let catalog = document.animation_catalog();
        if let Some(selection) = &self.motion {
            if let Some(resource) = catalog.motions.iter().find(|r| &r.selection == selection) {
                self.time = if !resource.playable() {
                    0.
                } else if self.preview_mode == AnimationPreviewMode::Editing {
                    // Raw preview uses clip-local time, including imported tail keys.
                    let end = document
                        .motion_definition(&selection.clip_id)
                        .unwrap()
                        .tracks
                        .iter()
                        .map(|track| {
                            track
                                .segments
                                .last()
                                .map_or(track.initial.time, |s| s.end().time)
                        })
                        .fold(resource.duration, f32::max);
                    self.edit_time.min(end)
                } else if resource.looping && !self.loop_position {
                    // A looping preview's clock includes completed cycles and
                    // timestamps the expression schedule and physics history.
                    self.time
                } else {
                    self.time.min(resource.duration)
                };
                if self.preview_mode == AnimationPreviewMode::Editing {
                    self.edit_time = self.time;
                } else if !resource.looping {
                    self.edit_time = self.edit_time.min(resource.duration);
                }
            } else {
                self.motion = None;
                self.time = 0.;
                self.edit_time = 0.;
                self.error = Some(AnimationFailure::MotionRemoved);
            }
        }
        let valid = |id: &str| {
            catalog
                .expressions
                .iter()
                .any(|r| r.id == id && r.unresolved_targets.is_empty())
        };
        let before = self.expressions.len();
        self.expressions
            .retain(|trigger| valid(&trigger.expression_id));
        if self.expression.as_ref().is_some_and(|id| !valid(id)) {
            self.expression = None;
            self.error = Some(AnimationFailure::ExpressionUnavailable);
        } else if before != self.expressions.len() {
            self.error = Some(AnimationFailure::TriggersRemoved);
        }
    }
    fn accepts(&self, id: AnimationIdentity) -> bool {
        id.model_generation == self.model_generation
            && id.candidate == self.candidate
            && id.preview_mode == self.preview_mode
            && id.preview_generation == self.preview_generation
            && id.request >= self.floor
            && id.request > self.shown
            && self
                .requested
                .is_some_and(|(latest, _, _)| id.request <= latest)
            && ((self.preview_mode == AnimationPreviewMode::Editing && self.preview_loop)
                || self.scrubbing
                || self
                    .presented
                    .as_ref()
                    .is_none_or(|shown| id.time >= shown.time))
    }
}

impl Editor {
    pub fn set_animation_preview_mode(
        &mut self,
        document: DocumentId,
        mode: AnimationPreviewMode,
    ) -> Result<()> {
        self.ensure_animation_target(document)?;
        self.cancel_timeline_edit();
        let session = self.active_session_mut().unwrap();
        let animation = &mut session.animation;
        if animation.preview_mode == mode {
            return Ok(());
        }
        let local = animation.edit_time;
        let time = if mode == AnimationPreviewMode::Result {
            if let Some(clip) = animation
                .motion
                .as_ref()
                .and_then(|motion| session.document.motion_definition(&motion.clip_id))
            {
                kasane_sdk::canonical_motion_seek_time(
                    clip,
                    animation
                        .result_position
                        .as_ref()
                        .map_or(0, |position| position.loop_index),
                    local.min(clip.duration),
                )?
            } else {
                local
            }
        } else {
            local
        };
        animation.preview_mode = mode;
        animation.head_located = true;
        animation.time = time;
        animation.rebuild();
        animation.source = None;
        Ok(())
    }
    pub fn set_animation_preview_loop(
        &mut self,
        document: DocumentId,
        looping: bool,
    ) -> Result<()> {
        self.ensure_animation_target(document)?;
        self.cancel_timeline_edit();
        let animation = &mut self.active_session_mut().unwrap().animation;
        animation.preview_loop = looping;
        animation.pause();
        Ok(())
    }
    pub fn animation_viewport_revision(&self) -> Option<u64> {
        self.active_session().map(|s| s.viewport_revision)
    }
    pub fn invalidate_animation(&mut self) {
        if let Some(session) = self.active_session_mut() {
            session.animation.barrier();
        }
    }
    pub fn animation_playable(&self) -> bool {
        self.animation().is_some_and(|a| {
            if a.preview_mode == AnimationPreviewMode::Editing && a.motion.is_none() {
                return false;
            }
            a.playing
                || (a.motion.is_none() && a.expression.is_some())
                || a.motion.as_ref().is_some_and(|selection| {
                    self.document().is_some_and(|d| {
                        d.animation_catalog()
                            .motions
                            .iter()
                            .any(|r| &r.selection == selection && r.playable())
                    })
                })
        })
    }
    pub fn animation(&self) -> Option<&AnimationSession> {
        self.active_session().map(|session| &session.animation)
    }
    pub fn select_motion(
        &mut self,
        document: DocumentId,
        selection: MotionSelection,
    ) -> Result<()> {
        self.ensure_animation_target(document)?;
        self.cancel_timeline_edit();
        let session = self.active_session_mut().unwrap();
        ensure!(
            session
                .document()
                .animation_catalog()
                .motions
                .iter()
                .any(|r| r.selection == selection),
            "Motion registration no longer exists"
        );
        session.animation.motion = Some(selection);
        session.animation.time = 0.;
        session.animation.new_run();
        session.reconcile_animation();
        Ok(())
    }
    pub fn select_expression(&mut self, document: DocumentId, expression_id: String) -> Result<()> {
        self.ensure_animation_target(document)?;
        self.cancel_timeline_edit();
        let session = self.active_session_mut().unwrap();
        let catalog = session.document().animation_catalog();
        let resource = catalog
            .expressions
            .iter()
            .find(|r| r.id == expression_id)
            .ok_or_else(|| anyhow::anyhow!("Expression no longer exists"))?;
        ensure!(
            resource.unresolved_targets.is_empty(),
            "Unresolved expression targets: {}",
            resource.unresolved_targets.join(", ")
        );
        let animation = &mut session.animation;
        if animation.expression.as_ref() == Some(&expression_id)
            && animation
                .expressions
                .iter()
                .rev()
                .find(|trigger| trigger.time <= animation.time)
                .is_some_and(|trigger| {
                    trigger.expression_id == expression_id
                        && (animation.playing
                            || trigger.transition == ExpressionTransition::Immediate)
                })
        {
            return Ok(());
        }
        let playing = animation.playing;
        // A paused click replaces this time's choice. Earlier triggers remain
        // replayable, but the immediate activation cuts their runtime queue.
        animation.expressions.retain(|trigger| {
            trigger.time < animation.time || (playing && trigger.time == animation.time)
        });
        animation.expressions.push(ExpressionTrigger {
            expression_id: expression_id.clone(),
            time: animation.time,
            transition: if playing {
                ExpressionTransition::Fade
            } else {
                ExpressionTransition::Immediate
            },
        });
        animation.expression = Some(expression_id);
        animation.rebuild();
        animation.playing = playing;
        Ok(())
    }
    pub fn clear_expression(&mut self) -> Result<()> {
        let id = self
            .active_id()
            .ok_or_else(|| anyhow::anyhow!("No active document"))?;
        self.ensure_animation_target(id)?;
        self.cancel_timeline_edit();
        let animation = &mut self.active_session_mut().unwrap().animation;
        let playing = animation.playing;
        animation.expression = None;
        animation.expressions.clear();
        animation.rebuild();
        animation.playing = playing;
        Ok(())
    }
    pub fn reset_animation(&mut self) -> Result<()> {
        let id = self
            .active_id()
            .ok_or_else(|| anyhow::anyhow!("No active document"))?;
        self.ensure_animation_target(id)?;
        self.cancel_timeline_edit();
        let animation = &mut self.active_session_mut().unwrap().animation;
        animation.motion = None;
        animation.expression = None;
        animation.expressions.clear();
        animation.time = 0.;
        animation.new_run();
        self.active_session_mut().unwrap().reconcile_animation();
        Ok(())
    }
    pub fn seek_animation(&mut self, document: DocumentId, time: f32) -> Result<()> {
        self.locate_animation(document, time, false, false)
    }
    pub fn seek_timeline(&mut self, document: DocumentId, time: f32) -> Result<()> {
        self.locate_animation(document, time, false, true)
    }
    pub fn scrub_timeline(&mut self, document: DocumentId, time: f32) -> Result<()> {
        self.locate_animation(document, time, true, true)
    }
    /// While the pointer moves, allow completed samples from this drag to be
    /// presented in request order. Finish with `seek_animation` for an exact target.
    pub fn scrub_animation(&mut self, document: DocumentId, time: f32) -> Result<()> {
        self.locate_animation(document, time, true, false)
    }
    fn locate_animation(
        &mut self,
        document: DocumentId,
        time: f32,
        scrubbing: bool,
        timeline: bool,
    ) -> Result<()> {
        self.ensure_animation_target(document)?;
        self.cancel_timeline_edit();
        ensure!(time.is_finite() && time >= 0., "Invalid animation time");
        let session = self.active_session_mut().unwrap();
        let catalog = session.document().animation_catalog();
        let motion = session
            .animation
            .motion
            .as_ref()
            .and_then(|selection| catalog.motions.iter().find(|r| &r.selection == selection))
            .filter(|r| r.playable())
            .ok_or_else(|| anyhow::anyhow!("Select a seekable motion"))?;
        let animation = &mut session.animation;
        let max_time = session
            .timeline
            .current()
            .map_or(motion.duration, |state| state.projection.display_end as f32);
        let max_time = if timeline && animation.preview_mode == AnimationPreviewMode::Result {
            animation
                .result_position
                .as_ref()
                .map_or(max_time, |position| max_time.max(position.period))
        } else {
            max_time
        };
        let local = time.min(max_time);
        let time = if timeline && animation.preview_mode == AnimationPreviewMode::Result {
            kasane_sdk::canonical_motion_seek_time(
                session
                    .document
                    .motion_definition(&motion.selection.clip_id)
                    .unwrap(),
                animation
                    .result_position
                    .as_ref()
                    .map_or(0, |position| position.loop_index),
                local,
            )?
        } else {
            local
        };
        animation.edit_time = local;
        if scrubbing && animation.scrubbing {
            // Keep the acceptance/cancellation epoch stable for the entire drag.
            // Replacing the queued target must not starve work already in flight.
            animation.time = time;
            animation.error = None;
        } else {
            animation.locate(time);
        }
        animation.scrubbing = scrubbing;
        animation.loop_position = motion.looping;
        Ok(())
    }
    pub fn pause_animation(&mut self) {
        if let Some(session) = self.active_session_mut() {
            if session.animation.playing || session.animation.scrubbing {
                if let Some(frame) = session
                    .animation
                    .presented
                    .as_ref()
                    .filter(|_| session.animation.playing && !session.animation.scrubbing)
                {
                    session.animation.time = frame.time;
                    session.animation.edit_time = session
                        .animation
                        .motion
                        .as_ref()
                        .and_then(|motion| {
                            frame
                                .motion_positions
                                .iter()
                                .find(|position| position.motion_id == motion.clip_id)
                        })
                        .map_or(frame.time, |position| position.local_time);
                }
                session.animation.pause();
            }
        }
    }
    pub fn toggle_animation(&mut self) -> Result<()> {
        let id = self
            .active_id()
            .ok_or_else(|| anyhow::anyhow!("No active document"))?;
        self.ensure_animation_target(id)?;
        self.cancel_timeline_edit();
        if self.animation().unwrap().playing {
            self.pause_animation();
            return Ok(());
        }
        let session = self.active_session_mut().unwrap();
        let catalog = session.document().animation_catalog();
        let motion = session
            .animation
            .motion
            .as_ref()
            .and_then(|selection| catalog.motions.iter().find(|r| &r.selection == selection));
        let animation = &mut session.animation;
        if animation.playing {
            animation.pause();
            return Ok(());
        }
        ensure!(
            motion.is_some_and(MotionResource::playable)
                || (animation.preview_mode == AnimationPreviewMode::Result
                    && motion.is_none()
                    && animation.expression.is_some()),
            "Select a playable motion or expression"
        );
        if animation.scrubbing {
            animation.pause();
        }
        if let Some(motion) = motion {
            let looping = if animation.preview_mode == AnimationPreviewMode::Editing {
                animation.preview_loop
            } else {
                motion.looping
            };
            if !looping && animation.time >= motion.duration {
                animation.locate(0.);
                animation.edit_time = 0.;
            }
        }
        animation.error = None;
        animation.playing = true;
        animation.head_located = false;
        animation.loop_position = false;
        Ok(())
    }
    pub fn rewind_animation(&mut self) -> Result<()> {
        let id = self
            .active_id()
            .ok_or_else(|| anyhow::anyhow!("No active document"))?;
        self.ensure_animation_target(id)?;
        self.cancel_timeline_edit();
        let animation = &mut self.active_session_mut().unwrap().animation;
        animation.locate(0.);
        animation.edit_time = 0.;
        animation.result_position = None;
        Ok(())
    }
    pub fn advance_animation_clock(&mut self, target: f32) {
        if !target.is_finite() || target < 0. {
            return;
        }
        if let Some(session) = self.active_session_mut() {
            let catalog = session.document().animation_catalog();
            let motion =
                session.animation.motion.as_ref().and_then(|selection| {
                    catalog.motions.iter().find(|r| &r.selection == selection)
                });
            let animation = &mut session.animation;
            if !animation.playing {
                return;
            }
            if let Some(motion) = motion {
                if animation.preview_mode == AnimationPreviewMode::Editing {
                    animation.time = if animation.preview_loop && target > motion.duration {
                        target % motion.duration
                    } else {
                        target.min(motion.duration)
                    };
                    if !animation.preview_loop && target >= motion.duration {
                        animation.pause();
                    }
                    return;
                }
                let target = target.max(animation.time);
                animation.time = if motion.looping {
                    target
                } else {
                    target.min(motion.duration)
                };
                if !motion.looping && animation.time >= motion.duration {
                    animation.pause();
                }
            } else {
                animation.time = target.max(animation.time);
            }
        }
    }
    pub(crate) fn ensure_animation_target(&self, document: DocumentId) -> Result<()> {
        ensure!(
            self.workspace() == WorkspaceKind::Animation
                && self.active_id() == Some(document)
                && !self.is_busy()
                && !self.exiting(),
            "Animation context changed"
        );
        Ok(())
    }
    pub fn animation_request(&mut self) -> Option<AnimationRequest> {
        let document = self.active_id()?;
        if self.ensure_animation_target(document).is_err() || !self.canvas_metrics().visible() {
            return None;
        }
        let workspace_revision = self.workspace_revision();
        let metrics = self.canvas_metrics();
        let session = self.active_session_mut()?;
        session.fit_animation_once(metrics);
        session.reconcile_animation();
        let viewport = session.viewport_revision;
        let source = if let Some(source) = &session.animation.source {
            source.clone()
        } else {
            let source = Arc::new(session.document().animation_source());
            session.animation.source = Some(source.clone());
            source
        };
        let animation = &mut session.animation;
        if animation
            .requested
            .is_some_and(|(_, view, time)| view == viewport && time == animation.time)
        {
            return None;
        }
        // Monotonic across barriers and recipe changes, including repeated seeks to zero.
        let sequence = animation.floor.max(
            animation
                .requested
                .map_or(animation.shown + 1, |(r, _, _)| r + 1),
        );
        animation.requested = Some((sequence, viewport, animation.time));
        Some(AnimationRequest {
            identity: AnimationIdentity {
                document,
                workspace: WorkspaceKind::Animation,
                workspace_revision,
                model_generation: animation.model_generation,
                preview_generation: animation.preview_generation,
                request: sequence,
                acceptance_floor: animation.floor,
                viewport_revision: viewport,
                time: animation.time,
                preview_mode: animation.preview_mode,
                candidate: animation.candidate,
            },
            source,
            motion: animation.motion.clone(),
            expressions: animation.expressions.clone(),
        })
    }
    /// Keep the last presented image visible while a new recipe is evaluated.
    /// Incoming CPU/GPU results must still pass `accepts_animation` / `present_animation`.
    pub fn can_display_animation_image(&self, identity: AnimationIdentity) -> bool {
        identity.workspace == WorkspaceKind::Animation
            && self.workspace() == WorkspaceKind::Animation
            && self.workspace_revision() == identity.workspace_revision
            && self.active_id() == Some(identity.document)
            && self.animation().is_some_and(|a| {
                a.model_generation == identity.model_generation
                    && a.shown != 0
                    && a.shown == identity.request
            })
    }
    pub fn accepts_animation(&self, identity: AnimationIdentity) -> bool {
        self.workspace() == identity.workspace
            && self.workspace() == WorkspaceKind::Animation
            && self.workspace_revision() == identity.workspace_revision
            && self.active_id() == Some(identity.document)
            && !self.is_busy()
            && !self.exiting()
            && self
                .animation()
                .is_some_and(|animation| animation.accepts(identity))
    }
    pub fn present_animation(
        &mut self,
        identity: AnimationIdentity,
        snapshot: Arc<MotionSnapshot>,
    ) -> bool {
        if !self.accepts_animation(identity) {
            return false;
        }
        if self.animation_viewport_revision() != Some(identity.viewport_revision) {
            return false;
        }
        let animation = &mut self.active_session_mut().unwrap().animation;
        animation.shown = identity.request;
        if animation.preview_mode == AnimationPreviewMode::Result {
            animation.result_position = animation
                .motion
                .as_ref()
                .and_then(|motion| {
                    snapshot
                        .motion_positions
                        .iter()
                        .find(|position| position.motion_id == motion.clip_id)
                })
                .cloned();
        }
        if animation.playing || !animation.head_located {
            animation.edit_time = animation
                .motion
                .as_ref()
                .and_then(|motion| {
                    snapshot
                        .motion_positions
                        .iter()
                        .find(|position| position.motion_id == motion.clip_id)
                })
                .map_or(snapshot.time, |position| position.local_time);
        }
        animation.presented = Some(snapshot);
        true
    }
    pub fn fail_animation(&mut self, identity: AnimationIdentity, error: AnimationFailure) {
        if self.accepts_animation(identity) {
            let animation = &mut self.active_session_mut().unwrap().animation;
            animation.playing = false;
            animation.error = Some(error);
        }
    }
}
