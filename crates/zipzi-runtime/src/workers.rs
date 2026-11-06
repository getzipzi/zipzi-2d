//! Host-owned persistent workers. Runtime still decides admission and dispatch.
use crate::{Completion, Executor, Work};
use std::sync::Arc;
use std::thread::{self, JoinHandle};

pub struct WorkerPool {
    queue: async_channel::Sender<Work>,
    threads: Vec<JoinHandle<()>>,
}
impl WorkerPool {
    /// The completion sink must be nonblocking and must not panic. The host
    /// publishes every completion and dispatches again on its owner thread.
    pub fn new(
        count: usize,
        stack_bytes: usize,
        complete: impl Fn(Completion) + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        Self::with_runner(count, stack_bytes, Work::run, complete)
    }
    pub(crate) fn with_runner(
        count: usize,
        stack_bytes: usize,
        run: impl Fn(Work) -> Completion + Send + Sync + 'static,
        complete: impl Fn(Completion) + Send + Sync + 'static,
    ) -> std::io::Result<Self> {
        if count == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "worker count must be nonzero",
            ));
        }
        let (queue, receiver) = async_channel::bounded::<Work>(count);
        let mut pool = Self {
            queue,
            threads: vec![],
        };
        let run = Arc::new(run);
        let complete = Arc::new(complete);
        for index in 0..count {
            let receiver = receiver.clone();
            let run = run.clone();
            let complete = complete.clone();
            pool.threads.push(
                thread::Builder::new()
                    .name(format!("zipzi-runtime-worker-{index}"))
                    .stack_size(stack_bytes)
                    .spawn(move || {
                        while let Ok(work) = receiver.recv_blocking() {
                            complete(run(work));
                        }
                    })?,
            );
        }
        Ok(pool)
    }
}
impl Executor for WorkerPool {
    fn submit(&mut self, work: Work) -> Result<(), Box<Work>> {
        self.queue
            .try_send(work)
            .map_err(|e| Box::new(e.into_inner()))
    }
}
impl Drop for WorkerPool {
    fn drop(&mut self) {
        self.queue.close();
        for worker in self.threads.drain(..) {
            let _ = worker.join();
        }
    }
}
