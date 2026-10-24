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
    pub limit_bytes: usize,
}
impl ImageMemoryUsage {
    pub fn total_bytes(self) -> usize {
        self.rgba_bytes
            .saturating_add(self.png_bytes)
            .saturating_add(self.blob_bytes)
            .saturating_add(self.preparing_bytes)
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
        })))
    }
    pub fn same_pool(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
    pub fn usage(&self) -> ImageMemoryUsage {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).usage()
    }
    pub fn reserve(&self, bytes: usize) -> Result<ImageMemoryReservation, Status> {
        let mut state = self.0.lock().unwrap_or_else(|p| p.into_inner());
        if state
            .usage()
            .total_bytes()
            .checked_add(bytes)
            .is_none_or(|n| n > state.limit)
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
            .and_then(|n| usage.total_bytes().checked_add(n))
            .is_none_or(|n| n > state.limit)
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
/// A preparation reservation is released on every failure/unwind. Commit replaces
/// it with weak allocation accounting; readers need no explicit release call.
pub struct ImageMemoryReservation {
    budget: ImageMemoryBudget,
    bytes: usize,
}
impl ImageMemoryReservation {
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
