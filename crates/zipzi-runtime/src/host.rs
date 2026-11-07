//! Explicitly owned in-process host. Only its owner thread enters Runtime;
//! workers return completions even when the caller makes no further requests.
use crate::{ClientId, Completion, Executor, JobId, JobInfo, Result, Runtime, RuntimeLimits, Work};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::thread::{self, JoinHandle};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostError {
    #[error("{0:?}")]
    Runtime(zipzi_sdk::Status),
    #[error("Local host call queue is full; operation was not executed")]
    Full,
    #[error("Local host is closed")]
    Closed,
    #[error("Local host operation panicked; host has stopped accepting calls")]
    Panicked,
}
pub type HostResult<T> = std::result::Result<T, HostError>;
type Reply<T> = mpsc::SyncSender<HostResult<T>>;
type Call = Box<dyn FnOnce(&mut Runtime) -> bool + Send>;

// Calls/waiters require a permit before enqueueing. Completions use the same
// mailbox without a permit: at most running_jobs can exist, and a full call
// queue can never prevent a worker from returning ownership to Runtime.
struct Permit(Arc<AtomicUsize>);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
enum Message {
    Call(Call, Permit),
    Wait(ClientId, JobId, Reply<JobInfo>, Permit),
    Complete(Box<Completion>),
    Shutdown,
}
struct Waiter {
    client: ClientId,
    job: JobId,
    reply: Reply<JobInfo>,
    _permit: Permit,
}

/// Local lifetime differs from an IPC connection: closing this owner drops all
/// its sessions, without saving them. Entered IO finishes with its actual result;
/// queued jobs are cancelled. Waiting never holds a Runtime lock.
pub struct LocalHost {
    sender: mpsc::Sender<Message>,
    queued: Arc<AtomicUsize>,
    capacity: usize,
    closed: AtomicBool,
    owner: Mutex<Option<JoinHandle<()>>>,
}
impl LocalHost {
    pub fn new(limits: RuntimeLimits, call_capacity: usize) -> std::io::Result<Self> {
        if call_capacity == 0 || limits.running_jobs == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "local host requires a nonzero call capacity and worker count",
            ));
        }
        let (sender, receiver) = mpsc::channel();
        let runtime = Runtime::new(limits.clone());
        let workers = Workers::new(limits.running_jobs, sender.clone())?;
        let owner = thread::Builder::new()
            .name("zipzi-local-runtime".into())
            .spawn(move || {
                run(runtime, receiver, workers);
            })?;
        Ok(Self {
            sender,
            queued: Arc::new(AtomicUsize::new(0)),
            capacity: call_capacity,
            closed: AtomicBool::new(false),
            owner: Mutex::new(Some(owner)),
        })
    }
    fn permit(&self) -> HostResult<Permit> {
        if self.closed.load(Ordering::Acquire) {
            return Err(HostError::Closed);
        }
        let mut queued = self.queued.load(Ordering::Acquire);
        loop {
            if queued >= self.capacity {
                return Err(HostError::Full);
            }
            match self.queued.compare_exchange_weak(
                queued,
                queued + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(actual) => queued = actual,
            }
        }
        Ok(Permit(self.queued.clone()))
    }
    /// Rust adapters submit concrete typed operations. This closure must never
    /// invoke Python/user callbacks or block waiting for one of this host's jobs.
    pub fn call<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut Runtime) -> Result<T> + Send + 'static,
    ) -> HostResult<T> {
        let permit = self.permit()?;
        let (reply, receive) = mpsc::sync_channel(1);
        let call = Box::new(move |runtime: &mut Runtime| {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(runtime))) {
                Ok(value) => {
                    let _ = reply.send(value.map_err(HostError::Runtime));
                    true
                }
                Err(_) => {
                    let _ = reply.send(Err(HostError::Panicked));
                    false
                }
            }
        });
        self.sender
            .send(Message::Call(call, permit))
            .map_err(|_| HostError::Closed)?;
        receive.recv().unwrap_or(Err(HostError::Closed))
    }
    /// Register a completion waiter on the owner thread, then wait on the caller
    /// thread. PyO3 must release the GIL around this and `call`/`close`.
    pub fn wait_job(&self, client: ClientId, job: JobId) -> HostResult<JobInfo> {
        let permit = self.permit()?;
        let (reply, receive) = mpsc::sync_channel(1);
        self.sender
            .send(Message::Wait(client, job, reply, permit))
            .map_err(|_| HostError::Closed)?;
        receive.recv().unwrap_or(Err(HostError::Closed))
    }
    pub fn close(&self) -> HostResult<()> {
        // Only close calls share this lock; neither call/wait nor the owner
        // thread acquires it. Concurrent close waits for the same shutdown.
        let mut owner = self.owner.lock().map_err(|_| HostError::Panicked)?;
        self.closed.store(true, Ordering::Release);
        if let Some(owner) = owner.take() {
            let _ = self.sender.send(Message::Shutdown);
            owner.join().map_err(|_| HostError::Panicked)?;
        }
        Ok(())
    }
}
impl Drop for LocalHost {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

struct Workers {
    pool: crate::WorkerPool,
    active: usize,
}
impl Workers {
    fn new(count: usize, complete: mpsc::Sender<Message>) -> std::io::Result<Self> {
        let pool = crate::WorkerPool::with_runner(count, 16 << 20, Work::run, move |completion| {
            let _ = complete.send(Message::Complete(Box::new(completion)));
        })?;
        Ok(Self { pool, active: 0 })
    }
}
impl Executor for Workers {
    fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
        self.pool.submit(work)?;
        self.active += 1;
        Ok(())
    }
}

fn notify(runtime: &Runtime, waiters: &mut Vec<Waiter>) {
    let mut i = 0;
    while i < waiters.len() {
        let waiter = &waiters[i];
        match runtime.job(waiter.client, waiter.job).cloned() {
            Ok(info) if !info.state.is_terminal() => i += 1,
            result => {
                let waiter = waiters.swap_remove(i);
                let _ = waiter.reply.send(result.map_err(HostError::Runtime));
            }
        }
    }
}
fn run(mut runtime: Runtime, receiver: mpsc::Receiver<Message>, mut workers: Workers) {
    let mut waiters = vec![];
    let mut stopping = false;
    loop {
        let Ok(message) = receiver.recv() else {
            break;
        };
        match message {
            Message::Call(call, permit) => {
                drop(permit);
                if !stopping && !call(&mut runtime) {
                    stopping = true;
                }
            }
            Message::Wait(client, job, reply, permit) => {
                waiters.push(Waiter {
                    client,
                    job,
                    reply,
                    _permit: permit,
                });
            }
            Message::Complete(completion) => {
                workers.active -= 1;
                let _ = runtime.complete(*completion);
            }
            Message::Shutdown => stopping = true,
        }
        if stopping {
            runtime.cancel_host_jobs();
        }
        runtime.dispatch(&mut workers);
        notify(&runtime, &mut waiters);
        if stopping && workers.active == 0 && runtime.outstanding_jobs() == 0 {
            break;
        }
    }
    // `workers` finish before Runtime is dropped. No caller or getter is needed
    // to publish a completion, and no thread retains a mutable Runtime borrow.
}
