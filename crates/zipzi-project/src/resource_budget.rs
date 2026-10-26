//! Shared accounting for immutable image buffers. Each Arc allocation is charged
//! once, including buffers retained by textures after their registry entry dies.
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
};
use zipzi_core::Status;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageBufferKind {
    Rgba,
    Png,
    Blob,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
pub struct ImageMemoryUsage {
    pub allocations: usize,
    pub rgba_bytes: usize,
    pub png_bytes: usize,
    pub blob_bytes: usize,
    pub preparing_bytes: usize,
    pub transfer_bytes: usize,
    pub limit_bytes: usize,
}
impl ImageMemoryUsage {
    pub fn total_bytes(self) -> usize {
        self.rgba_bytes
            .saturating_add(self.png_bytes)
            .saturating_add(self.blob_bytes)
            .saturating_add(self.preparing_bytes)
            .saturating_add(self.transfer_bytes)
    }
}
struct Buffer {
    data: Weak<[u8]>,
    kind: ImageBufferKind,
    bytes: usize,
}
struct State {
    buffers: HashMap<usize, Buffer>,
    preparing: usize,
    limit: usize,
    transfer: usize,
    host_bytes: usize,
    host_limit: usize,
}
#[derive(Clone)]
pub struct ImageMemoryBudget(Arc<Mutex<State>>);
impl std::fmt::Debug for ImageMemoryBudget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.usage().fmt(f)
    }
}
impl Default for ImageMemoryBudget {
    fn default() -> Self {
        Self::new(usize::MAX)
    }
}
impl State {
    fn usage(&mut self) -> ImageMemoryUsage {
        self.buffers.retain(|_, b| b.data.strong_count() > 0);
        let mut usage = ImageMemoryUsage {
            allocations: self.buffers.len(),
            preparing_bytes: self.preparing,
            transfer_bytes: self.transfer,
            limit_bytes: self.limit,
            ..Default::default()
        };
        for b in self.buffers.values() {
            match b.kind {
                ImageBufferKind::Rgba => usage.rgba_bytes += b.bytes,
                ImageBufferKind::Png => usage.png_bytes += b.bytes,
                ImageBufferKind::Blob => usage.blob_bytes += b.bytes,
            }
        }
        usage
    }
}
impl ImageMemoryBudget {
    pub fn new(limit_bytes: usize) -> Self {
        Self(Arc::new(Mutex::new(State {
            buffers: HashMap::new(),
            preparing: 0,
            limit: limit_bytes,
            transfer: 0,
            host_bytes: 0,
            host_limit: usize::MAX,
        })))
    }
    pub fn same_pool(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn available_bytes(&self) -> usize {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let usage = state.usage();
        state
            .limit
            .saturating_sub(usage.total_bytes().saturating_sub(usage.transfer_bytes))
            .min(
                state
                    .host_limit
                    .saturating_sub(state.host_bytes)
                    .saturating_sub(usage.total_bytes()),
            )
    }
    pub fn usage(&self) -> ImageMemoryUsage {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).usage()
    }
    /// Owner-thread document/queue estimates share admission with resource and
    /// off-thread transport allocations. Binding never prevents cleanup.
    pub fn bind_host_budget(&self, bytes: usize, limit: usize) {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        state.host_bytes = bytes;
        state.host_limit = limit;
    }
    pub fn admit_host_budget(&self, bytes: usize, limit: usize) -> Result<(), Status> {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if state
            .usage()
            .total_bytes()
            .checked_add(bytes)
            .is_none_or(|n| n > limit)
        {
            return Err(Status::error(
                "BUDGET_EXCEEDED",
                "Host metadata/temporary budget exceeded",
            ));
        }
        state.host_bytes = bytes;
        state.host_limit = limit;
        Ok(())
    }
    pub fn reserve_transfer(&self, bytes: usize) -> Result<TransferReservation, Status> {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if state
            .usage()
            .total_bytes()
            .checked_add(state.host_bytes)
            .and_then(|v| v.checked_add(bytes))
            .is_none_or(|v| v > state.host_limit)
        {
            return Err(Status::error(
                "BUDGET_EXCEEDED",
                "Host transfer/temporary budget exceeded",
            ));
        }
        state.transfer += bytes;
        Ok(TransferReservation {
            budget: self.clone(),
            bytes,
        })
    }
    pub fn reserve(&self, bytes: usize) -> Result<ImageMemoryReservation, Status> {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let usage = state.usage();
        if usage
            .total_bytes()
            .saturating_sub(usage.transfer_bytes)
            .checked_add(bytes)
            .is_none_or(|n| n > state.limit)
            || usage
                .total_bytes()
                .checked_add(state.host_bytes)
                .and_then(|n| n.checked_add(bytes))
                .is_none_or(|n| n > state.host_limit)
        {
            return Err(Status::error(
                "RESOURCE_BUDGET_EXCEEDED",
                "Image buffers exceed the shared memory budget",
            ));
        }
        state.preparing += bytes;
        Ok(ImageMemoryReservation {
            budget: self.clone(),
            bytes,
        })
    }
    pub fn track(&self, data: &Arc<[u8]>, kind: ImageBufferKind) -> Result<(), Status> {
        self.track_buffers(&[(data, kind)])
    }
    /// Register a shared set atomically; rejection cannot leave a partial charge.
    pub fn track_buffers(&self, buffers: &[(&Arc<[u8]>, ImageBufferKind)]) -> Result<(), Status> {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let usage = state.usage();
        let mut additional = HashMap::new();
        for (data, _) in buffers {
            let key = data.as_ptr() as usize;
            if !state.buffers.contains_key(&key) {
                additional.insert(key, data.len());
            }
        }
        let growth = additional
            .values()
            .try_fold(0usize, |sum, n| sum.checked_add(*n));
        if growth
            .and_then(|n| {
                usage
                    .total_bytes()
                    .saturating_sub(usage.transfer_bytes)
                    .checked_add(n)
            })
            .is_none_or(|n| n > state.limit)
            || growth
                .and_then(|n| {
                    usage
                        .total_bytes()
                        .checked_add(state.host_bytes)?
                        .checked_add(n)
                })
                .is_none_or(|n| n > state.host_limit)
        {
            return Err(Status::error(
                "RESOURCE_BUDGET_EXCEEDED",
                "Image buffers exceed the shared memory budget",
            ));
        }
        for (data, kind) in buffers {
            let entry = state
                .buffers
                .entry(data.as_ptr() as usize)
                .or_insert_with(|| Buffer {
                    data: Arc::downgrade(data),
                    kind: *kind,
                    bytes: data.len(),
                });
            if entry.kind == ImageBufferKind::Blob && *kind != ImageBufferKind::Blob {
                entry.kind = *kind;
            }
        }
        Ok(())
    }
}
#[derive(Debug)]
pub struct TransferReservation {
    budget: ImageMemoryBudget,
    bytes: usize,
}
impl Drop for TransferReservation {
    fn drop(&mut self) {
        let mut state = self.budget.0.lock().unwrap_or_else(|p| p.into_inner());
        state.transfer -= self.bytes;
    }
}
/// A preparation reservation is released on every failure/unwind. Commit replaces
/// it with weak allocation accounting; readers need no explicit release call.
pub struct ImageMemoryReservation {
    budget: ImageMemoryBudget,
    bytes: usize,
}
impl ImageMemoryReservation {
    pub fn commit_many(mut self, buffers: &[(&Arc<[u8]>, ImageBufferKind)]) -> Result<(), Status> {
        let mut unique = HashMap::new();
        for (data, kind) in buffers {
            unique.insert(data.as_ptr() as usize, (*data, *kind));
        }
        if unique
            .values()
            .fold(0usize, |n, (data, _)| n.saturating_add(data.len()))
            > self.bytes
        {
            return Err(Status::error(
                "RESOURCE_BUDGET_EXCEEDED",
                "Decoded buffers exceeded reservation",
            ));
        }
        let mut state = self.budget.0.lock().unwrap_or_else(|p| p.into_inner());
        state.preparing -= self.bytes;
        self.bytes = 0;
        for (key, (data, kind)) in unique {
            state.buffers.entry(key).or_insert_with(|| Buffer {
                data: Arc::downgrade(data),
                kind,
                bytes: data.len(),
            });
        }
        Ok(())
    }

