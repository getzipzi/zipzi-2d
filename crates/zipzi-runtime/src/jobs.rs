//! Ownership-carrying jobs. Executors schedule Work and deliver Completion to the
//! application host even after the initiating view has disappeared.
use crate::{ClientId, JobId, JobValue, Result, RuntimeError, SessionId};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use zipzi_sdk::{AuthoringSession, EditSession, Version};
mod batches;

pub trait Executor {
    /// Rejection returns the exact unexecuted work for a later dispatch attempt.
    fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>>;
}

pub(crate) enum Task {
    Read(ReadTask),
    Project {
        request: crate::project_io::ProjectRequest,
        _lease: Arc<zipzi_project::TransferReservation>,
    },
    Open(PathBuf),
    Import(crate::ImportSource),
    Save(PathBuf),
    Commit(Box<EditSession<'static>>),
    Undo,
    Redo,
}

pub struct Work {
    pub(crate) transfer: Option<Arc<zipzi_project::TransferReservation>>,
    pub(crate) job: JobId,
    pub(crate) client: ClientId,
    pub(crate) session_id: SessionId,
    pub(crate) base: Version,
    pub(crate) session: Option<AuthoringSession>,
    pub(crate) task: Task,
    pub(crate) cancel: Arc<AtomicBool>,
}
impl Work {
    /// Run a decoder on a host-selected stack. Thread creation failure returns
    /// the untouched formal session; an unexpected outer panic returns Faulted.
    pub fn run_on_thread(self, name: &str, stack_bytes: usize) -> Completion {
        let (job, client, session_id, base) = (self.job, self.client, self.session_id, self.base);
        let pending = Arc::new(std::sync::Mutex::new(Some(self)));
        let owned = pending.clone();
        match std::thread::Builder::new()
            .name(name.into())
            .stack_size(stack_bytes)
            .spawn(move || owned.lock().unwrap().take().unwrap().run_quantum())
        {
            Ok(worker) => worker.join().unwrap_or_else(|_| Completion {
                job,
                client,
                session_id,
                base,
                session: None,
                transfer: None,
                outcome: WorkOutcome::Complete(Err(RuntimeError::WorkerPanic(
                    "worker panicked outside the SDK operation".into(),
                ))),
            }),
            Err(error) => {
                let work = pending.lock().unwrap().take().unwrap();
                Completion {
                    job,
                    client,
                    session_id,
                    base,
                    session: work.session,
                    transfer: work.transfer,
                    outcome: WorkOutcome::Complete(Err(RuntimeError::ExecutorFailure(
                        error.to_string(),
                    ))),
                }
            }
        }
    }
    pub fn job_id(&self) -> JobId {
        self.job
    }
    pub fn session_id(&self) -> SessionId {
        self.session_id
    }
    /// Always emits a terminal completion for unwind panics. Process aborts
    /// cannot be recovered. SDK errors return the same session and history.
    pub fn run(self) -> Completion {
        self.run_with_hook(|| {})
    }
    /// Execute one batch sample, texture acquisition, or GPU frame and return
    /// ownership to Runtime. Hosts must publish every completion and dispatch
    /// again, including completions for jobs that have not reached a terminal state.
    pub fn run_quantum(self) -> Completion {
        self.run_quantum_with_hook(|| {})
    }
    pub(crate) fn run_quantum_with_hook(self, before: impl FnOnce()) -> Completion {
        let Self {
            job,
            client,
            session_id,
            base,
            session,
            transfer,
            task,
            cancel,
        } = self;
        let task = match task {
            Task::Read(task) => match batches::ReadBatch::from_task(task) {
                Ok(batch) => batch,
                Err(task) => {
                    return Self {
                        job,
                        client,
                        session_id,
                        base,
                        session,
                        transfer,
                        task: Task::Read(task),
                        cancel,
                    }
                    .run_with_hook(before);
                }
            },
            task => {
                return Self {
                    job,
                    client,
                    session_id,
                    base,
                    session,
                    transfer,
                    task,
                    cancel,
                }
                .run_with_hook(before);
            }
        };
        let mut batch = task;
        let control = zipzi_sdk::ConversionControl::from_cancel_flag(cancel.clone());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::observation::check(&cancel)?;
            before();
            batch.step(&cancel, &control)
        }))
        .unwrap_or_else(|_| Err(RuntimeError::WorkerPanic("read worker panicked".into())));
        let outcome = match result {
            Ok(None) => WorkOutcome::Yielded(Box::new(Self {
                job,
                client,
                session_id,
                base,
                session: None,
                transfer: None,
                task: Task::Read(ReadTask::Batch(batch)),
                cancel,
            })),
            result => WorkOutcome::Complete(result.map(Option::unwrap)),
        };
        Completion {
            job,
            client,
            session_id,
            base,
            session: None,
            transfer: None,
            outcome,
        }
    }
    #[cfg(feature = "test-support")]
    pub(crate) fn executor_failure(self, message: String) -> Completion {
        Completion {
            job: self.job,
            client: self.client,
            session_id: self.session_id,
            base: self.base,
            session: self.session,
            transfer: self.transfer,
            outcome: WorkOutcome::Complete(Err(RuntimeError::ExecutorFailure(message))),
        }
    }
    pub(crate) fn run_with_hook(self, before_io: impl FnOnce()) -> Completion {
        if matches!(&self.task, Task::Read(_)) {
            return self.run_read(before_io);
        }
        let Self {
            job,
            client,
            session_id,
            base,
            session,
            task,
            cancel,
            transfer,
        } = self;
        let mut session = session.expect("ownership job has its session");
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if cancel.load(Ordering::Acquire) {
                return Err(RuntimeError::Cancelled);
            }
            before_io();
            // Cancellation after this check is advisory. A successful SDK
            // publication/save is reported as success, never a false rollback.
            match task {
                Task::Read(_) => unreachable!("read job dispatched separately"),
                Task::Project { request, _lease } => {
                    let _lease = _lease;
                    return crate::project_io::execute(&mut session, request, &cancel).map(|r| {
                        JobValue::Project {
                            session: session_id,
                            report: Arc::new(r),
                        }
                    });
                }
                Task::Import(source) => {
                    let (path, result) = match source {
                        crate::ImportSource::Model3(path) => {
                            let report = session.import_model3(&path, Some(base));
                            (path, report.map(|r| r.project))
                        }
                        crate::ImportSource::Psd(path) => {
                            let report = session.import_psd_in_memory(&path, Some(base));
                            (path, report.map(|r| r.project))
                        }
                        crate::ImportSource::Cubism(path) => {
                            let report = session.import_cubism(&path, Some(base));
                            (path, report.map(|_| zipzi_sdk::ProjectResult::ok()))
                        }
                    };
                    result.map(|report| {
                        let mut notices = report.warnings;
                        notices.extend(
                            report
                                .diagnostics
                                .into_iter()
                                .map(|d| format!("{}: {}", d.code, d.message)),
                        );
                        let name = path
                            .file_stem()
                            .map(|s| s.to_string_lossy().trim_end_matches(".model3").to_owned());
                        JobValue::Imported {
                            session: session_id,
                            notices,
                            name,
                        }
                    })
                }
                Task::Open(path) => session
                    .open_project(&path, Some(base))
                    .map(|_| JobValue::Opened(session_id)),
                Task::Save(path) => session.save_project(&path, Some(base)).map(JobValue::Saved),
                Task::Commit(edit) => (*edit)
                    .commit_to(&mut session)
                    .map(|receipt| JobValue::Published(Arc::new(receipt))),
                Task::Undo => session
                    .undo()
                    .map(|receipt| JobValue::Published(Arc::new(receipt))),
                Task::Redo => session
                    .redo()
                    .map(|receipt| JobValue::Published(Arc::new(receipt))),
            }
            .map_err(RuntimeError::from)
        }));
        match outcome {
            Ok(result) => Completion {
                job,
                client,
                session_id,
                base,
                session: Some(session),
                transfer,
                outcome: WorkOutcome::Complete(result),
            },
            Err(panic) => {
                let message = panic
                    .downcast_ref::<&str>()
                    .map(|s| (*s).to_owned())
                    .or_else(|| panic.downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "non-string panic".into());
                // Consistency after a worker panic is unknown. Never return
                // the potentially half-mutated authoring state to Ready.
                Completion {
                    job,
                    client,
                    session_id,
                    base,
                    session: None,
                    transfer,
                    outcome: WorkOutcome::Complete(Err(RuntimeError::WorkerPanic(message))),
                }
            }
        }
    }
}

