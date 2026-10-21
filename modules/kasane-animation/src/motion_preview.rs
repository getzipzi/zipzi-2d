//! Detached MotionBehavior V2 preview for typed project clips.
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use kasane_core::{
    evaluation::{evaluate_frame_including_hidden, EvaluationTimings},
    Document, DrawableFrame, FrameEvaluator, PreviewValues,
};

use crate::expression::{ExpressionRuntime, ExpressionTransition};
use crate::motion_runtime::MotionRuntime;
use crate::physics::PhysicsRuntime;
use crate::pose::PoseRuntime;
use crate::seek_cache::{Checkpoint, SeekCache};
use crate::{AnimationError, CompiledCurve, SeekCacheStats};

/// A motion event crossed during the last update, returned without side effects.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MotionEventFired {
    /// Source clip UUID.
    pub motion_id: String,
    /// Source event UUID.
    pub event_id: String,
    /// User-authored event text.
    pub value: String,
}

/// Combined values after Motion, Expression, Physics and Pose evaluation.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MotionSnapshot {
    /// Optional inspector observations. Collected explicitly by a host after
    /// evaluation. Advance/seek results leave this empty; populate it with
    /// `snapshot.physics = preview.physics_snapshot();` when displaying an inspector.
    /// Internal observations are retained in replay checkpoints for consistent seeks.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub physics: Vec<crate::PhysicsRigSnapshot>,
    /// Absolute preview time in seconds.
    pub time: f32,
    /// Final parameter values keyed by parameter UUID.
    pub parameters: BTreeMap<String, f32>,
    /// PartOpacity curves write virtual controls when no real parameter has the Part runtime ID.
    pub part_opacity_channels: BTreeMap<String, f32>,
    /// Pose-produced opacities keyed by Part UUID.
    pub part_opacities: BTreeMap<String, f32>,
    /// Model Opacity channel, separate from the drawable geometry opacity.
    pub model_opacity: f32,
    /// Active clip UUIDs in queue order; repeated activations may repeat a UUID.
    pub active_motions: Vec<String>,
    /// Active expression UUIDs in queue order, including fading-out entries.
    pub active_expressions: Vec<String>,
    /// Events crossed by the last update, not accumulated across all seek steps.
    pub fired_events: Vec<MotionEventFired>,
    /// Model EyeBlink/LipSync mappings and unsupported Model IDs are reported.
    pub coverage: Vec<String>,
    /// SDK sampling coordinates, including the duration + 1 / FPS loop tail.
    pub motion_positions: Vec<MotionPlaybackPosition>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MotionPlaybackPosition {
    pub motion_id: String,
    pub local_time: f32,
    pub period: f32,
    pub loop_index: u64,
    pub cycle_origin: f32,
}

/// Identity of the last successful preview operation. This is evidence of
/// provenance, not a serialized simulation state or a replay recipe.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct MotionOperation {
    pub preview_id: String,
    pub sequence: u64,
    pub kind: MotionOperationKind,
    pub actual_time: f32,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
pub enum MotionOperationKind {
    SetPhysicsEnabled {
        enabled: bool,
    },
    SetPhysicsStabilizedStart {
        enabled: bool,
    },
    Created,
    Reset,
    SetBaseParameter {
        parameter_id: String,
        value: f32,
    },
    ScheduleMotion {
        motion_id: String,
        time: f32,
    },
    ScheduleMotionRegistration {
        registration_id: String,
        motion_id: String,
        time: f32,
    },
    ScheduleMotionEntry {
        group: String,
        index: usize,
        motion_id: String,
        time: f32,
    },
    ScheduleExpression {
        expression_id: String,
        time: f32,
    },
    ReplaceExpressionSchedule {
        activations: Vec<(String, f32, ExpressionTransition)>,
    },
    ScheduleParameterInput {
        parameter_id: String,
        time: f32,
        value: f32,
    },
    Advance {
        dt: f32,
    },
    Seek {
        requested_time: f32,
    },
    StabilizePhysics,
}

#[derive(Default)]
struct GeometryWorkspace {
    values: PreviewValues,
    evaluator: FrameEvaluator,
}

/// Additional work around the core evaluator during a timed preview query.
#[derive(Clone, Copy, Debug, Default)]
pub struct PreviewGeometryTimings {
    pub workspace_setup: Duration,
    pub core: EvaluationTimings,
    pub part_opacity: Duration,
}

