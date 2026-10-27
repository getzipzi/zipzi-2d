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
        Self::with_runner(count, stack_bytes, Work::run_quantum, complete)
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        authoring::ModelingSampleQuery,
        jobs::{ReadTask, Task, WorkOutcome},
        Runtime, RuntimeLimits,
    };
    use std::{collections::HashSet, sync::mpsc, time::Duration};

    #[test]
    fn batch_quanta_reuse_the_same_worker_and_expired_continuations_drop_there() {
        let owner = thread::current().id();
        let mut runtime = Runtime::new(RuntimeLimits {
            running_jobs: 1,
            ..Default::default()
        });
        let client = runtime.register_client().unwrap();
        let session = runtime
            .create_session(
                client,
                &uuid::Uuid::new_v4().to_string(),
                zipzi_core::ModelSetup::default(),
            )
            .unwrap();
        let (tx, rx) = mpsc::channel();
        let mut pool = WorkerPool::new(1, 16 << 20, move |c| {
            tx.send((thread::current().id(), c)).unwrap();
        })
        .unwrap();
        let query = || ModelingSampleQuery {
            session,
            edit: None,
            parameters: vec![Default::default(); 64],
            checks: Default::default(),
            expected: None,
        };
        let job = runtime.request_modeling_samples(client, query()).unwrap();
        let mut threads = HashSet::new();
        for step in 0..64 {
            runtime.dispatch(&mut pool);
            let (worker, completion) = rx.recv_timeout(Duration::from_secs(10)).unwrap();
            threads.insert(worker);
            assert_ne!(worker, owner);
            assert_eq!(completion.is_terminal(), step == 63);
            assert!(runtime.complete(completion));
        }
        assert_eq!(threads.len(), 1);
        runtime.release_job(client, job).unwrap();
        for expired in [false, true] {
            let job = runtime.request_modeling_samples(client, query()).unwrap();
            runtime.dispatch(&mut pool);
            let (_, mut completion) = rx.recv_timeout(Duration::from_secs(10)).unwrap();
            let (drop_tx, drop_rx) = mpsc::channel();
            let WorkOutcome::Yielded(work) = &mut completion.outcome else {
                panic!()
            };
            let Task::Read(ReadTask::Batch(batch)) = &mut work.task else {
                panic!()
            };
            batch.drop_probe = Some(drop_tx);
            if expired {
                runtime.release_client(client);
                assert!(runtime.complete(completion));
            } else {
                assert!(runtime.complete(completion));
                runtime.cancel_job(client, job).unwrap();
            }
            assert!(
                drop_rx.try_recv().is_err(),
                "owner thread destroyed the continuation"
            );
            runtime.dispatch(&mut pool);
            let (worker, cleanup) = rx.recv_timeout(Duration::from_secs(10)).unwrap();
            assert_eq!(
                drop_rx.recv_timeout(Duration::from_secs(10)).unwrap(),
                worker
            );
            assert!(threads.contains(&worker));
            assert!(runtime.complete(cleanup));
            if !expired {
                runtime.release_job(client, job).unwrap();
            }
        }
        assert_eq!(runtime.live_counts().3, 0);
    }
}