/// Fields are private to prevent an executor from constructing, cloning or
/// retargeting a completion. Runtime also validates the active job and version.
pub struct Completion {
    pub(crate) transfer: Option<Arc<zipzi_project::TransferReservation>>,
    pub(crate) job: JobId,
    pub(crate) client: ClientId,
    pub(crate) session_id: SessionId,
    pub(crate) base: Version,
    pub(crate) session: Option<AuthoringSession>,
    pub(crate) outcome: WorkOutcome,
}
pub(crate) enum WorkOutcome {
    Complete(Result<JobValue>),
    Yielded(Box<Work>),
}
impl Completion {
    /// False when the Job yielded and needs another dispatch, without a public
    /// result or event. Hosts can avoid waking views for these internal steps.
    pub fn is_terminal(&self) -> bool {
        matches!(self.outcome, WorkOutcome::Complete(_))
    }
    /// Read-only preparation while an executor still owns the returned state.
    /// Hosts must publish this completion before attaching any resulting view.
    pub fn returned_session(&self) -> Option<&AuthoringSession> {
        let WorkOutcome::Complete(Ok(_)) = &self.outcome else {
            return None;
        };
        self.session.as_ref()
    }

    pub fn job_id(&self) -> JobId {
        self.job
    }
}

pub(crate) enum ReadTask {
    Batch(Box<batches::ReadBatch>),
    Observe {
        source: crate::SourceSnapshot,
        parameters: Vec<zipzi_core::PreviewValues>,
        view: crate::observation::ObservationView,
    },
    Spatial {
        source: crate::SourceSnapshot,
        parameters: Vec<zipzi_core::PreviewValues>,
        queries: Vec<zipzi_sdk::SpatialQuery>,
    },
    Preview {
        preview_id: u64,
        source: crate::SourceSnapshot,
        sequence: u64,
        state: Arc<std::sync::Mutex<zipzi_sdk::MotionPreview>>,
        times: Vec<f32>,
        geometry: bool,
        incremental: bool,
        queries: Vec<zipzi_sdk::SpatialQuery>,
        view: Option<crate::observation::ObservationView>,
    },
    Image {
        path: Option<PathBuf>,
        bytes: Option<Arc<[u8]>>,
        budget: zipzi_project::ImageMemoryBudget,
    },
    Sample {
        source: crate::SourceSnapshot,
        parameters: Vec<zipzi_core::PreviewValues>,
        checks: zipzi_sdk::GeometryChecks,
    },
    Plan {
        source: crate::SourceSnapshot,
        intent: zipzi_sdk::ModelingIntent,
        limits: zipzi_sdk::AuthoringLimits,
        bytes: usize,
    },
}
#[derive(Debug)]
pub struct PreparedModelingPlan {
    pub(crate) source: crate::SourceSnapshot,
    pub(crate) plan: Arc<zipzi_sdk::ModelingPlan>,
    pub(crate) bytes: usize,
}
impl Work {
    fn run_read(self, before: impl FnOnce()) -> Completion {
        let Self {
            job,
            client,
            session_id,
            base,
            task,
            cancel,
            ..
        } = self;
        let Task::Read(task) = task else {
            unreachable!()
        };
        let control = zipzi_sdk::ConversionControl::from_cancel_flag(cancel.clone());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if cancel.load(Ordering::Acquire) {
                return Err(RuntimeError::Cancelled);
            }
            before();
            let task = match batches::ReadBatch::from_task(task) {
                Ok(mut batch) => {
                    return loop {
                        if let Some(value) = batch.step(&cancel, &control)? {
                            break Ok(value);
                        }
                    };
                }
                Err(task) => task,
            };
            let value = match task {
                ReadTask::Image {
                    path,
                    bytes,
                    budget,
                } => {
                    let image = if let Some(path) = path {
                        zipzi_project::prepare_image_content_with_budget(&path, &budget)
                    } else {
                        zipzi_project::prepare_png_content(
                            bytes.ok_or(RuntimeError::InvalidReference)?,
                            &budget,
                        )
                    }
                    .map_err(crate::runtime::resources::preparation_error)?;
                    JobValue::PreparedImage(Arc::new(image))
                }
                ReadTask::Plan {
                    source,
                    intent,
                    limits,
                    bytes,
                } => {
                    let plan = source.snapshot.plan_modeling(intent, limits, &control)?;
                    JobValue::PreparedPlan(Arc::new(PreparedModelingPlan {
                        source,
                        plan: Arc::new(plan),
                        bytes,
                    }))
                }
                _ => unreachable!("batch tasks handled above"),
            };
            if cancel.load(Ordering::Acquire) && !matches!(value, JobValue::Previewed(_)) {
                Err(RuntimeError::Cancelled)
            } else {
                Ok(value)
            }
        }))
        .unwrap_or_else(|_| Err(RuntimeError::WorkerPanic("read worker panicked".into())));
        Completion {
            job,
            client,
            session_id,
            base,
            session: None,
            transfer: None,
            outcome: WorkOutcome::Complete(result),
        }
    }
}
