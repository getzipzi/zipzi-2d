//! Shared accounting for immutable image buffers. Each Arc allocation is charged
//! once, including buffers retained by textures after their registry entry dies.
use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, Weak,
    },
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
    fn resident_bytes(self) -> usize {
        self.rgba_bytes
            .saturating_add(self.png_bytes)
            .saturating_add(self.blob_bytes)
    }

    pub fn total_bytes(self) -> usize {
        self.resident_bytes()
            .saturating_add(self.preparing_bytes)
            .saturating_add(self.transfer_bytes)
    }
}
struct Buffer {
    data: Weak<[u8]>,
    kind: ImageBufferKind,
    bytes: usize,
}
struct SystemMemoryLimit {
    headroom: Box<dyn FnMut() -> usize + Send>,
    last_sample: Option<(usize, usize)>,
}

impl SystemMemoryLimit {
    fn limit(&mut self, resident_bytes: usize) -> usize {
        let headroom = (self.headroom)();
        let mut limit = resident_bytes.saturating_add(headroom);
        if let Some((previous_headroom, previous_limit)) = self.last_sample {
            // OS queries can return a cached sample. Keep charging newly
            // committed buffers until the reported headroom changes.
            if headroom == previous_headroom {
                limit = limit.min(previous_limit);
            }
        }
        self.last_sample = Some((headroom, limit));
        limit
    }
}

struct State {
    buffers: HashMap<usize, Buffer>,
    preparing: usize,
    limit: usize,
    transfer: usize,
    host_bytes: usize,
    host_limit: usize,
    system_memory: Option<SystemMemoryLimit>,
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
    fn system_limit(&mut self, usage: ImageMemoryUsage) -> usize {
        match &mut self.system_memory {
            // The system sample already reflects allocated image buffers.
            // Outstanding reservations must still compete for the headroom.
            Some(memory) => memory.limit(usage.resident_bytes()),
            None => usize::MAX,
        }
    }

    fn admits_host_growth(
        &mut self,
        usage: ImageMemoryUsage,
        host_bytes: usize,
        limit: usize,
        growth: usize,
    ) -> bool {
        let limit = if growth == 0 && host_bytes <= self.host_bytes {
            limit
        } else {
            limit.min(self.system_limit(usage))
        };
        usage
            .total_bytes()
            .checked_add(host_bytes)
            .and_then(|n| n.checked_add(growth))
            .is_some_and(|n| n <= limit)
    }

    fn admits_growth(&mut self, usage: ImageMemoryUsage, growth: usize) -> bool {
        self.admits_host_growth(usage, self.host_bytes, self.host_limit, growth)
    }

    fn collect_unused(&mut self) -> usize {
        let before = self.buffers.len();
        self.buffers.retain(|_, b| b.data.strong_count() > 0);
        before - self.buffers.len()
    }
    fn usage(&mut self) -> ImageMemoryUsage {
        self.collect_unused();
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
        Self::with_limits(limit_bytes, None)
    }

    /// The callback supplies additional system memory available for allocation.
    /// It runs under the admission lock and must not call back into the budget.
    pub fn with_system_headroom(headroom: impl FnMut() -> usize + Send + 'static) -> Self {
        Self::with_limits(
            usize::MAX,
            Some(SystemMemoryLimit {
                headroom: Box::new(headroom),
                last_sample: None,
            }),
        )
    }

