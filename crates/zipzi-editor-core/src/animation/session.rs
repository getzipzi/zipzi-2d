//! Playback state transitions and reconciliation after document edits.
use super::*;

impl AnimationSession {
    pub(crate) fn is_scrubbing(&self) -> bool {
        self.scrubbing
    }
    /// Last SDK result coordinates, retained while inspecting raw clip values.
    pub fn result_playback_position(&self) -> Option<&zipzi_sdk::MotionPlaybackPosition> {
        self.result_position.as_ref()
    }
    pub(crate) fn new(document: &Document) -> Self {
        Self {
            physics_preview: Default::default(),
            physics_return: None,
            inspected: None,
            motion: None,
            expression: None,
            expressions: Vec::new(),
            time: 0.,
            edit_time: 0.,
            head_located: true,
            preview_mode: AnimationPreviewMode::default(),
            preview_loop: true,
            playback_speed: 1.,
            candidate: None,
            result_position: None,
            playing: false,
            loop_position: false,
            presented: None,
            error: None,
            model_generation: document.animation_generation(),
            document_revision: document.model_generation(),
            preview_generation: 1,
            source: None,
            frames: provenance::FrameSequence::default(),
            scrubbing: false,
        }
    }
    pub(crate) fn physics_input_changed(&mut self) {
        self.frames.mark_target_dirty();
        self.error = None;
    }
    pub(crate) fn property_preview_changed(&mut self) {
        self.source = None;
        self.rebuild();
    }

    pub(crate) fn pause(&mut self) {
        self.playing = false;
        // An explicit pause needs a fresh request even at an unchanged time.
        // Requests already in flight cannot replace this target frame.
        self.barrier();
    }
    pub(crate) fn set_candidate(&mut self, candidate: Option<TimelineCandidateIdentity>) {
        // Revisions inside one edit are successive preview targets, not new
        // acceptance epochs. Let in-flight work make progress while input keeps
        // replacing the queued target. Begin, commit and cancel remain barriers.
        let continuing = self
            .candidate
            .zip(candidate)
            .is_some_and(|(old, new)| old.edit == new.edit && old.revision <= new.revision);
        if !continuing {
            self.pause();
            self.preview_generation += 1;
        }
        self.candidate = candidate;
        self.source = None;
        self.error = None;
    }
    pub(super) fn barrier(&mut self) {
        if self.scrubbing {
            // A completed drag sample can be ahead of the final pointer target.
            self.presented = None;
        }
        self.scrubbing = false;
        self.frames.barrier();
    }
    pub(crate) fn rebuild(&mut self) {
        // Configuration changes must keep a running manual trial alive.
        let manual_playing = self.physics_preview.manual && self.playing;
        self.pause();
        self.playing = manual_playing;
        self.preview_generation += 1;
        self.presented = None;
        self.error = None;
    }
    pub(super) fn new_run(&mut self) {
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
    pub(super) fn locate(&mut self, time: f32) {
        self.head_located = true;
        self.loop_position = false;
        self.time = time;
        self.pause();
        // Permit a backward frame in this recipe without dropping the SDK seek cache.
        self.presented = None;
        self.error = None;
    }
    pub(crate) fn reconcile(&mut self, document: &Document) {
        let generation = document.animation_generation();
        let revision = document.model_generation();
        if generation == self.model_generation && revision == self.document_revision {
            return;
        }
        self.document_revision = revision;
        if self
            .inspected
            .as_ref()
            .is_some_and(|object| document.object_index().get(object).is_none())
        {
            self.inspected = None;
        }
        // Library additions/removals do not change existing playback inputs.
        // Still retire a deleted active registration without activating a sibling.
        let motion_removed = self.motion.as_ref().is_some_and(|selection| {
            selection.entry.as_ref().is_some_and(|id| {
                document
                    .model()
                    .get_motion_registration(id)
                    .is_none_or(|(_, entry)| entry.clip_id != selection.clip_id)
            })
        });
        if generation == self.model_generation && !motion_removed {
            return;
        }
        self.model_generation = generation;
        self.source = None;
        self.rebuild();
        if self.motion.is_none() && self.expression.is_none() && self.expressions.is_empty() {
            return;
        }
        let catalog = document.animation_catalog();
        if self.physics_preview.manual {
            self.physics_preview
                .baseline
                .retain(|id, _| catalog.parameters.iter().any(|p| &p.id == id));
            self.physics_preview
                .inputs
                .retain(|input| catalog.parameters.iter().any(|p| p.id == input.parameter));
            return;
        }
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
}