/// Detached combined preview with immutable assets and bounded canonical seek checkpoints.
/// Create a new preview after authoring edits; playback never edits the source document.
pub struct MotionPreview {
    document: Arc<Document>,
    curves: Arc<BTreeMap<String, CompiledCurve>>,
    source_identity: Option<(u64, u64)>,
    base: BTreeMap<String, f32>,
    snapshot: MotionSnapshot,
    pose: PoseRuntime,
    physics: PhysicsRuntime,
    physics_enabled: bool,
    physics_stabilized_start: bool,
    expressions: ExpressionRuntime,
    motion: MotionRuntime,
    cache: SeekCache,
    operation: MotionOperation,
    // Scratch only, not simulation state. A mutex preserves the shared-reference
    // API and Send + Sync while independent previews use independent workspaces.
    geometry: Mutex<GeometryWorkspace>,
    canonical_step: Option<u32>,
    // Full simulation state before a presentation-only fractional tail.
    fractional_base: Option<Checkpoint>,
}

impl MotionPreview {
    /// Capture an independent document snapshot at time zero. Later document edits are not observed.
    pub fn new(document: &Document) -> Self {
        let curves = document
            .motion_order()
            .iter()
            .flat_map(|id| &document.get_motion(id).expect("ordered motion").tracks)
            .map(|track| (track.id.clone(), CompiledCurve::from(track)))
            .collect();
        Self::from_shared(Arc::new(document.clone()), Arc::new(curves))
    }