    fn with_limits(limit_bytes: usize, system_memory: Option<SystemMemoryLimit>) -> Self {
        Self(Arc::new(Mutex::new(State {
            buffers: HashMap::new(),
            preparing: 0,
            limit: limit_bytes,
            transfer: 0,
            host_bytes: 0,
            host_limit: usize::MAX,
            system_memory,
        })))
    }
    pub fn same_pool(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn available_bytes(&self) -> usize {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let usage = state.usage();
        let host_limit = state.host_limit.min(state.system_limit(usage));
        state
            .limit
            .saturating_sub(usage.total_bytes().saturating_sub(usage.transfer_bytes))
            .min(
                host_limit
                    .saturating_sub(state.host_bytes)
                    .saturating_sub(usage.total_bytes()),
            )
    }
    pub fn usage(&self) -> ImageMemoryUsage {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let mut usage = state.usage();
        if state.system_memory.is_some() {
            usage.limit_bytes = usage
                .limit_bytes
                .min(state.host_limit.min(state.system_limit(usage)));
        }
        usage
    }
    /// Release registrations without a remaining pixel owner. Weak slice Arcs
    /// still retain the inline allocation after the final strong owner drops;
    /// cleanup boundaries must sweep them even if no further allocation follows.
    /// Returns the number of retired registrations.
    pub fn collect_unused(&self) -> usize {
        self.0
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .collect_unused()
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
        let usage = state.usage();
        if !state.admits_host_growth(usage, bytes, limit, 0) {
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
        let usage = state.usage();
        if !state.admits_growth(usage, bytes) {
            return Err(Status::error(
                "BUDGET_EXCEEDED",
                "Host transfer/temporary budget exceeded",
            ));
        }
        state.transfer += bytes;
        Ok(TransferReservation {
            budget: self.clone(),
            bytes: AtomicUsize::new(bytes),
        })
    }
    /// Atomically replace a completed task's host reservation with retained
    /// result storage. Other allocating threads never observe the old peak as
    /// free space before the result lease has been charged.
    pub fn reserve_transfer_with_host_budget(
        &self,
        bytes: usize,
        host_bytes: usize,
        host_limit: usize,
    ) -> Result<TransferReservation, Status> {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let usage = state.usage();
        if !state.admits_host_growth(usage, host_bytes, host_limit, bytes) {
            return Err(Status::error(
                "BUDGET_EXCEEDED",
                "Retained result exceeds host memory budget",
            ));
        }
        state.transfer = state.transfer.checked_add(bytes).ok_or_else(|| {
            Status::error("BUDGET_EXCEEDED", "Retained result byte count overflows")
        })?;
        state.host_bytes = host_bytes;
        state.host_limit = host_limit;
        Ok(TransferReservation {
            budget: self.clone(),
            bytes: AtomicUsize::new(bytes),
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
            || !state.admits_growth(usage, bytes)
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
        if growth.is_none_or(|growth| {
            usage
                .total_bytes()
                .saturating_sub(usage.transfer_bytes)
                .checked_add(growth)
                .is_none_or(|n| n > state.limit)
                || !state.admits_growth(usage, growth)
        }) {
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
    bytes: AtomicUsize,
}
impl TransferReservation {
    pub fn reserved_bytes(&self) -> usize {
        self.bytes.load(Ordering::Relaxed)
    }

    pub(crate) fn shares_budget(&self, budget: &ImageMemoryBudget) -> bool {
        self.budget.same_pool(budget)
    }

    pub(crate) fn shrink_to(&self, bytes: usize) {
        let mut state = self.budget.0.lock().unwrap_or_else(|p| p.into_inner());
        let previous = self.bytes.load(Ordering::Relaxed);
        if bytes < previous {
            state.transfer -= previous - bytes;
            self.bytes.store(bytes, Ordering::Relaxed);
        }
    }

    /// Grow a worker's reservation after bounded metadata admission.
    /// Shared-budget locking makes concurrent growth atomic.
    pub fn ensure_at_least(&self, bytes: usize) -> Result<(), Status> {
        let mut state = self.budget.0.lock().unwrap_or_else(|p| p.into_inner());
        let previous = self.bytes.load(Ordering::Relaxed);
        if bytes <= previous {
            return Ok(());
        }
        let growth = bytes - previous;
        let usage = state.usage();
        if !state.admits_growth(usage, growth) {
            return Err(Status::error(
                "BUDGET_EXCEEDED",
                "Project decode workspace exceeds host memory budget",
            ));
        }
        state.transfer += growth;
        self.bytes.store(bytes, Ordering::Relaxed);
        Ok(())
    }
}
impl Drop for TransferReservation {
    fn drop(&mut self) {
        let mut state = self.budget.0.lock().unwrap_or_else(|p| p.into_inner());
        state.transfer -= self.bytes.load(Ordering::Relaxed);
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
    fn explicit_sweep_keeps_live_pixels_and_retires_weak_slice_allocations() {
        let budget = ImageMemoryBudget::new(1024);
        let image = crate::PreparedImageContent::from_rgba_bytes(1, 1, vec![1, 2, 3, 255], &budget)
            .unwrap();
        let pixels = image.texture();
        drop(image);
        assert_eq!(budget.collect_unused(), 0);
        assert_eq!(pixels.rgba.as_ref(), &[1, 2, 3, 255]);
        drop(pixels);
        assert_eq!(budget.collect_unused(), 1);
        assert_eq!(budget.collect_unused(), 0);
    }
    #[test]
    fn worker_reservation_growth_shares_admission_and_releases_the_full_peak() {
        let budget = ImageMemoryBudget::new(1024);
        budget.bind_host_budget(10, 100);
        let first = Arc::new(budget.reserve_transfer(10).unwrap());
        let second = budget.reserve_transfer(20).unwrap();
        first.ensure_at_least(60).unwrap();
        first.ensure_at_least(30).unwrap();
        assert_eq!(first.reserved_bytes(), 60);
        assert_eq!(budget.usage().transfer_bytes, 80);
        assert!(second.ensure_at_least(31).is_err());
        assert_eq!(budget.usage().transfer_bytes, 80);
        drop(first);
        second.ensure_at_least(90).unwrap();
        drop(second);
        assert_eq!(budget.usage().transfer_bytes, 0);
    }

    #[test]
    fn completed_peak_is_atomically_replaced_by_a_retained_result_lease() {
        let budget = ImageMemoryBudget::new(1024);
        budget.bind_host_budget(100, 128);
        assert!(budget.reserve_transfer(80).is_err());
        let result = budget
            .reserve_transfer_with_host_budget(80, 20, 128)
            .unwrap();
        assert_eq!(budget.usage().transfer_bytes, 80);
        assert!(budget.reserve_transfer(29).is_err());
        assert!(budget
            .reserve_transfer_with_host_budget(129, 0, 128)
            .is_err());
        assert_eq!(budget.usage().transfer_bytes, 80);
        drop(result);
        assert_eq!(budget.usage().transfer_bytes, 0);
        assert!(budget.reserve_transfer(108).is_ok());
    }
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
    fn system_headroom_follows_pressure_and_counts_pending_work_once() {
        let headroom = Arc::new(AtomicUsize::new(96));
        let budget = ImageMemoryBudget::with_system_headroom({
            let headroom = headroom.clone();
            move || headroom.load(Ordering::Relaxed)
        });
        let pixels: Arc<[u8]> = vec![0; 32].into();
        budget.track(&pixels, ImageBufferKind::Rgba).unwrap();
        headroom.store(64, Ordering::Relaxed);
        let image = budget.reserve(24).unwrap();
        let transfer = budget.reserve_transfer(16).unwrap();
        assert_eq!(budget.available_bytes(), 24);
        assert_eq!(budget.usage().limit_bytes, 96);

        headroom.store(40, Ordering::Relaxed);
        assert_eq!(budget.available_bytes(), 0);
        assert_eq!(
            budget.reserve(1).err().unwrap().code,
            "RESOURCE_BUDGET_EXCEEDED"
        );
        assert_eq!(
            transfer.ensure_at_least(17).unwrap_err().code,
            "BUDGET_EXCEEDED"
        );
        assert_eq!(transfer.reserved_bytes(), 16);

        headroom.store(0, Ordering::Relaxed);
        budget.track(&pixels, ImageBufferKind::Rgba).unwrap();
        budget.reserve_transfer(0).unwrap();
        let prepared: Arc<[u8]> = vec![0; 24].into();
        image.commit(&prepared, ImageBufferKind::Png).unwrap();

        headroom.store(48, Ordering::Relaxed);
        transfer.ensure_at_least(48).unwrap();
        assert_eq!(budget.available_bytes(), 0);
        drop(transfer);
        assert_eq!(budget.available_bytes(), 48);
    }

    #[test]
    fn repeated_system_sample_cannot_admit_the_same_bytes_twice() {
        let budget = ImageMemoryBudget::with_system_headroom(|| 64);
        let reservation = budget.reserve(40).unwrap();
        let pixels: Arc<[u8]> = vec![0; 40].into();
        reservation.commit(&pixels, ImageBufferKind::Rgba).unwrap();
        assert_eq!(budget.available_bytes(), 24);
        assert!(budget.reserve(25).is_err());
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
