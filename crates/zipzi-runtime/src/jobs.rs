//! Worker messages contain values and resource responsibility, never a Session.
use crate::*;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use zipzi_project::{ProjectIo, TransferReservation};
use zipzi_sdk::{Commands, CompletedSave, Draft, PreparedChange, PreparedProject, PreparedSave};

#[derive(Clone, Default)]
pub struct Cancellation(Arc<AtomicBool>);
impl Cancellation {
    pub fn conversion_control(&self) -> zipzi_sdk::ConversionControl {
        zipzi_sdk::ConversionControl::from_cancel_flag(self.0.clone())
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(error("CANCELLED", "Task cancelled"))
        } else {
            Ok(())
        }
    }
    pub(crate) fn cancel(&self) {
        self.0.store(true, Ordering::Release);
    }
}
pub trait Executor {
    /// A rejected dispatch returns the exact unentered work for retry.
    fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>>;
}
pub(crate) type Batch = Box<dyn FnOnce(&mut Commands<'_>, &Cancellation) -> Result<()> + Send>;
pub(crate) type Read = Box<dyn FnOnce(SourcedSnapshot, &Cancellation) -> Result<Vec<u8>> + Send>;
/// Cooperative reads retain their frozen input and reservations between quanta.
/// Each invocation must do bounded work and check cancellation within long loops.
pub enum ReadProgress {
    Yield,
    Complete(Vec<u8>),
}
pub(crate) type ReadSteps =
    Box<dyn FnMut(&SourcedSnapshot, &Cancellation) -> Result<ReadProgress> + Send>;
pub(crate) enum Task {
    Batch {
        draft: Draft,
        run: Batch,
    },
    Publish {
        draft: Draft,
        description: String,
    },
    Read {
        snapshot: SourcedSnapshot,
        run: Read,
        max_bytes: usize,
    },
    ReadSteps {
        snapshot: SourcedSnapshot,
        run: ReadSteps,
        max_bytes: usize,
    },
    Open {
        io: ProjectIo,
        path: PathBuf,
    },
    Save(PreparedSave),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskKind {
    Batch,
    Publish,
    Read,
    Open,
    Save,
}
impl Task {
    pub(crate) fn kind(&self) -> TaskKind {
        match self {
            Self::Batch { .. } => TaskKind::Batch,
            Self::Publish { .. } => TaskKind::Publish,
            Self::Read { .. } | Self::ReadSteps { .. } => TaskKind::Read,
            Self::Open { .. } => TaskKind::Open,
            Self::Save(_) => TaskKind::Save,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Identity {
    pub job: JobId,
    pub client: ClientId,
    pub source: QuerySource,
}
impl Identity {
    pub fn base(self) -> Version {
        match self.source {
            QuerySource::Published(v) => v,
            QuerySource::Draft(d) => d.base,
        }
    }
}
pub struct Work {
    pub(crate) identity: Identity,
    pub(crate) task: Task,
    pub(crate) cancel: Cancellation,
    pub(crate) workspace: Arc<TransferReservation>,
    pub(crate) result_lease: Arc<TransferReservation>,
}
impl std::fmt::Debug for Work {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Work")
            .field("identity", &self.identity)
            .field("kind", &self.task.kind())
            .finish_non_exhaustive()
    }
}
#[derive(Debug)]
pub(crate) enum Output {
    Yield(Box<Work>),
    Draft(Draft),
    Publish(PreparedChange),
    Read(ReadResult),
    Open(PreparedProject),
    Save(CompletedSave),
}
/// Accepted work must deliver a completion even after client detachment. In
/// particular, an externally published save always needs metadata installation.
#[must_use = "Deliver completion to its Runtime, including cancelled/disconnected jobs"]
#[derive(Debug)]
pub struct Completion {
    pub(crate) identity: Identity,
    pub(crate) outcome: Result<Output>,
}
impl Completion {
    pub fn job_id(&self) -> JobId {
        self.identity.job
    }
}
impl Work {
    pub fn job_id(&self) -> JobId {
        self.identity.job
    }
    pub fn source(&self) -> QuerySource {
        self.identity.source
    }
    pub fn session_key(&self) -> SessionKey {
        self.identity.base().session
    }
    /// Converts unwinding task failures to terminal results. Dropping a candidate
    /// or decoder never affects the resident Session/history.
    pub fn run(self) -> Completion {
        let Self {
            identity,
            task,
            cancel,
            workspace,
            result_lease,
        } = self;
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cancel.check()?;
            match task {
                Task::Batch { mut draft, run } => {
                    let QuerySource::Draft(source) = identity.source else {
                        unreachable!()
                    };
                    draft.batch(source, |commands| run(commands, &cancel))?;
                    cancel.check()?;
                    Ok(Output::Draft(draft))
                }
                Task::Publish { draft, description } => {
                    if identity.source != QuerySource::Draft(draft.source()) {
                        return Err(error("STALE_SOURCE", "Commit source changed"));
                    }
                    let prepared = draft.prepare(description)?;
                    cancel.check()?;
                    Ok(Output::Publish(prepared))
                }
                Task::Read {
                    snapshot,
                    run,
                    max_bytes,
                } => {
                    let bytes = run(snapshot, &cancel)?;
                    cancel.check()?;
                    if bytes.capacity() > max_bytes {
                        return Err(error(
                            "RESOURCE_LIMIT",
                            "Read result exceeds its admitted bound",
                        ));
                    }
                    Ok(Output::Read(ReadResult::new(
                        identity.source,
                        bytes,
                        result_lease,
                    )))
                }
                Task::ReadSteps {
                    snapshot,
                    mut run,
                    max_bytes,
                } => match run(&snapshot, &cancel)? {
                    ReadProgress::Yield => {
                        cancel.check()?;
                        Ok(Output::Yield(Box::new(Work {
                            identity,
                            task: Task::ReadSteps {
                                snapshot,
                                run,
                                max_bytes,
                            },
                            cancel,
                            workspace: workspace.clone(),
                            result_lease,
                        })))
                    }
                    ReadProgress::Complete(bytes) => {
                        cancel.check()?;
                        if bytes.capacity() > max_bytes {
                            return Err(error(
                                "RESOURCE_LIMIT",
                                "Read result exceeds its admitted bound",
                            ));
                        }
                        Ok(Output::Read(ReadResult::new(
                            identity.source,
                            bytes,
                            result_lease,
                        )))
                    }
                },
                Task::Open { io, path } => {
                    PreparedProject::load(identity.base(), &io, &path, || !cancel.is_cancelled())
                        .map(Output::Open)
                }
                // No cancellation check after write: a success means externally published.
                Task::Save(prepared) => prepared.write(|| !cancel.is_cancelled()).map(Output::Save),
            }
        }))
        .unwrap_or_else(|_| Err(error("WORKER_PANIC", "Worker task panicked")));
        drop(workspace);
        Completion { identity, outcome }
    }
}
