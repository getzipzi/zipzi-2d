//! Explicitly owned in-process host. Only its owner thread enters Runtime;
//! workers return completions even when the caller makes no further requests.
use crate::{
    ClientId, Completion, Executor, JobId, JobInfo, Result, Runtime, RuntimeError, RuntimeLimits,
    Work,
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicUsize, Ordering},
    mpsc,
};
use std::thread::{self, JoinHandle};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HostError {
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
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
    #[cfg(feature = "test-support")]
    Wake,
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
    #[cfg(feature = "test-support")]
    control: Arc<crate::test_support::Control>,
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
        #[cfg(feature = "test-support")]
        let control = runtime.test_controller();
        let workers = Workers::new(
            limits.running_jobs,
            sender.clone(),
            #[cfg(feature = "test-support")]
            control.clone(),
        )?;
        let owner = thread::Builder::new()
            .name("zipzi-local-runtime".into())
            .spawn(move || {
                run(runtime, receiver, workers);
            })?;
        Ok(Self {
            #[cfg(feature = "test-support")]
            control,
            sender,
            queued: Arc::new(AtomicUsize::new(0)),
            capacity: call_capacity,
            closed: AtomicBool::new(false),
            owner: Mutex::new(Some(owner)),
        })
    }
    #[cfg(feature = "test-support")]
    pub fn test_control(&self, action: &str) -> Result<bool> {
        let result = self.control.command(action);
        let _ = self.sender.send(Message::Wake);
        result
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
            runtime.refresh_memory_budget();
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| operation(runtime))) {
                Ok(value) => {
                    runtime.refresh_memory_budget();
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
    queue: async_channel::Sender<Work>,
    threads: Vec<JoinHandle<()>>,
    active: usize,
}
impl Workers {
    fn new(
        count: usize,
        complete: mpsc::Sender<Message>,
        #[cfg(feature = "test-support")] control: Arc<crate::test_support::Control>,
    ) -> std::io::Result<Self> {
        let (queue, receiver) = async_channel::bounded::<Work>(count);
        let mut workers = Self {
            queue,
            threads: vec![],
            active: 0,
        };
        for index in 0..count {
            let receiver = receiver.clone();
            let complete = complete.clone();
            #[cfg(feature = "test-support")]
            let control = control.clone();
            workers.threads.push(
                thread::Builder::new()
                    .name(format!("zipzi-local-io-{index}"))
                    .stack_size(16 << 20)
                    .spawn(move || {
                        while let Ok(work) = receiver.recv_blocking() {
                            #[cfg(feature = "test-support")]
                            let completion = control.run(work);
                            #[cfg(not(feature = "test-support"))]
                            let completion = work.run();
                            if complete
                                .send(Message::Complete(Box::new(completion)))
                                .is_err()
                            {
                                break;
                            }
                        }
                    })?,
            );
        }
        Ok(workers)
    }
}
impl Executor for Workers {
    fn submit(&mut self, work: Work) -> std::result::Result<(), Box<Work>> {
        self.queue
            .try_send(work)
            .map_err(|error| Box::new(error.into_inner()))?;
        self.active += 1;
        Ok(())
    }
}
impl Drop for Workers {
    fn drop(&mut self) {
        self.queue.close();
        for worker in self.threads.drain(..) {
            let _ = worker.join();
        }
    }
}
fn notify(runtime: &Runtime, waiters: &mut Vec<Waiter>) {
    let mut i = 0;
    while i < waiters.len() {
        let waiter = &waiters[i];
        match runtime.job_info(waiter.client, waiter.job) {
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
                runtime.complete(*completion);
            }
            Message::Shutdown => stopping = true,
            #[cfg(feature = "test-support")]
            Message::Wake => (),
        }
        if stopping {
            runtime.cancel_host_jobs();
        }
        #[cfg(feature = "test-support")]
        let dispatch = stopping || runtime.test_controller().dispatch_enabled();
        #[cfg(not(feature = "test-support"))]
        let dispatch = true;
        if dispatch {
            runtime.dispatch(&mut workers);
        }
        notify(&runtime, &mut waiters);
        runtime.refresh_memory_budget();
        if stopping && workers.active == 0 {
            break;
        }
    }
    // `workers` finish before Runtime is dropped. No caller or getter is needed
    // to publish a completion, and no thread retains a mutable Runtime borrow.
}

#[cfg(test)]
mod tests {
    #[test]
    fn call_mailbox_cells_do_not_inline_formal_sessions() {
        assert!(std::mem::size_of::<super::Message>() <= 128);
        assert!(std::mem::size_of::<crate::Completion>() > 1024);
    }

    use super::*;
    use std::time::Duration;

    #[test]
    fn full_queue_confirms_no_execution_and_returns_its_permit() {
        let host = Arc::new(LocalHost::new(Default::default(), 1).unwrap());
        let (entered, entry) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let blocking = host.clone();
        let caller = thread::spawn(move || {
            blocking.call(move |_| {
                entered.send(()).unwrap();
                blocked.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(())
            })
        });
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        let (executed, execution) = mpsc::channel();
        let permit = host.permit().unwrap();
        host.sender
            .send(Message::Call(
                Box::new(move |_| {
                    executed.send(()).unwrap();
                    true
                }),
                permit,
            ))
            .unwrap();
        assert_eq!(
            host.call::<()>(|_| panic!("rejected call must never execute")),
            Err(HostError::Full)
        );
        release.send(()).unwrap();
        caller.join().unwrap().unwrap();
        execution.recv_timeout(Duration::from_secs(5)).unwrap();
        host.call(|_| Ok(())).unwrap();
        assert_eq!(host.queued.load(Ordering::Acquire), 0);
    }

    #[test]
    fn concurrent_close_waits_accepted_calls_and_rejects_later_admission() {
        let host = Arc::new(LocalHost::new(Default::default(), 2).unwrap());
        let (entered, entry) = mpsc::channel();
        let (release, blocked) = mpsc::channel();
        let calling = host.clone();
        let caller = thread::spawn(move || {
            calling.call(move |_| {
                entered.send(()).unwrap();
                blocked.recv_timeout(Duration::from_secs(5)).unwrap();
                Ok(())
            })
        });
        entry.recv_timeout(Duration::from_secs(5)).unwrap();
        let closing = host.clone();
        let first = thread::spawn(move || closing.close());
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !host.closed.load(Ordering::Acquire) {
            assert!(std::time::Instant::now() < deadline);
            thread::yield_now();
        }
        assert_eq!(host.call(|_| Ok(())), Err(HostError::Closed));
        let closing = host.clone();
        let second = thread::spawn(move || closing.close());
        release.send(()).unwrap();
        caller.join().unwrap().unwrap();
        first.join().unwrap().unwrap();
        second.join().unwrap().unwrap();
    }
}
