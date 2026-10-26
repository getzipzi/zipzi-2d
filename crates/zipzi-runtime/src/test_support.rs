//! Deterministic CPU acceptance controls. Compiled only with `test-support`;
//! no environment switch or wire operation exists in production builds.
use crate::{Completion, Result, RuntimeError, Work};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Condvar, Mutex,
};

#[derive(Default)]
pub struct Control {
    paused: AtomicBool,
    held: AtomicBool,
    entered: AtomicUsize,
    fault: AtomicUsize,
    lock: Mutex<()>,
    wake: Condvar,
}
impl Control {
    pub fn command(&self, action: &str) -> Result<bool> {
        match action {
            "pause_dispatch" => self.paused.store(true, Ordering::SeqCst),
            "resume_dispatch" => self.paused.store(false, Ordering::SeqCst),
            "hold_io" => self.held.store(true, Ordering::SeqCst),
            "release_io" => {
                let _lock = self.lock.lock().unwrap();
                self.held.store(false, Ordering::SeqCst);
                self.wake.notify_all();
            }
            "panic_next" => self.fault.store(1, Ordering::SeqCst),
            "fail_next" => self.fault.store(2, Ordering::SeqCst),
            "entered" => return Ok(self.entered.load(Ordering::SeqCst) != 0),
            _ => return Err(RuntimeError::InvalidReference),
        }
        Ok(true)
    }
    pub fn dispatch_enabled(&self) -> bool {
        !self.paused.load(Ordering::SeqCst)
    }
    pub fn run(&self, work: Work) -> Completion {
        let fault = self.fault.swap(0, Ordering::SeqCst);
        if fault == 2 {
            return work.executor_failure("injected executor rejection".into());
        }
        let entered = std::cell::Cell::new(false);
        let result = work.run_with_hook(|| {
            entered.set(true);
            self.entered.fetch_add(1, Ordering::SeqCst);
            if fault == 1 {
                panic!("injected worker panic");
            }
            if self.held.load(Ordering::SeqCst) {
                let mut lock = self.lock.lock().unwrap();
                while self.held.load(Ordering::SeqCst) {
                    let (next, timeout) = self
                        .wake
                        .wait_timeout(lock, std::time::Duration::from_secs(5))
                        .unwrap();
                    lock = next;
                    assert!(!timeout.timed_out(), "test IO gate was not released");
                }
            }
        });
        // Cancelled work can stop before entering the hook.
        if entered.get() {
            self.entered.fetch_sub(1, Ordering::SeqCst);
        }
        result
    }
}
