//! Runtime admission/completion stays on the UI thread; workers own only Work.
//! Keep this driver identical in both builds of a paired comparison.
use anyhow::{Context as _, ensure};
use std::{
    sync::mpsc,
    thread::JoinHandle,
    time::{Duration, Instant},
};
use zipzi_editor_core::RuntimeHost;
use zipzi_runtime::{ClientId, Completion, Executor, JobId, JobState, PreviewId, SessionId, Work};

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Cpu,
    Preview,
    Observe,
}
impl Kind {
    pub(super) fn name(self) -> &'static str {
        match self {
            Self::Cpu => "other_session_cpu_sweep",
            Self::Preview => "other_session_preview",
            Self::Observe => "other_session_gpu_observation",
        }
    }
}
#[derive(Default)]
struct Scheduled(Vec<Work>);
impl Executor for Scheduled {
    fn submit(&mut self, work: Work) -> Result<(), Box<Work>> {
        self.0.push(work);
        Ok(())
    }
}
pub(super) struct Background {
    runtime: RuntimeHost,
    session: SessionId,
    client: ClientId,
    kind: Kind,
    preview: Option<PreviewId>,
    job: Option<JobId>,
    worker: Option<(JoinHandle<()>, mpsc::Receiver<Completion>)>,
    stopped: bool,
    jobs: usize,
    steps: usize,
    warmup_steps: usize,
}
impl Background {
    pub(super) fn start(
        runtime: RuntimeHost,
        session: SessionId,
        kind: Kind,
    ) -> anyhow::Result<Self> {
        let client = runtime.write().register_client()?;
        let mut result = Self {
            runtime,
            session,
            client,
            kind,
            preview: None,
            job: None,
            worker: None,
            stopped: false,
            jobs: 0,
            steps: 0,
            warmup_steps: 0,
        };
        if matches!(kind, Kind::Preview) {
            let (preview, _) = result
                .runtime
                .write()
                .create_preview(client, session, None, None)?;
            result.preview = Some(preview);
            let clip = result
                .runtime
                .read()
                .session(session)?
                .motion_ids()
                .first()
                .cloned();
            if let Some(id) = clip {
                result.runtime.write().apply_preview(
                    client,
                    preview,
                    zipzi_runtime::preview::PreviewCommand::ScheduleMotion { id, time: 0. },
                )?;
            }
        }
        let deadline = Instant::now() + Duration::from_secs(60);
        while result.jobs == 0 {
            result.poll()?;
            ensure!(Instant::now() < deadline, "background warmup timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
        result.warmup_steps = result.steps;
        Ok(result)
    }
    fn enqueue(&self) -> zipzi_runtime::Result<JobId> {
        let mut runtime = self.runtime.write();
        match self.kind {
            Kind::Cpu => runtime.request_modeling_samples(
                self.client,
                zipzi_runtime::authoring::ModelingSampleQuery {
                    session: self.session,
                    edit: None,
                    parameters: vec![Default::default(); 4],
                    checks: Default::default(),
                    expected: None,
                },
            ),
            Kind::Preview => runtime.request_preview_samples(
                self.client,
                self.preview.unwrap(),
                vec![0., 0.125, 0.25, 0.375],
                true,
                false,
            ),
            Kind::Observe => runtime.request_observation(
                self.client,
                self.session,
                None,
                vec![Default::default(); 4],
                zipzi_runtime::observation::ObservationView {
                    width: 96,
                    height: 96,
                    roi: [-4., -4., 4., 4.],
                    padding_model: 0.,
                    topology: None,
                    quality: false,
                },
                None,
            ),
        }
    }
    pub(super) fn poll(&mut self) -> anyhow::Result<()> {
        if let Some((_, receiver)) = &self.worker {
            match receiver.try_recv() {
                Ok(completion) => {
                    self.worker
                        .take()
                        .unwrap()
                        .0
                        .join()
                        .map_err(|_| anyhow::anyhow!("background worker panicked"))?;
                    ensure!(
                        self.runtime.write().complete(completion),
                        "background completion rejected"
                    );
                    self.steps += 1;
                }
                Err(mpsc::TryRecvError::Empty) => return Ok(()),
                Err(error) => anyhow::bail!("background worker disconnected: {error}"),
            }
        }
        if let Some(job) = self.job {
            let info = self.runtime.read().job_info(self.client, job)?;
            if info.state.is_terminal() {
                self.runtime.write().release_job(self.client, job)?;
                self.job = None;
                if !(self.stopped && info.state == JobState::Cancelled) {
                    info.result.context("background result missing")??;
                    self.jobs += 1;
                }
            }
        }
        if self.job.is_none() && !self.stopped {
            self.job = Some(self.enqueue()?);
        }
        if self.job.is_some() {
            let mut scheduled = Scheduled::default();
            self.runtime.write().dispatch(&mut scheduled);
            // Cancellation may finish in dispatch without entering a worker.
            if scheduled.0.is_empty() {
                ensure!(
                    self.runtime
                        .read()
                        .job_info(self.client, self.job.unwrap())?
                        .state
                        .is_terminal(),
                    "background work could not be dispatched"
                );
            } else {
                ensure!(scheduled.0.len() == 1, "unexpected unrelated Runtime work");
                let work = scheduled.0.pop().unwrap();
                let (sender, receiver) = mpsc::channel();
                self.worker = Some((
                    std::thread::spawn(move || {
                        // Whole work in the baseline, one quantum in the candidate.
                        let _ = sender.send(work.run_on_thread("zipzi-bench-read", 16 << 20));
                    }),
                    receiver,
                ));
            }
        }
        Ok(())
    }
    fn drain(&mut self) -> anyhow::Result<()> {
        self.stopped = true;
        if let Some(job) = self.job {
            self.runtime.write().cancel_job(self.client, job)?;
        }
        let deadline = Instant::now() + Duration::from_secs(60);
        while self.job.is_some() {
            self.poll()?;
            ensure!(
                Instant::now() < deadline,
                "background cancellation timed out"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
        if let Some(preview) = self.preview.take() {
            self.runtime.write().release_preview(self.client, preview)?;
        }
        Ok(())
    }
    pub(super) fn finish(mut self) -> anyhow::Result<(usize, usize, usize)> {
        self.drain()?;
        Ok((self.jobs, self.steps, self.warmup_steps))
    }
}
impl Drop for Background {
    fn drop(&mut self) {
        let _ = self.drain();
        self.runtime.write().release_client(self.client);
    }
}