    pub fn commit(mut self, data: &Arc<[u8]>, kind: ImageBufferKind) -> Result<(), Status> {
        if data.len() > self.bytes {
            return Err(Status::error(
                "RESOURCE_BUDGET_EXCEEDED",
                "Image preparation exceeded its reservation",
            ));
        }
        let mut state = self.budget.0.lock().unwrap_or_else(|p| p.into_inner());
        state.preparing -= self.bytes;
        self.bytes = 0;
        state
            .buffers
            .entry(data.as_ptr() as usize)
            .or_insert_with(|| Buffer {
                data: Arc::downgrade(data),
                kind,
                bytes: data.len(),
            });
        Ok(())
    }
}
impl Drop for ImageMemoryReservation {
    fn drop(&mut self) {
        self.budget
            .0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .preparing -= self.bytes;
    }
}

// Actual Vec<u8> -> Arc<[u8]> payload copy in the selected preparation path.
#[cfg(feature = "bench-instrumentation")]
static RGBA_COPIES: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
#[cfg(feature = "bench-instrumentation")]
pub fn record_rgba_copy(bytes: usize) {
    RGBA_COPIES.fetch_add(bytes, std::sync::atomic::Ordering::Relaxed);
}
#[cfg(feature = "bench-instrumentation")]
pub fn rgba_copy_bytes(reset: bool) -> usize {
    if reset {
        RGBA_COPIES.swap(0, std::sync::atomic::Ordering::Relaxed)
    } else {
        RGBA_COPIES.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn transport_and_resource_reservations_share_the_aggregate_ceiling() {
        let budget = ImageMemoryBudget::new(8);
        budget.bind_host_budget(4, 16);
        let transfer = budget.reserve_transfer(5).unwrap();
        let image = budget.reserve(7).unwrap();
        assert!(budget.reserve_transfer(1).is_err());
        assert!(budget.reserve(1).is_err());
        assert!(budget.admit_host_budget(5, 16).is_err());
        drop(transfer);
        let second = budget.reserve(1).unwrap();
        assert!(budget.reserve(1).is_err());
        drop(image);
        drop(second);
        assert_eq!(budget.usage().total_bytes(), 0);
    }
    #[test]
    fn atomic_shared_allocation_registration_does_not_double_charge_or_leak() {
        let budget = ImageMemoryBudget::new(8);
        let a: Arc<[u8]> = vec![0; 4].into();
        let b: Arc<[u8]> = vec![0; 5].into();
        assert!(budget
            .track_buffers(&[(&a, ImageBufferKind::Rgba), (&b, ImageBufferKind::Png)])
            .is_err());
        assert_eq!(budget.usage().allocations, 0);
        budget
            .track_buffers(&[(&a, ImageBufferKind::Blob), (&a, ImageBufferKind::Blob)])
            .unwrap();
        budget.track(&a, ImageBufferKind::Rgba).unwrap();
        assert_eq!(budget.usage().rgba_bytes, 4);
        assert_eq!(budget.usage().blob_bytes, 0);
        drop(a);
        assert_eq!(budget.usage().total_bytes(), 0);
    }
}
