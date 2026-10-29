use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// One conversion's cooperative lifetime. Clones share cancellation and progress;
/// it is session state, never document data or an Undo checkpoint.
#[derive(Clone, Default)]
pub struct ConversionControl {
    cancelled: Arc<AtomicBool>,
    checkpoints: Arc<AtomicU64>,
    progress: Option<Arc<dyn Fn(u64) -> bool + Send + Sync>>,
}
impl std::fmt::Debug for ConversionControl {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConversionControl")
            .field("cancelled", &self.is_cancelled())
            .field("checkpoints", &self.checkpoints())
            .finish()
    }
}
impl ConversionControl {
    /// Returning false from the callback cancels. Callbacks run on the planning
    /// worker, at bounded units of conversion work, and must return promptly.
    pub fn with_progress(progress: impl Fn(u64) -> bool + Send + Sync + 'static) -> Self {
        Self {
            progress: Some(Arc::new(progress)),
            ..Self::default()
        }
    }
    /// Share the host job cancellation flag without introducing callbacks.
    pub fn from_cancel_flag(cancelled: Arc<AtomicBool>) -> Self {
        Self {
            cancelled,
            ..Self::default()
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
    pub fn checkpoints(&self) -> u64 {
        self.checkpoints.load(Ordering::Acquire)
    }
    pub fn check(&self) -> Result<(), Status> {
        if self.is_cancelled() {
            Err(Status::error(
                "AUTHORING_CANCELLED",
                "Conversion cancelled; no changes published",
            ))
        } else {
            Ok(())
        }
    }
    pub(in crate::document) fn checkpoint(&self) -> Result<(), Status> {
        self.check()?;
        let n = self.checkpoints.fetch_add(1, Ordering::AcqRel) + 1;
        if self.progress.as_ref().is_some_and(|f| !f(n)) {
            self.cancel();
        }
        self.check()
    }
}
