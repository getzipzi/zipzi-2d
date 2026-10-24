//! Ownership-carrying jobs. Executors schedule Work and deliver Completion to the
//! application host even after the initiating view has disappeared.
use crate::{ClientId, JobId, JobValue, Result, RuntimeError, SessionId};
use std::path::PathBuf;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use zipzi_sdk::{AuthoringSession, EditSession, Version};

pub trait Executor {
    /// Rejection returns the exact unexecuted work for a later dispatch attempt.
    fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>>;
}

pub(crate) enum Task {
    Open(PathBuf),
    Import(crate::ImportSource),
    Save(PathBuf),
    Commit(Box<EditSession<'static>>),
    Undo,
    Redo,
}

pub struct Work {
    pub(crate) job: JobId,
    pub(crate) client: ClientId,
    pub(crate) session_id: SessionId,
    pub(crate) base: Version,
    pub(crate) session: AuthoringSession,
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
            .spawn(move || owned.lock().unwrap().take().unwrap().run())
        {
            Ok(worker) => worker.join().unwrap_or_else(|_| Completion {
                job,
                client,
                session_id,
                base,
                session: None,
                result: Err(RuntimeError::WorkerPanic(
                    "worker panicked outside the SDK operation".into(),
                )),
            }),
            Err(error) => {
                let work = pending.lock().unwrap().take().unwrap();
                Completion {
                    job,
                    client,
                    session_id,
                    base,
                    session: Some(work.session),
                    result: Err(RuntimeError::ExecutorFailure(error.to_string())),
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
        let Self {
            job,
            client,
            session_id,
            base,
            mut session,
            task,
            cancel,
        } = self;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            if cancel.load(Ordering::Acquire) {
                return Err(RuntimeError::Cancelled);
            }
            // Cancellation after this check is advisory. A successful SDK
            // publication/save is reported as success, never a false rollback.
            match task {
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
                result,
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
                    result: Err(RuntimeError::WorkerPanic(message)),
                }
            }
        }
    }
}

/// Fields are private to prevent an executor from constructing, cloning or
/// retargeting a completion. Runtime also validates the active job and version.
pub struct Completion {
    pub(crate) job: JobId,
    pub(crate) client: ClientId,
    pub(crate) session_id: SessionId,
    pub(crate) base: Version,
    pub(crate) session: Option<AuthoringSession>,
    pub(crate) result: Result<JobValue>,
}
impl Completion {
    /// Read-only preparation while an executor still owns the returned state.
    /// Hosts must publish this completion before attaching any resulting view.
    pub fn returned_session(&self) -> Option<&AuthoringSession> {
        self.result.as_ref().ok()?;
        self.session.as_ref()
    }

    pub fn job_id(&self) -> JobId {
        self.job
    }
}
