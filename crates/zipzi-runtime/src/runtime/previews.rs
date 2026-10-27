//! Each entry has its own SDK simulation; workers never borrow a formal session.
use super::*;
use crate::{jobs::ReadTask, preview::*};
use std::sync::Mutex;

pub(super) struct PreviewEntry {
    pub client: ClientId,
    pub source: SourceSnapshot,
    pub state: Arc<Mutex<zipzi_sdk::MotionPreview>>,
    pub sequence: u64,
    pub busy: Option<JobId>,
    bytes: usize,
    motion_output_bytes: usize,
    schedule_entries: usize,
}

/// A batch's simulation and GPU resources stay with its Job across dispatches.
pub(crate) struct PreviewSampling {
    preview_id: u64,
    sequence: u64,
    state: Arc<Mutex<zipzi_sdk::MotionPreview>>,
    times: VecDeque<f32>,
    geometry: bool,
    incremental: bool,
    queries: Vec<zipzi_sdk::SpatialQuery>,
    view: Option<crate::observation::ObservationView>,
    initialized: bool,
    #[cfg(feature = "gpu-observation")]
    observer: Option<zipzi_sdk_observe::Observer>,
    #[cfg(feature = "gpu-observation")]
    inputs: Vec<zipzi_sdk_observe::ObservationInput>,
    #[cfg(feature = "gpu-observation")]
    captures: Option<VecDeque<zipzi_sdk_observe::ResolvedObservation>>,
    frames: Vec<PreviewFrame>,
}
impl PreviewSampling {
    pub(crate) fn new(
        preview_id: u64,
        sequence: u64,
        state: Arc<Mutex<zipzi_sdk::MotionPreview>>,
        times: Vec<f32>,
        geometry: bool,
        incremental: bool,
        queries: Vec<zipzi_sdk::SpatialQuery>,
        view: Option<crate::observation::ObservationView>,
    ) -> Self {
        Self {
            preview_id,
            sequence,
            state,
            times: times.into(),
            geometry,
            incremental,
            queries,
            view,
            initialized: false,
            #[cfg(feature = "gpu-observation")]
            observer: None,
            #[cfg(feature = "gpu-observation")]
            inputs: vec![],
            #[cfg(feature = "gpu-observation")]
            captures: None,
            frames: vec![],
        }
    }
    pub(crate) fn step(
        &mut self,
        source: &SourceSnapshot,
        cancel: &AtomicBool,
        control: &zipzi_sdk::ConversionControl,
    ) -> Result<Option<JobValue>> {
        crate::observation::check(cancel)?;
        if !self.initialized {
            zipzi_sdk::validate_spatial_queries(
                source.snapshot.document(),
                &self.queries,
                control,
            )?;
            if self.view.is_some() {
                crate::observation::require_gpu_observation()?;
            }
            #[cfg(feature = "gpu-observation")]
            {
                self.observer = self.view.map(crate::observation::observer).transpose()?;
            }
            self.initialized = true;
        }
        if let Some(time) = self.times.pop_front() {
            let mut state = self
                .state
                .lock()
                .map_err(|_| RuntimeError::WorkerPanic("preview worker panicked".into()))?;
            let progress = |_, _| !cancel.load(Ordering::Acquire);
            if self.incremental {
                state.advance_to_with_progress(time, progress)
            } else {
                state.seek_with_progress(time, progress)
            }
            .map_err(animation_error)?;
            let spatial = if self.queries.is_empty() {
                vec![]
            } else {
                let frame = state
                    .evaluate_drawables_including_hidden()
                    .map_err(animation_error)?;
                zipzi_sdk::query_spatial_frame(
                    source.snapshot.document(),
                    &frame,
                    &self.queries,
                    control,
                )?
            };
            self.frames.push(PreviewFrame {
                observation: None,
                spatial,
                preview_id: self.preview_id,
                sequence: self.sequence,
                index: self.frames.len(),
                motion: state.snapshot().clone(),
                geometry: if self.geometry {
                    Some(state.evaluate_drawables().map_err(animation_error)?)
                } else {
                    None
                },
            });
            #[cfg(feature = "gpu-observation")]
            if self.view.is_some() {
                crate::observation::check(cancel)?;
                self.inputs.push(
                    zipzi_sdk_observe::ObservationInput::capture_motion_from_snapshot(
                        &source.snapshot,
                        &state,
                        true,
                    )
                    .map_err(crate::observation::error)?,
                );
            }
            if !self.times.is_empty() || self.view.is_some() {
                return Ok(None);
            }
        }
        #[cfg(feature = "gpu-observation")]
        if self.times.is_empty() && self.view.is_some() {
            if !self.render_step(control)? {
                return Ok(None);
            }
        }
        Ok(Some(JobValue::Previewed(Arc::new(PreviewSampleBatch {
            source: source.source,
            samples: PreviewBatch {
                view: self.view,
                sequence: self.sequence,
                frames: std::mem::take(&mut self.frames),
                queries: std::mem::take(&mut self.queries),
            },
        }))))
    }
    #[cfg(feature = "gpu-observation")]
    fn render_step(&mut self, control: &zipzi_sdk::ConversionControl) -> Result<bool> {
        let view = self.view.expect("observation view");
        if self.captures.is_none() {
            self.captures = Some(
                zipzi_sdk_observe::ResolvedObservation::capture_many_with_control(
                    std::mem::take(&mut self.inputs),
                    control,
                )
                .map_err(crate::observation::error)?
                .into(),
            );
            return Ok(false);
        }
        let captures = self.captures.as_mut().unwrap();
        if let Some(capture) = captures.pop_front() {
            let index = self.frames.len() - captures.len() - 1;
            self.frames[index].observation = Some(crate::observation::render(
                self.observer.as_mut().unwrap(),
                &capture,
                view,
            )?);
        }
        if !captures.is_empty() {
            return Ok(false);
        }
        Ok(true)
    }
}
impl Runtime {
    pub fn preview_count(&self) -> usize {
        self.previews.len()
    }
    pub(super) fn preview_bytes(&self) -> usize {
        self.previews
            .values()
            .fold(0usize, |n, p| n.saturating_add(p.bytes))
    }
    fn preview(&self, client: ClientId, id: PreviewId) -> Result<&PreviewEntry> {
        self.client(client)?;
        let entry = self
            .previews
            .get(&id)
            .ok_or(RuntimeError::InvalidReference)?;
        if entry.client != client {
            return Err(RuntimeError::WrongOwner);
        }
        if entry.busy.is_some() {
            return Err(RuntimeError::Busy);
        }
        Ok(entry)
    }
    pub fn create_preview(
        &mut self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        expected: Option<SourceRef>,
    ) -> Result<(PreviewId, SourceRef)> {
        self.client(client)?;
        if self
            .previews
            .values()
            .filter(|p| p.client == client)
            .count()
            >= self.limits.previews_per_client
        {
            return Err(RuntimeError::Budget("previews"));
        }
        let (document, bytes) =
            self.with_authoring_source(client, session, edit, |doc, source| {
                if expected.is_some_and(|s| s != source) {
                    return Err(RuntimeError::InvalidReference);
                }
                Ok((
                    doc.estimated_content_bytes(),
                    Self::sample_bytes(doc, 1)
                        .saturating_mul(3)
                        .saturating_add(self.limits.preview_seek_cache_bytes.saturating_mul(2))
                        .saturating_add(self.limits.preview_schedule_entries.saturating_mul(512)),
                ))
            })?;
        if self.preview_bytes().saturating_add(bytes) > self.limits.preview_bytes {
            return Err(RuntimeError::Budget("preview bytes"));
        }
        self.check_bytes(bytes.saturating_add(document), 0, document)?;
        let source = if let Some(edit) = edit {
            self.edit_snapshot(client, edit)?
        } else {
            self.source_snapshot(session)?
        };
        let mut state = zipzi_sdk::MotionPreview::new(source.snapshot.document());
        state.bind_session_identity(
            source.source.version().session_id,
            source.source.version().generation,
        );
        state.set_seek_cache_budget(self.limits.preview_seek_cache_bytes);
        let id = PreviewId::from_parts(self.instance, self.serial());
        let identity = source.source;
        self.previews.insert(
            id,
            PreviewEntry {
                client,
                source,
                state: Arc::new(Mutex::new(state)),
                sequence: 0,
                busy: None,
                bytes,
                motion_output_bytes: 0,
                schedule_entries: 0,
            },
        );
        self.refresh_memory_budget();
        Ok((id, identity))
    }
    pub fn preview_state(
        &self,
        client: ClientId,
        id: PreviewId,
    ) -> Result<(SourceRef, PreviewState)> {
        let entry = self.preview(client, id)?;
        let state = entry
            .state
            .try_lock()
            .map_err(|_| RuntimeError::WorkerPanic("preview state unavailable".into()))?;
        Ok((
            entry.source.source,
            PreviewState {
                sequence: entry.sequence,
                time: state.snapshot().time,
            },
        ))
    }
    pub fn apply_preview(
        &mut self,
        client: ClientId,
        id: PreviewId,
        command: PreviewCommand,
    ) -> Result<(SourceRef, PreviewState)> {
        let entry = self.preview(client, id)?;
        let scheduled = matches!(
            command,
            PreviewCommand::ScheduleMotion { .. }
                | PreviewCommand::ScheduleRegistration { .. }
                | PreviewCommand::ScheduleExpression { .. }
                | PreviewCommand::ScheduleParameter { .. }
        );
        if scheduled && entry.schedule_entries >= self.limits.preview_schedule_entries {
            return Err(RuntimeError::Budget("preview schedule"));
        }
        if serialized_bytes(&command)? > 16384 {
            return Err(RuntimeError::Budget("preview command"));
        }
        let next = entry
            .sequence
            .checked_add(1)
            .ok_or(RuntimeError::Budget("preview sequence"))?;
        {
            let mut state = entry
                .state
                .try_lock()
                .map_err(|_| RuntimeError::WorkerPanic("preview state unavailable".into()))?;
            match &command {
                PreviewCommand::SetParameter { id, value } => state
                    .set_base_parameter(id, *value)
                    .map_err(animation_error)?,
                PreviewCommand::ScheduleMotion { id, time } => {
                    state.schedule_motion(id, *time).map_err(animation_error)?
                }
                PreviewCommand::ScheduleRegistration { id, time } => state
                    .schedule_motion_registration(id, *time)
                    .map_err(animation_error)?,
                PreviewCommand::ScheduleExpression { id, time } => state
                    .schedule_expression(id, *time)
                    .map_err(animation_error)?,
                PreviewCommand::ScheduleParameter { id, time, value } => state
                    .schedule_parameter_input(id, *time, *value)
                    .map_err(animation_error)?,
                PreviewCommand::SetPhysics {
                    enabled,
                    stabilized_start,
                } => {
                    state.set_physics_enabled(*enabled);
                    state.set_physics_stabilized_start(*stabilized_start);
                }
                PreviewCommand::Reset => state.reset(),
            }
        }
        let entry = self.previews.get_mut(&id).unwrap();
        entry.sequence = next;
        if scheduled {
            entry.schedule_entries += 1;
        }
        // SDK reset preserves the activation schedules; it only resets the clock/state.
        self.preview_state(client, id)
    }
    pub fn request_preview_samples(
        &mut self,
        client: ClientId,
        id: PreviewId,
        times: Vec<f32>,
        geometry: bool,
        incremental: bool,
    ) -> Result<JobId> {
        self.request_preview_samples_with_queries(client, id, times, geometry, incremental, vec![])
    }
    pub fn request_preview_samples_with_queries(
        &mut self,
        client: ClientId,
        id: PreviewId,
        times: Vec<f32>,
        geometry: bool,
        incremental: bool,
        queries: Vec<zipzi_sdk::SpatialQuery>,
    ) -> Result<JobId> {
        self.request_preview_frames(client, id, times, geometry, incremental, queries, None)
    }
    pub fn request_preview_frames(
        &mut self,
        client: ClientId,
        id: PreviewId,
        times: Vec<f32>,
        geometry: bool,
        incremental: bool,
        queries: Vec<zipzi_sdk::SpatialQuery>,
        view: Option<crate::observation::ObservationView>,
    ) -> Result<JobId> {
        self.check_job_slot(client)?;
        if times.is_empty()
            || times.len() > 64
            || times
                .iter()
                .any(|t| !t.is_finite() || *t < 0.0 || *t > 1_000_000.0 / 60.0)
        {
            return Err(RuntimeError::Budget("preview sample targets"));
        }
        let entry = self.preview(client, id)?;
        let sequence = entry
            .sequence
            .checked_add(1)
            .ok_or(RuntimeError::Budget("preview sequence"))?;
        let motion_output = entry
            .state
            .try_lock()
            .map_err(|_| RuntimeError::WorkerPanic("preview state unavailable".into()))?
            .canonical_motion_output_bytes();
        // Keep the live snapshot and fractional checkpoint charged after the
        // Job is released. A failed/cancelled replay may retain completed state.
        let retained_output = motion_output
            .saturating_mul(2)
            .max(entry.motion_output_bytes);
        let output_growth = retained_output.saturating_sub(entry.motion_output_bytes);
        if self.preview_bytes().saturating_add(output_growth) > self.limits.preview_bytes {
            return Err(RuntimeError::Budget("preview motion output bytes"));
        }
        let bytes = Self::spatial_bytes(entry.source.snapshot.document(), times.len(), &queries)?
            .saturating_add(if let Some(view) = view {
                Self::observation_bytes(entry.source.snapshot.document(), times.len(), view)?
            } else {
                0
            })
            // Output, transactional replay, observation captures and digest
            // serialization can all own copies of the same Motion output strings.
            .saturating_add(
                motion_output
                    .saturating_mul(times.len().saturating_add(1))
                    .saturating_mul(4),
            )
            .saturating_add(
                entry
                    .schedule_entries
                    .saturating_mul(1024)
                    .saturating_mul(times.len()),
            );
        let document = entry.source.snapshot.document().estimated_content_bytes();
        self.check_bytes(
            bytes.saturating_add(document).saturating_add(output_growth),
            0,
            document,
        )?;
        let source = entry.source.clone();
        let task = ReadTask::Preview {
            preview_id: id.serial(),
            source: source.clone(),
            sequence,
            state: entry.state.clone(),
            times,
            geometry,
            incremental,
            queries,
            view,
        };
        let job = self.queue_read(client, source, task, bytes, false)?;
        let entry = self.previews.get_mut(&id).unwrap();
        entry.bytes = entry.bytes.saturating_add(output_growth);
        entry.motion_output_bytes = retained_output;
        entry.sequence = sequence;
        entry.busy = Some(job);
        self.refresh_memory_budget();
        Ok(job)
    }
    pub fn release_preview(&mut self, client: ClientId, id: PreviewId) -> Result<()> {
        self.client(client)?;
        if let Some(entry) = self.previews.get(&id) {
            if entry.client != client {
                return Err(RuntimeError::WrongOwner);
            }
            if let Some(job) = entry.busy {
                self.cancel_job(client, job)?;
            }
        }
        self.previews.remove(&id);
        self.refresh_memory_budget();
        Ok(())
    }
    pub(super) fn complete_preview_job(&mut self, job: JobId) {
        for entry in self.previews.values_mut() {
            if entry.busy == Some(job) {
                entry.busy = None;
            }
        }
    }
    pub fn preview_response(
        &mut self,
        client: ClientId,
        value: &PreviewSampleBatch,
    ) -> Result<serde_json::Value> {
        use zipzi_core::binary_wire::WireWrite;
        let (blob, length) = self.binary_result_blob(client, &value.samples)?;
        Ok(
            serde_json::json!({"source":value.source,"blob":blob,"length":length,"signature":format!("{:032x}",PreviewBatch::SIGNATURE)}),
        )
    }
}
