//! Editor playback commands and request construction.
use super::*;

impl Editor {
    pub fn set_animation_preview_mode(
        &mut self,
        document: DocumentId,
        mode: AnimationPreviewMode,
    ) -> Result<()> {
        self.ensure_animation_target(document)?;
        self.cancel_timeline_edit();
        let session = self.active_session_mut().unwrap();
        let animation = &mut session.animation.playback;
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
        session.animation.timeline.binding_generation += 1;
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
        let animation = &mut self.active_session_mut().unwrap().animation.playback;
        animation.preview_loop = looping;
        animation.pause();
        Ok(())
    }
    pub fn animation_viewport_revision(&self) -> Option<u64> {
        self.active_id()
            .zip(self.primary_viewport_id())
            .and_then(|(document, viewport)| {
                self.viewport_state(document, viewport)
                    .map(|state| state.revision)
            })
    }
    pub fn invalidate_animation(&mut self) {
        if let Some(session) = self.active_session_mut() {
            session.animation.playback.barrier();
        }
    }
    /// Cancel outstanding evaluation results without ending playback or a
    /// Timeline scrub when the last animation viewport becomes hidden.
    pub fn invalidate_animation_requests(&mut self) {
        if let Some(session) = self.active_session_mut() {
            session.animation.playback.frames.barrier();
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
        self.active_session()
            .map(|session| &session.animation.playback)
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
        session.animation.playback.motion = Some(selection);
        session.animation.playback.time = 0.;
        session.animation.playback.new_run();
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
        let animation = &mut session.animation.playback;
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
        let animation = &mut self.active_session_mut().unwrap().animation.playback;
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
        let animation = &mut self.active_session_mut().unwrap().animation.playback;
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
        self.timeline_scrub_owner = None;
        self.cancel_timeline_edit();
        ensure!(time.is_finite() && time >= 0., "Invalid animation time");
        let session = self.active_session_mut().unwrap();
        let catalog = session.document().animation_catalog();
        let motion = session
            .animation
            .playback
            .motion
            .as_ref()
            .and_then(|selection| catalog.motions.iter().find(|r| &r.selection == selection))
            .filter(|r| r.playable())
            .ok_or_else(|| anyhow::anyhow!("Select a seekable motion"))?;
        let animation = &mut session.animation.playback;
        let max_time = session
            .animation
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
        self.timeline_scrub_owner = None;
        if let Some(session) = self.active_session_mut() {
            if session.animation.playback.playing || session.animation.playback.scrubbing {
                if let Some(frame) = session.animation.playback.presented.as_ref().filter(|_| {
                    session.animation.playback.playing && !session.animation.playback.scrubbing
                }) {
                    session.animation.playback.time = frame.time;
                    session.animation.playback.edit_time = session
                        .animation
                        .playback
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
                session.animation.playback.pause();
            }
        }
    }
    pub fn toggle_animation(&mut self) -> Result<()> {
        self.timeline_scrub_owner = None;
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
            .playback
            .motion
            .as_ref()
            .and_then(|selection| catalog.motions.iter().find(|r| &r.selection == selection));
        let animation = &mut session.animation.playback;
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
        let animation = &mut self.active_session_mut().unwrap().animation.playback;
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
            let motion = session
                .animation
                .playback
                .motion
                .as_ref()
                .and_then(|selection| catalog.motions.iter().find(|r| &r.selection == selection));
            let animation = &mut session.animation.playback;
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
            self.editing_context() == Some(crate::EditingContext::Animation)
                && self.active_id() == Some(document)
                && !self.is_busy()
                && !self.exiting(),
            "Animation context changed"
        );
        Ok(())
    }
    pub fn animation_request(&mut self) -> Option<AnimationRequest> {
        let document = self.active_id()?;
        if self.is_busy() || self.exiting() {
            return None;
        }
        let binding = self
            .visible_viewport_bindings()
            .into_iter()
            .find(|binding| {
                binding.kind == crate::ViewportKind::Animation
                    && self
                        .viewport_metrics(binding.viewport)
                        .is_some_and(|metrics| metrics.visible())
            })?;
        let workspace_revision = self.workspace_revision();
        let metrics = self.viewport_metrics(binding.viewport)?;
        let viewport_id = binding.viewport;
        let session = self.active_session_mut()?;
        session.fit_default_view(viewport_id, metrics);
        session.reconcile_animation();
        let source = if let Some(source) = &session.animation.playback.source {
            source.clone()
        } else {
            let source = Arc::new(session.document().animation_source());
            session.animation.playback.source = Some(source.clone());
            source
        };
        let animation = &mut session.animation.playback;
        if animation.frames.latest().is_some_and(|previous| {
            previous.time == animation.time && previous.candidate == animation.candidate
        }) {
            return None;
        }
        // Monotonic across barriers and recipe changes, including repeated seeks to zero.
        let sequence = animation.frames.next();
        let identity = AnimationIdentity {
            document,
            workspace: WorkspaceKind::Animation,
            workspace_revision,
            model_generation: animation.model_generation,
            preview_generation: animation.preview_generation,
            request: sequence,
            acceptance_floor: animation.frames.acceptance_floor(),
            time: animation.time,
            preview_mode: animation.preview_mode,
            candidate: animation.candidate,
        };
        animation.frames.record(identity);
        Some(AnimationRequest {
            identity,
            source,
            motion: animation.motion.clone(),
            expressions: animation.expressions.clone(),
        })
    }
}
