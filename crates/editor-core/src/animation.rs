//! Detached playback descriptions and frame provenance. No authoring edits.
use crate::{Document, DocumentId, Editor, SceneSnapshot, SceneTexture, WorkspaceKind};
use anyhow::{Result, ensure};
use kasane_sdk::{AuthoringSnapshot, MotionPreview, MotionSnapshot};
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
}
impl MotionResource {
    pub fn playable(&self) -> bool {
        self.duration.is_finite() && self.duration > 0. && self.duration * 60. <= 1_000_000.
    }
}

impl Document {
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
}

pub struct AnimationRequest {
    pub identity: AnimationIdentity,
    pub source: Arc<AnimationSource>,
    pub motion: Option<MotionSelection>,
}

pub struct AnimationEvaluated {
    pub identity: AnimationIdentity,
    pub scene: SceneSnapshot,
    pub snapshot: Arc<MotionSnapshot>,
}

/// Background-owned SDK adapter. Immutable descriptions make latest-request
/// coalescing safe: every motion selection is a complete replacement recipe.
#[derive(Default)]
pub struct AnimationEvaluator {
    preview: Option<MotionPreview>,
    identity: Option<AnimationIdentity>,
}
impl AnimationEvaluator {
    pub fn evaluate(
        &mut self,
        request: &AnimationRequest,
        progress: impl FnMut(u32, u32) -> bool,
    ) -> Result<AnimationEvaluated> {
        let id = request.identity;
        let reusable = self.identity.is_some_and(|previous| {
            previous.document == id.document
                && previous.model_generation == id.model_generation
                && previous.preview_generation == id.preview_generation
        });
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
            self.preview = Some(preview);
            self.identity = Some(id);
        }
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
    pub time: f32,
    pub playing: bool,
    pub presented: Option<Arc<MotionSnapshot>>,
    pub error: Option<String>,
    pub model_generation: u64,
    pub preview_generation: u64,
    source: Option<Arc<AnimationSource>>,
    requested: Option<(u64, u64, f32)>,
    floor: u64,
    shown: u64,
}
impl AnimationSession {
    pub(crate) fn new(document: &Document) -> Self {
        Self {
            motion: None,
            time: 0.,
            playing: false,
            presented: None,
            error: None,
            model_generation: document.model_generation(),
            preview_generation: 1,
            source: None,
            requested: None,
            floor: 0,
            shown: 0,
        }
    }
    pub(crate) fn pause(&mut self) {
        self.playing = false;
        // An explicit pause needs a fresh request even at an unchanged time.
        // Requests already in flight cannot replace this target frame.
        self.barrier();
    }
    fn barrier(&mut self) {
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
    pub(crate) fn reconcile(&mut self, document: &Document) {
        let generation = document.model_generation();
        if generation == self.model_generation {
            return;
        }
        self.model_generation = generation;
        self.source = None;
        self.rebuild();
        if let Some(selection) = &self.motion {
            if let Some(resource) = document
                .motion_resources()
                .iter()
                .find(|r| &r.selection == selection)
            {
                self.time = self.time.min(resource.duration);
            } else {
                self.motion = None;
                self.time = 0.;
                self.error = Some("Selected motion registration no longer exists".into());
            }
        }
    }
    fn accepts(&self, id: AnimationIdentity) -> bool {
        id.model_generation == self.model_generation
            && id.preview_generation == self.preview_generation
            && id.request >= self.floor
            && id.request > self.shown
            && self
                .requested
                .is_some_and(|(latest, _, _)| id.request <= latest)
            && self
                .presented
                .as_ref()
                .is_none_or(|shown| id.time >= shown.time)
    }
}

impl Editor {
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
            a.playing
                || a.motion.as_ref().is_some_and(|selection| {
                    self.document().is_some_and(|d| {
                        d.motion_resources()
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
        let session = self.active_session_mut().unwrap();
        ensure!(
            session
                .document()
                .motion_resources()
                .iter()
                .any(|r| r.selection == selection),
            "Motion registration no longer exists"
        );
        session.animation.motion = Some(selection);
        session.animation.time = 0.;
        session.animation.rebuild();
        Ok(())
    }
    pub fn pause_animation(&mut self) {
        if let Some(session) = self.active_session_mut() {
            if session.animation.playing {
                session.animation.pause();
            }
        }
    }
    pub fn toggle_animation(&mut self) -> Result<()> {
        let id = self
            .active_id()
            .ok_or_else(|| anyhow::anyhow!("No active document"))?;
        self.ensure_animation_target(id)?;
        let session = self.active_session_mut().unwrap();
        let motion = session.animation.motion.as_ref().and_then(|selection| {
            session
                .document()
                .motion_resources()
                .into_iter()
                .find(|r| &r.selection == selection)
        });
        let animation = &mut session.animation;
        if animation.playing {
            animation.pause();
            return Ok(());
        }
        ensure!(
            motion.as_ref().is_some_and(MotionResource::playable),
            "Select a playable motion"
        );
        let motion = motion.unwrap();
        if !motion.looping && animation.time >= motion.duration {
            animation.time = 0.;
            animation.rebuild();
        }
        animation.error = None;
        animation.playing = true;
        Ok(())
    }
    pub fn rewind_animation(&mut self) -> Result<()> {
        let id = self
            .active_id()
            .ok_or_else(|| anyhow::anyhow!("No active document"))?;
        self.ensure_animation_target(id)?;
        let animation = &mut self.active_session_mut().unwrap().animation;
        animation.time = 0.;
        animation.rebuild();
        Ok(())
    }
    pub fn advance_animation_clock(&mut self, target: f32) {
        if !target.is_finite() || target < 0. {
            return;
        }
        if let Some(session) = self.active_session_mut() {
            let motion = session.animation.motion.as_ref().and_then(|selection| {
                session
                    .document()
                    .motion_resources()
                    .into_iter()
                    .find(|r| &r.selection == selection)
            });
            let animation = &mut session.animation;
            if !animation.playing {
                return;
            }
            if let Some(motion) = motion {
                let target = target.max(animation.time);
                animation.time = if motion.looping {
                    target
                } else {
                    target.min(motion.duration)
                };
                if !motion.looping && animation.time >= motion.duration {
                    animation.pause();
                }
            }
        }
    }
    fn ensure_animation_target(&self, document: DocumentId) -> Result<()> {
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
            },
            source,
            motion: animation.motion.clone(),
        })
    }
    pub fn accepts_animation_image(&self, identity: AnimationIdentity) -> bool {
        self.workspace() == WorkspaceKind::Animation
            && self.workspace_revision() == identity.workspace_revision
            && self.active_id() == Some(identity.document)
            && self.animation().is_some_and(|a| {
                a.model_generation == identity.model_generation
                    && a.preview_generation == identity.preview_generation
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
        animation.presented = Some(snapshot);
        true
    }
    pub fn fail_animation(&mut self, identity: AnimationIdentity, error: String) {
        if self.accepts_animation(identity) {
            let animation = &mut self.active_session_mut().unwrap().animation;
            animation.playing = false;
            animation.error = Some(error);
        }
    }
}