    fn from_shared(document: Arc<Document>, curves: Arc<BTreeMap<String, CompiledCurve>>) -> Self {
        let base: BTreeMap<String, f32> = document
            .parameter_order()
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    document
                        .get_parameter(id)
                        .expect("ordered parameter")
                        .default_value,
                )
            })
            .collect();
        let mut parameters = base.clone();
        let mut virtual_controls = BTreeMap::new();
        let pose = PoseRuntime::new(&document, &mut parameters, &mut virtual_controls);
        Self {
            document: Arc::clone(&document),
            curves,
            source_identity: None,
            geometry: Mutex::default(),
            canonical_step: None,
            fractional_base: None,
            cache: SeekCache::default(),
            operation: MotionOperation {
                preview_id: uuid::Uuid::new_v4().to_string(),
                sequence: 0,
                kind: MotionOperationKind::Created,
                actual_time: 0.0,
            },
            motion: MotionRuntime::new(parameters.clone()),
            physics: PhysicsRuntime::new(&document),
            physics_enabled: true,
            physics_stabilized_start: false,
            expressions: ExpressionRuntime::default(),
            base,
            snapshot: MotionSnapshot {
                physics: Vec::new(),
                time: 0.0,
                parameters,
                part_opacity_channels: virtual_controls,
                part_opacities: pose.opacities.clone(),
                model_opacity: 1.0,
                active_motions: Vec::new(),
                active_expressions: Vec::new(),
                fired_events: Vec::new(),
                coverage: Vec::new(),
                motion_positions: Vec::new(),
            },
            pose,
        }
    }
    /// Return the captured document revision, not the current authoring-session revision.
    pub fn document_revision(&self) -> u64 {
        self.document.revision()
    }

    /// Rebuild this detached preview when comparing with physics bypassed.
    /// Schedules are retained; the authoring document is untouched.
    pub fn set_physics_enabled(&mut self, enabled: bool) {
        if self.physics_enabled != enabled {
            self.physics_enabled = enabled;
            self.reset_state();
            self.commit_operation(MotionOperationKind::SetPhysicsEnabled { enabled });
        }
    }

    /// Start replay at equilibrium for the configured baseline parameters.
    /// Intended for detached rig audition; schedules and the document are retained.
    pub fn set_physics_stabilized_start(&mut self, enabled: bool) {
        if self.physics_stabilized_start != enabled {
            self.physics_stabilized_start = enabled;
            self.reset_state();
            self.commit_operation(MotionOperationKind::SetPhysicsStabilizedStart { enabled });
        }
    }

    /// Copy observations without advancing simulation. Inputs are from the latest
    /// solved step; outputs reflect the latest application to model parameters.
    /// Returns empty when disabled, unresolved, or no step has been solved.
    /// Reset clears observations unless stabilized start initializes equilibrium.
    /// Returned data is owned and remains unchanged after later advances or resets.
    pub fn physics_snapshot(&self) -> Vec<crate::PhysicsRigSnapshot> {
        if self.physics_enabled {
            self.physics.snapshot(&self.document)
        } else {
            Vec::new()
        }
    }
    /// Associate the preview with an observer session and generation.
    /// This metadata does not change playback, schedules or cached checkpoints.
    pub fn bind_session_identity(&mut self, session_id: u64, generation: u64) {
        self.source_identity = Some((session_id, generation));
    }
    /// Return the observer session/generation, or `None` for an unbound preview.
    pub fn source_identity(&self) -> Option<(u64, u64)> {
        self.source_identity
    }
    /// Return the captured document UUID.
    pub fn document_id(&self) -> &str {
        self.document.id()
    }
    /// Borrow the current snapshot without advancing playback.
    pub fn snapshot(&self) -> &MotionSnapshot {
        &self.snapshot
    }

    pub fn operation(&self) -> &MotionOperation {
        &self.operation
    }

    fn commit_operation(&mut self, kind: MotionOperationKind) {
        self.fractional_base = None;
        self.operation.sequence += 1;
        self.operation.kind = kind;
        self.operation.actual_time = self.snapshot.time;
    }

    /// Schedule a clip UUID using clip fades (no model3 registration overrides).
    /// Time is finite, nonnegative absolute seconds and cannot precede current time.
    /// Equal-time calls retain order; activation starts on the first update at or after time.
    /// Returns `InvalidTime`, `PastActivation` or `MissingMotion` on invalid input.
    /// Unresolved imported tracks are skipped and reported in snapshot coverage.
    /// Success clears seek checkpoints/statistics; failure changes nothing.
    pub fn schedule_motion(&mut self, id: &str, time: f32) -> Result<(), AnimationError> {
        self.motion
            .schedule_motion(&self.document, self.snapshot.time, id, time, (None, None))?;
        self.cache.clear();
        self.canonical_step = None;
        self.commit_operation(MotionOperationKind::ScheduleMotion {
            motion_id: id.into(),
            time,
        });
        Ok(())
    }

    /// Schedule a model3 group entry at an absolute preview time in seconds.
    ///
    /// `group` is an exact group name and `index` is zero-based. Parameter-curve
    /// fades override registration fades, which override clip fades (default:
    /// one second). Each activation retains its own fades. Sound is not played.
    /// Success clears seek checkpoints and statistics without resetting playback.
    ///
    /// # Errors
    /// Returns [`AnimationError::MissingMotionEntry`] for an unknown group/index,
    /// [`AnimationError::InvalidTime`] for nonfinite or negative time,
    /// or [`AnimationError::PastActivation`] for time before the current preview.
    /// Unresolved MOC targets remain in the clip and are reported as coverage
    /// diagnostics when sampled, since Cubism permits virtual parameters.
    /// Failure leaves playback, schedules and the cache unchanged.
    pub fn schedule_motion_entry(
        &mut self,
        group: &str,
        index: usize,
        time: f32,
    ) -> Result<(), AnimationError> {
        let entry = self
            .document
            .motion_groups()
            .iter()
            .find(|item| item.name == group)
            .and_then(|item| item.entries.get(index))
            .ok_or_else(|| AnimationError::MissingMotionEntry {
                group: group.into(),
                index,
            })?;
        self.motion.schedule_motion(
            &self.document,
            self.snapshot.time,
            &entry.clip_id,
            time,
            (entry.fade_in, entry.fade_out),
        )?;
        self.cache.clear();
        self.canonical_step = None;
        self.commit_operation(MotionOperationKind::ScheduleMotionEntry {
            group: group.into(),
            index,
            motion_id: entry.clip_id.clone(),
            time,
        });
        Ok(())
    }

    /// Stable registration activation, including root registration overrides.
    pub fn schedule_motion_registration(
        &mut self,
        id: &str,
        time: f32,
    ) -> Result<(), AnimationError> {
        let (_, entry) = self
            .document
            .get_motion_registration(id)
            .ok_or_else(|| AnimationError::MissingMotion(id.into()))?;
        self.motion.schedule_motion(
            &self.document,
            self.snapshot.time,
            &entry.clip_id,
            time,
            (entry.fade_in, entry.fade_out),
        )?;
        let motion_id = entry.clip_id.clone();
        self.cache.clear();
        self.canonical_step = None;
        self.commit_operation(MotionOperationKind::ScheduleMotionRegistration {
            registration_id: id.into(),
            motion_id,
            time,
        });
        Ok(())
    }
    /// Set the retained checkpoint budget in bytes (default: 16 MiB).
    ///
    /// Every call clears checkpoints and seek statistics, even if the budget is
    /// unchanged; zero disables caching. Playback and schedules are preserved.
    /// The budget uses conservative allocation estimates, excluding shared
    /// document/curve data and temporary transactional seek state. Oldest inserted
    /// checkpoints are evicted first; a checkpoint larger than the budget is skipped.
    pub fn set_seek_cache_budget(&mut self, bytes: usize) {
        self.cache.set_budget(bytes);
    }
    /// Discard all checkpoints and zero seek statistics, retaining the budget.
    /// Playback, baseline values and schedules are unchanged.
    pub fn clear_seek_cache(&mut self) {
        self.cache.clear();
    }
    /// Return a detached copy of cache usage and last successful seek statistics.
    /// Reading statistics does not mutate playback or the cache.
    pub fn seek_cache_stats(&self) -> SeekCacheStats {
        self.cache.stats
    }

    fn checkpoint_bytes(&self) -> usize {
        use crate::seek_cache::{map_bytes, strings_bytes, vec_bytes};
        // Include Arc and cache-entry allocation overhead in each estimate.
        std::mem::size_of::<Checkpoint>()
            + 128
            + map_bytes(&self.snapshot.parameters)
            + map_bytes(&self.snapshot.part_opacity_channels)
            + map_bytes(&self.snapshot.part_opacities)
            + strings_bytes(&self.snapshot.active_motions)
            + vec_bytes(&self.snapshot.motion_positions)
            + self
                .snapshot
                .motion_positions
                .iter()
                .map(|position| position.motion_id.capacity())
                .sum::<usize>()
            + strings_bytes(&self.snapshot.active_expressions)
            + strings_bytes(&self.snapshot.coverage)
            + vec_bytes(&self.snapshot.fired_events)
            + self
                .snapshot
                .fired_events
                .iter()
                .map(|e| e.motion_id.capacity() + e.event_id.capacity() + e.value.capacity())
                .sum::<usize>()
            + self.motion.heap_bytes()
            + self.expressions.heap_bytes()
            + self.physics.heap_bytes()
            + self.pose.heap_bytes()
    }

    /// Schedule an expression UUID at an absolute time in seconds; equal-time calls retain order.
    /// Activation starts on the first update at or after the scheduled time.
    /// Returns `InvalidTime` for nonfinite/negative time, `PastActivation` for past time,
    /// `MissingExpression` for an unknown UUID, or `UnresolvedParameter` for unresolved
    /// targets. Failure preserves state; Motion preview clears its cache on success.
    pub fn schedule_expression(&mut self, id: &str, time: f32) -> Result<(), AnimationError> {
        self.expressions
            .schedule(&self.document, self.snapshot.time, id, time)?;
        self.cache.clear();
        self.canonical_step = None;
        self.commit_operation(MotionOperationKind::ScheduleExpression {
            expression_id: id.into(),
            time,
        });
        Ok(())
    }

    /// Replace the complete expression schedule, preserving unchanged simulation history.
    /// Times are absolute, finite and nonnegative; equal-time entries retain input order.
    /// Unlike `schedule_expression`, past activations can be edited or removed.
    /// Invalid entries leave playback, schedules and cache unchanged.
    ///
    /// Checkpoints at or after the first changed activation are discarded. If the
    /// current frame is affected, playback rewinds to the last unaffected checkpoint
    /// (or the initial state). Call `advance_to_with_progress` to evaluate the desired
    /// time before presenting it. Identical schedules leave all state unchanged.
    pub fn replace_expression_schedule<'a>(
        &mut self,
        schedule: impl IntoIterator<Item = (&'a str, f32)>,
    ) -> Result<(), AnimationError> {
        self.replace_expression_schedule_with_transitions(
            schedule
                .into_iter()
                .map(|(id, time)| (id, time, ExpressionTransition::Fade)),
        )
    }

    /// Replace a schedule with per-activation transitions. Immediate activations
    /// discard the previous expression queue and apply at full weight without
    /// advancing time. Faded activations retain authored fade-in/out behavior.
    /// Validation, replay and cache invalidation match `replace_expression_schedule`.
    pub fn replace_expression_schedule_with_transitions<'a>(
        &mut self,
        schedule: impl IntoIterator<Item = (&'a str, f32, ExpressionTransition)>,
    ) -> Result<(), AnimationError> {
        let mut replacement = ExpressionRuntime::default();
        let mut activations = Vec::new();
        for (id, time, transition) in schedule {
            replacement.schedule_with_transition(&self.document, 0.0, id, time, transition)?;
            activations.push((id.to_owned(), time, transition));
        }
        let Some(changed_at) = self.expressions.first_schedule_difference(&replacement) else {
            return Ok(());
        };
        self.cache.retain_before(changed_at);
        if self.snapshot.time >= changed_at {
            if let Some(state) = self.cache.nearest(changed_at) {
                self.snapshot.clone_from(&state.snapshot);
                self.motion.clone_from(&state.motion);
                self.expressions.clone_from(&state.expressions);
                self.physics.clone_from(&state.physics);
                self.pose.clone_from(&state.pose);
                self.canonical_step = Some(state.step);
            } else {
                self.reset_state();
            }
        }
        self.expressions.copy_schedule_from(&replacement);
        self.commit_operation(MotionOperationKind::ReplaceExpressionSchedule { activations });
        Ok(())
    }

    /// Set a parameter UUID baseline, clamp it to its range, then reset playback.
    /// Retains schedules. Missing UUIDs/nonfinite values return [`AnimationError::Evaluation`]
    /// without changing state. Motion preview also clears seek checkpoints/statistics.
    pub fn set_base_parameter(&mut self, id: &str, value: f32) -> Result<(), AnimationError> {
        let parameter = self
            .document
            .get_parameter(id)
            .ok_or_else(|| AnimationError::Evaluation(format!("missing parameter {id}")))?;
        if !value.is_finite() {
            return Err(AnimationError::Evaluation(format!(
                "nonfinite parameter {id}"
            )));
        }
        let value = value.clamp(parameter.minimum, parameter.maximum);
        self.base.insert(id.into(), value);
        self.reset_state();
        self.commit_operation(MotionOperationKind::SetBaseParameter {
            parameter_id: id.into(),
            value,
        });
        Ok(())
    }

    /// Schedule a parameter UUID override retained by reset and seek.
    /// Time is absolute seconds, finite, nonnegative and no earlier than current time.
    /// Equal-time inputs retain call order and apply before Motion on the first due update.
    /// Values must be finite and are clamped to the parameter range. Invalid time/value
    /// returns `InvalidTime`, past time `PastActivation`, and missing UUID `Evaluation`.
    /// Success clears seek checkpoints/statistics; failure preserves state.
    pub fn schedule_parameter_input(
        &mut self,
        id: &str,
        time: f32,
        value: f32,
    ) -> Result<(), AnimationError> {
        self.motion.schedule_parameter_input(
            &self.document,
            self.snapshot.time,
            id,
            time,
            value,
        )?;
        self.cache.clear();
        self.canonical_step = None;
        self.commit_operation(MotionOperationKind::ScheduleParameterInput {
            parameter_id: id.into(),
            time,
            value,
        });
        Ok(())
    }

    /// Restore time zero and initial Motion/Expression/Physics/Pose state from the baseline.
    /// Retains activation/input schedules and cache budget; clears checkpoints/statistics
    /// and fired events. Time-zero activations require `advance(0)` or `seek(0)`.
    pub fn reset(&mut self) {
        self.reset_state();
        self.commit_operation(MotionOperationKind::Reset);
    }

    fn reset_state(&mut self) {
        self.canonical_step = None;
        self.cache.clear();
        let mut parameters = self.base.clone();
        let mut virtual_controls = BTreeMap::new();
        self.pose
            .reset(&self.document, &mut parameters, &mut virtual_controls);
        self.physics.reset(&self.document);
        if self.physics_enabled && self.physics_stabilized_start {
            self.physics.stabilize(&self.document, &mut parameters);
        }
        self.expressions.reset();
        self.motion.reset(parameters.clone());
        self.snapshot = MotionSnapshot {
            physics: Vec::new(),
            time: 0.0,
            parameters,
            part_opacity_channels: virtual_controls,
            part_opacities: self.pose.opacities.clone(),
            model_opacity: 1.0,
            active_motions: Vec::new(),
            active_expressions: Vec::new(),
            fired_events: Vec::new(),
            coverage: Vec::new(),
            motion_positions: Vec::new(),
        };
    }

    /// Advance Motion → Expression → Physics → Pose by finite, nonnegative seconds.
    /// Zero evaluates activations due now. Invalid delta/clock overflow returns
    /// `InvalidTime`; an estimated event batch over one million returns `EventLimit`.
    /// These failures preserve playback. Arbitrary advances do not populate seek checkpoints.
    pub fn advance(&mut self, dt: f32) -> Result<&MotionSnapshot, AnimationError> {
        if !dt.is_finite() || dt < 0.0 || !(self.snapshot.time + dt).is_finite() {
            return Err(AnimationError::InvalidTime);
        }
        let time = self.snapshot.time + dt;
        self.motion
            .evaluate(&self.document, &self.curves, time, &mut self.snapshot)?;
        self.expressions
            .evaluate(&self.document, time, &mut self.snapshot.parameters);
        self.snapshot.active_expressions = self.expressions.active_ids();
        if self.physics_enabled {
            self.physics.evaluate(
                &self.document,
                &mut self.snapshot.parameters,
                dt,
                &mut self.snapshot.coverage,
            );
        }
        self.pose.update(
            &self.document,
            &self.snapshot.parameters,
            &self.snapshot.part_opacity_channels,
            dt,
            &mut self.snapshot.coverage,
        );
        crate::parameter_values::copy_values(
            &mut self.snapshot.part_opacities,
            &self.pose.opacities,
        );
        self.canonical_step = None;
        self.snapshot.time = time;
        self.snapshot.coverage.sort();
        self.snapshot.coverage.dedup();
        self.commit_operation(MotionOperationKind::Advance { dt });
        Ok(&self.snapshot)
    }

    /// Initialize particles and physics outputs using current snapshot parameters.
    /// Does not advance time or run Motion/Expression/Pose. Does not seed checkpoints;
    /// subsequent seek still replays the original baseline and scheduled inputs.
    pub fn stabilize_physics(&mut self) {
        if !self.physics_enabled {
            return;
        }
        self.canonical_step = None;
        self.physics
            .stabilize(&self.document, &mut self.snapshot.parameters);
        self.commit_operation(MotionOperationKind::StabilizePhysics);
    }

    /// Seek to absolute seconds using canonical checkpoints or the initial state.
    /// Uses an absolute 60 Hz grid and a partial final step. Time zero evaluates a
    /// zero-length step. Nonfinite/negative time returns `InvalidTime`; time * 60
    /// above one million returns `SeekLimit`, even with cached state. Failure preserves
    /// playback and cache. Returned events are data, with no playback side effects.
    pub fn seek(&mut self, time: f32) -> Result<&MotionSnapshot, AnimationError> {
        self.seek_with_progress(time, |_, _| true)
    }

    /// Replay from a canonical checkpoint, reporting remaining completed/total steps.
    /// An exact cache hit reports (0, 0).
    /// Returning false produces `SeekCancelled`, preserving playback and cache/statistics.
    /// The initial callback runs before replay, followed by one per replay step.
    /// Time constraints and other errors are the same as [`Self::seek`].
    pub fn seek_with_progress(
        &mut self,
        time: f32,
        mut progress: impl FnMut(u32, u32) -> bool,
    ) -> Result<&MotionSnapshot, AnimationError> {
        let mut steps = crate::replay::ReplaySteps::new(time)?;
        let mut candidate = Self::from_shared(Arc::clone(&self.document), Arc::clone(&self.curves));
        candidate.physics_enabled = self.physics_enabled;
        candidate.physics_stabilized_start = self.physics_stabilized_start;
        candidate.source_identity = self.source_identity;
        candidate
            .operation
            .preview_id
            .clone_from(&self.operation.preview_id);
        candidate.base.clone_from(&self.base);
        candidate.motion.copy_schedule_from(&self.motion);
        candidate.expressions.clone_from(&self.expressions);
        candidate.reset_state();
        let mut cache = self.cache.clone();
        let checkpoint = cache.nearest(time);
        let start = checkpoint.as_ref().map_or(0, |state| state.step);
        if let Some(state) = checkpoint {
            candidate.snapshot.clone_from(&state.snapshot);
            candidate.motion.clone_from(&state.motion);
            candidate.expressions.clone_from(&state.expressions);
            // Retained checkpoints may predate an edit to future activations.
            candidate.expressions.copy_schedule_from(&self.expressions);
            candidate.physics.clone_from(&state.physics);
            candidate.pose.clone_from(&state.pose);
        }
        let total = steps.total() - start;
        cache.stats.last_restored_time = candidate.snapshot.time;
        cache.stats.last_replayed_steps = total;
        if !progress(0, total) {
            return Err(AnimationError::SeekCancelled);
        }
        steps.resume_after(start);
        let mut fractional_base = None;
        for (index, next) in steps.enumerate() {
            let step = start + index as u32 + 1;
            if next > 0.0 && next != step as f32 / 60.0 {
                fractional_base = Some(candidate.checkpoint(step - 1));
            }
            candidate.advance((next - candidate.snapshot.time).max(0.0))?;
            // Never retain partial tail steps, seek(0), or arbitrary advance state.
            if step.is_multiple_of(60) && next == step as f32 / 60.0 {
                cache.insert(candidate.checkpoint_bytes(), || Checkpoint {
                    step,
                    snapshot: candidate.snapshot.clone(),
                    motion: candidate.motion.clone(),
                    expressions: candidate.expressions.clone(),
                    physics: candidate.physics.clone(),
                    pose: candidate.pose.clone(),
                });
            }
            if !progress(step - start, total) {
                return Err(AnimationError::SeekCancelled);
            }
        }
        candidate.canonical_step = (time > 0.0 && time == (time * 60.0).round() / 60.0)
            .then_some((time * 60.0).round() as u32);
        candidate.cache = cache;
        candidate.operation.sequence = self.operation.sequence;
        candidate.commit_operation(MotionOperationKind::Seek {
            requested_time: time,
        });
        candidate.fractional_base = fractional_base;
        *self = candidate;
        Ok(&self.snapshot)
    }

    /// Advance on the same absolute 60 Hz grid as canonical seek, reusing the
    /// current simulation or the full step retained before a fractional sample.
    /// Fractional tails never feed into subsequent simulation steps. Backward
    /// targets fall back to transactional seek. No UI frame delta enters physics.
    /// Cancellation retains completed canonical steps; callers should discard
    /// the cancelled result and may resume toward a newer target.
    pub fn advance_to_with_progress(
        &mut self,
        time: f32,
        mut progress: impl FnMut(u32, u32) -> bool,
    ) -> Result<&MotionSnapshot, AnimationError> {
        let mut steps = crate::replay::ReplaySteps::new(time)?;
        let Some(start) = self
            .canonical_step
            .or_else(|| self.fractional_base.as_ref().map(|base| base.step))
            .filter(|_| time >= self.snapshot.time)
        else {
            return self.seek_with_progress(time, progress);
        };
        let total = steps.total() - start;
        if !progress(0, total) {
            return Err(AnimationError::SeekCancelled);
        }
        if let Some(base) = self.fractional_base.take() {
            self.restore_checkpoint(base);
        }
        steps.resume_after(start);
        for (index, next) in steps.enumerate() {
            let step = start + index as u32 + 1;
            let fractional_base = (next != step as f32 / 60.0).then(|| self.checkpoint(step - 1));
            self.advance((next - self.snapshot.time).max(0.0))?;
            self.canonical_step = (next == step as f32 / 60.0).then_some(step);
            if self.canonical_step.is_some() && step.is_multiple_of(60) {
                self.cache.insert(self.checkpoint_bytes(), || Checkpoint {
                    step,
                    snapshot: self.snapshot.clone(),
                    motion: self.motion.clone(),
                    expressions: self.expressions.clone(),
                    physics: self.physics.clone(),
                    pose: self.pose.clone(),
                });
            }
            self.fractional_base = fractional_base;
            if !progress(step - start, total) {
                if let Some(base) = self.fractional_base.take() {
                    self.restore_checkpoint(base);
                }
                return Err(AnimationError::SeekCancelled);
            }
        }
        Ok(&self.snapshot)
    }

    fn checkpoint(&self, step: u32) -> Checkpoint {
        Checkpoint {
            step,
            snapshot: self.snapshot.clone(),
            motion: self.motion.clone(),
            expressions: self.expressions.clone(),
            physics: self.physics.clone(),
            pose: self.pose.clone(),
        }
    }

    fn restore_checkpoint(&mut self, state: Checkpoint) {
        self.snapshot = state.snapshot;
        self.motion = state.motion;
        self.expressions = state.expressions;
        self.physics = state.physics;
        self.pose = state.pose;
        self.canonical_step = Some(state.step);
        self.operation.actual_time = self.snapshot.time;
    }

    /// Evaluate geometry and multiply each drawable by its ancestor Part opacities.
    /// Model opacity remains separate in [`MotionSnapshot::model_opacity`].
    /// Does not advance playback. Returns [`AnimationError::Evaluation`] on failure.
    pub fn evaluate_drawables(&self) -> Result<DrawableFrame, AnimationError> {
        self.evaluate_drawables_with_hidden_geometry(false, None)
    }

    /// Evaluate the same frame while measuring its CPU stages.
    pub fn evaluate_drawables_timed(
        &self,
    ) -> Result<(DrawableFrame, PreviewGeometryTimings), AnimationError> {
        let mut timings = PreviewGeometryTimings::default();
        let frame = self.evaluate_drawables_with_hidden_geometry(false, Some(&mut timings))?;
        Ok((frame, timings))
    }

    /// Preserve hidden mesh positions for frozen observation queries.
    pub fn evaluate_drawables_including_hidden(&self) -> Result<DrawableFrame, AnimationError> {
        self.evaluate_drawables_with_hidden_geometry(true, None)
    }

    fn evaluate_drawables_with_hidden_geometry(
        &self,
        include_hidden: bool,
        mut timings: Option<&mut PreviewGeometryTimings>,
    ) -> Result<DrawableFrame, AnimationError> {
        let setup_start = timings.as_ref().map(|_| Instant::now());
        let mut workspace = self
            .geometry
            .lock()
            .map_err(|_| AnimationError::Evaluation("Geometry workspace lock poisoned".into()))?;
        let GeometryWorkspace { values, evaluator } = &mut *workspace;
        // The captured document has an immutable parameter set. Reuse the
        // UUID keys instead of allocating a HashMap and strings on every call.
        for (id, value) in &self.snapshot.parameters {
            if let Some(slot) = values.get_mut(id) {
                *slot = *value;
            } else {
                values.insert(id.clone(), *value);
            }
        }
        let mut frame = DrawableFrame::default();
        if let Some(timings) = timings.as_deref_mut() {
            timings.workspace_setup = setup_start.unwrap().elapsed();
        }
        let status = if include_hidden {
            evaluate_frame_including_hidden(&self.document, values, &mut frame)
        } else if let Some(timings) = timings.as_deref_mut() {
            evaluator.evaluate_timed(&self.document, values, &mut frame, &mut timings.core)
        } else {
            evaluator.evaluate(&self.document, values, &mut frame)
        };
        if status.is_ok() {
            // Core evaluation does not carry runtime Part opacity. Apply it to
            // each drawable once; Offscreen opacity is an independent factor.
            let opacity_start = timings.as_ref().map(|_| Instant::now());
            for drawable in &mut frame.drawables {
                let mut part_id = drawable.part_id.as_str();
                let mut opacity = 1.0;
                while !part_id.is_empty() {
                    opacity *= self
                        .snapshot
                        .part_opacities
                        .get(part_id)
                        .copied()
                        .unwrap_or(1.0);
                    part_id = self
                        .document
                        .get_part(part_id)
                        .map_or("", |part| part.parent_id.as_str());
                }
                drawable.opacity *= opacity;
            }
            if let Some(timings) = timings {
                timings.part_opacity = opacity_start.unwrap().elapsed();
            }
            Ok(frame)
        } else {
            Err(AnimationError::Evaluation(format!(
                "{}: {}",
                status.code, status.message
            )))
        }
    }
}
