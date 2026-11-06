//! Runtime admission/completion stays on the UI thread; workers own only Work.
//! All read workloads use an exact published source.
use anyhow::{Context as _, ensure};
use std::{
    sync::mpsc,
    thread::JoinHandle,
    time::{Duration, Instant},
};
use zipzi_contract::{
    kernel_tasks::{SampleInput, SampleRequest},
    observation::ObservationView,
};
use zipzi_editor_core::RuntimeHost;
use zipzi_runtime::{ClientId, Completion, Executor, JobId, JobState, Work};
use zipzi_sdk::{QuerySource, SessionKey};

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
    session: SessionKey,
    client: ClientId,
    kind: Kind,
    motion: Option<String>,
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
        session: SessionKey,
        kind: Kind,
    ) -> anyhow::Result<Self> {
        let client = runtime.write().register_client()?;
        let mut result = Self {
            runtime,
            session,
            client,
            kind,
            motion: None,
            job: None,
            worker: None,
            stopped: false,
            jobs: 0,
            steps: 0,
            warmup_steps: 0,
        };
        result.runtime.write().attach(client, session)?;
        if matches!(kind, Kind::Preview) {
            result.motion = result
                .runtime
                .read()
                .read_snapshot(client, session)?
                .snapshot
                .document()
                .motion_order()
                .first()
                .cloned();
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
        let source =
            QuerySource::Published(runtime.session_read(self.client, self.session)?.version());
        let input = match self.kind {
            Kind::Cpu | Kind::Observe => SampleInput::Parameters {
                values: vec![Default::default(); 4],
            },
            Kind::Preview => SampleInput::Animation {
                times: vec![0., 0.125, 0.25, 0.375],
                motions: self.motion.iter().map(|id| (id.clone(), 0.)).collect(),
                expressions: Vec::new(),
                inputs: Vec::new(),
                physics: true,
                stabilized_start: false,
            },
        };
        match self.kind {
            Kind::Cpu | Kind::Preview => runtime.request_samples(
                self.client,
                source,
                SampleRequest {
                    input,
                    queries: Vec::new(),
                    quality: false,
                },
            ),
            Kind::Observe => runtime.request_observation(
                self.client,
                source,
                input,
                ObservationView {
                    texture_policy: Default::default(),
                    width: 96,
                    height: 96,
                    roi: [-4., -4., 4., 4.],
                    padding_model: 0.,
                    topology: None,
                    quality: false,
                },
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
                        self.runtime.write().complete(completion).is_ok(),
                        "background completion rejected"
                    );
                    self.steps += 1;
                }
                Err(mpsc::TryRecvError::Empty) => return Ok(()),
                Err(error) => anyhow::bail!("background worker disconnected: {error}"),
            }
        }
        if let Some(job) = self.job {
            let info = self.runtime.read().job(self.client, job)?.clone();
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
                        .job(self.client, self.job.unwrap())?
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
                        // Cooperative read jobs return one quantum per dispatch.
                        let _ = sender.send(work.run_with_stack("zipzi-bench-read", 16 << 20));
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
