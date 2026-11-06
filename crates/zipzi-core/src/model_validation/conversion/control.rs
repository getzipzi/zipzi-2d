use super::*;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// One conversion's cooperative lifetime. Clones share cancellation and progress;
/// it is session state, never document data or an Undo checkpoint.
#[derive(Clone, Default)]
pub struct ConversionControl {
    cancelled: Arc<AtomicBool>,
    checkpoints: Arc<AtomicU64>,
    progress: Option<Arc<dyn Fn(u64) -> bool + Send + Sync>>,
    parents: Option<Arc<(Self, Self)>>,
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
    /// Check both owners at each bounded conversion checkpoint, preserving their
    /// progress callbacks and cancellation flags.
    pub fn linked(&self, other: &Self) -> Self {
        let first = self.clone();
        let second = other.clone();
        let mut linked =
            Self::with_progress(move |_| first.checkpoint().is_ok() && second.checkpoint().is_ok());
        linked.parents = Some(Arc::new((self.clone(), other.clone())));
        linked
    }
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
            || self
                .parents
                .as_ref()
                .is_some_and(|p| p.0.is_cancelled() || p.1.is_cancelled())
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
    pub(crate) fn checkpoint(&self) -> Result<(), Status> {
        self.check()?;
        let n = self.checkpoints.fetch_add(1, Ordering::AcqRel) + 1;
        if self.progress.as_ref().is_some_and(|f| !f(n)) {
            self.cancel();
        }
        self.check()
    }
}

#[cfg(test)]
mod linked_tests {
    use super::*;
    #[test]
    fn linked_control_observes_both_owners_without_waiting_for_progress() {
        for cancel_first in [true, false] {
            let first = ConversionControl::default();
            let second = ConversionControl::default();
            let linked = first.linked(&second);
            if cancel_first {
                first.cancel();
            } else {
                second.cancel();
            }
            assert!(linked.is_cancelled());
            assert!(linked.check().is_err());
        }
    }
}
