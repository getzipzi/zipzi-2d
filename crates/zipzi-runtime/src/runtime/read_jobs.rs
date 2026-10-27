//! Frozen CPU work never moves the formal AuthoringSession out of the owner.
use super::*;
use crate::{
    authoring::{ModelingSampleQuery, PlanSnapshot},
    jobs::{ReadTask, Task},
};
impl Runtime {
    pub(super) fn queue_read(
        &mut self,
        client: ClientId,
        source: SourceSnapshot,
        task: ReadTask,
        bytes: usize,
        plan_result: bool,
    ) -> Result<JobId> {
        self.check_job_slot(client)?;
        let session = source.source.session();
        let base = source.source.version();
        let id = JobId::from_parts(self.instance, self.serial());
        let cancel = Arc::new(AtomicBool::new(false));
        let gpu = matches!(
            &task,
            ReadTask::Observe { .. } | ReadTask::Preview { view: Some(_), .. }
        );
        self.jobs.insert(
            id,
            JobEntry {
                info: JobInfo {
                    id,
                    client,
                    session,
                    state: JobState::Queued,
                    result: None,
                },
                base,
                cancel: cancel.clone(),
                target: None,
                candidate_bytes: 0,
                publication_bytes: 0,
                read_source: Some(source),
                read_bytes: bytes,
                read_only: true,
                plan_result,
                gpu,
                gpu_resident: false,
            },
        );
        self.queue.push_back(Work {
            job: id,
            client,
            session_id: session,
            base,
            session: None,
            transfer: None,
            task: Task::Read(task),
            cancel,
        });
        Ok(id)
    }
    pub fn request_modeling_samples(
        &mut self,
        client: ClientId,
        input: ModelingSampleQuery,
    ) -> Result<JobId> {
        self.client(client)?;
        self.check_job_slot(client)?;
        if input.parameters.len() > 64 || input.parameters.iter().any(|p| p.len() > 256) {
            return Err(RuntimeError::Budget("modeling samples"));
        }
        let (document, bytes) =
            self.with_authoring_source(client, input.session, input.edit, |doc, source| {
                if input.expected.is_some_and(|s| s != source) {
                    return Err(RuntimeError::InvalidReference);
                }
                Ok((
                    doc.estimated_content_bytes(),
                    Self::sample_bytes(doc, input.parameters.len()),
                ))
            })?;
        self.check_bytes(bytes.saturating_add(document), 0, document)?;
        let source = if let Some(edit) = input.edit {
            self.edit_snapshot(client, edit)?
        } else {
            self.source_snapshot(input.session)?
        };
        let task = ReadTask::Sample {
            source: source.clone(),
            parameters: input.parameters,
            checks: input.checks,
        };
        self.queue_read(client, source, task, bytes, false)
    }
    pub fn request_modeling_plan(
        &mut self,
        client: ClientId,
        session: SessionId,
        edit: Option<EditId>,
        intent: zipzi_sdk::ModelingIntent,
    ) -> Result<JobId> {
        self.client(client)?;
        self.check_job_slot(client)?;
        let pending = self
            .jobs
            .values()
            .filter(|j| j.info.client == client && j.plan_result && !j.info.state.is_terminal())
            .count();
        if self.plans.values().filter(|p| p.client == client).count() + pending
            >= self.limits.plans_per_client
        {
            return Err(RuntimeError::Budget("plans"));
        }
        let document = self.with_authoring_source(client, session, edit, |d, _| {
            Ok(d.estimated_content_bytes())
        })?;
        let base = document
            .saturating_mul(3)
            .saturating_add(serialized_bytes(&intent)?.saturating_mul(4));
        let remaining = self
            .limits
            .plan_bytes
            .saturating_sub(self.plans.values().map(|p| p.bytes).sum::<usize>())
            .saturating_sub(
                self.jobs
                    .values()
                    .filter(|j| j.plan_result && !j.info.state.is_terminal())
                    .map(|j| j.read_bytes)
                    .sum::<usize>(),
            );
        if base > remaining {
            return Err(RuntimeError::Budget("plan bytes"));
        }
        let mut limits = self.ready(session)?.authoring_limits();
        limits.max_conversion_bytes = limits.max_conversion_bytes.min((remaining - base) / 2);
        let bytes = base.saturating_add(limits.max_conversion_bytes.saturating_mul(2));
        self.check_bytes(bytes.saturating_add(document), 0, document)?;
        let source = if let Some(edit) = edit {
            self.edit_snapshot(client, edit)?
        } else {
            self.source_snapshot(session)?
        };
        let task = ReadTask::Plan {
            source: source.clone(),
            intent,
            limits,
            bytes: base,
        };
        self.queue_read(client, source, task, bytes, true)
    }
    pub(super) fn complete_read(
        &mut self,
        job: JobId,
        client: ClientId,
        session: SessionId,
        base: Version,
        mut result: Result<JobValue>,
    ) -> bool {
        let Some(record) = self.jobs.get(&job) else {
            return false;
        };
        if job.runtime_instance() != self.instance
            || record.info.client != client
            || record.info.session != session
            || record.base != base
            || record.info.state.is_terminal()
        {
            return false;
        }
        self.jobs.get_mut(&job).unwrap().gpu_resident = false;
        if !self.clients.contains(&client) {
            self.jobs.remove(&job);
            return true;
        }
        self.complete_preview_job(job);
        let record = self.jobs.get(&job).unwrap();
        // Preview steps commit in the SDK worker. A cancellation that arrives
        // after the complete batch succeeded must report that actual outcome.
        if record.cancel.load(Ordering::Acquire) && !matches!(&result, Ok(JobValue::Previewed(_))) {
            result = Err(RuntimeError::Cancelled);
        }
        let expected = record.read_source.as_ref().map(|s| s.source);
        match &result {
            Ok(JobValue::Observed(_)) => {
                self.gpu_available = Some(true);
                self.gpu_error = None;
            }
            Ok(JobValue::Previewed(value))
                if value.samples.frames.iter().any(|f| f.observation.is_some()) =>
            {
                self.gpu_available = Some(true);
                self.gpu_error = None;
            }
            Err(RuntimeError::Sdk(e))
                if matches!(e.code.as_ref(), "GPU_ADAPTER" | "GPU_DEVICE") =>
            {
                self.gpu_available = Some(false);
                self.gpu_error = Some(e.code.to_string());
            }
            _ => {}
        }
        if let Ok(value) = &result {
            let actual = match value {
                JobValue::Sampled(value) => Some(value.source),
                JobValue::Spatial(value) => Some(value.source),
                JobValue::Observed(value) => Some(value.source),
                JobValue::Previewed(value) => Some(value.source),
                JobValue::PreparedPlan(value) => Some(value.source.source),
                _ => None,
            };
            if actual != expected {
                result = Err(RuntimeError::InvalidReference);
            }
        }
        if let Ok(JobValue::PreparedPlan(prepared)) = &result {
            let prepared = prepared.clone();
            let report = prepared.plan.report();
            let bytes = prepared.bytes.saturating_add(
                report
                    .cost
                    .map_or(0, |c| c.payload_bytes.saturating_add(c.temporary_bytes)),
            );
            let retained = self.plans.values().map(|p| p.bytes).sum::<usize>();
            if retained.saturating_add(bytes) > self.limits.plan_bytes
                || self.plans.values().filter(|p| p.client == client).count()
                    >= self.limits.plans_per_client
            {
                result = Err(RuntimeError::Budget("completed plan"));
            } else {
                let id = PlanId::from_parts(self.instance, self.serial());
                self.plans.insert(
                    id,
                    modeling::PlanEntry {
                        client,
                        source: prepared.source.clone(),
                        plan: prepared.plan.clone(),
                        bytes,
                    },
                );
                result = Ok(JobValue::Planned(Arc::new(PlanSnapshot {
                    plan: id,
                    source: prepared.source.source,
                    report,
                })));
            }
        }
        if let Ok(JobValue::PreparedImage(image)) = &result {
            let image = image.as_ref().clone();
            if self
                .prepared_images
                .values()
                .filter(|i| i.client == client)
                .count()
                >= self.limits.prepared_images_per_client
            {
                result = Err(RuntimeError::Budget("prepared images"));
            } else {
                let id = ImageId::from_parts(self.instance, self.serial());
                let value = crate::authoring::PreparedImageInfo {
                    image: id,
                    width: image.content().width,
                    height: image.content().height,
                };
                self.prepared_images
                    .insert(id, resources::ImageEntry { client, image });
                result = Ok(JobValue::ImagePrepared(value));
            }
        }
        let record = self.jobs.get_mut(&job).unwrap();
        if result.is_err() || matches!(&result, Ok(JobValue::Planned(_))) {
            record.read_source = None;
            record.read_bytes = 512;
        }
        record.info.state = match &result {
            Ok(_) => JobState::Succeeded,
            Err(RuntimeError::Cancelled) => JobState::Cancelled,
            Err(_) => JobState::Failed,
        };
        record.read_bytes = record.read_bytes.max(
            result
                .as_ref()
                .map_or_else(|e| e.retained_bytes(), JobValue::retained_bytes),
        );
        record.info.result = Some(result);
        self.refresh_memory_budget();
        true
    }
}
