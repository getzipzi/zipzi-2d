//! Optional P0 probes. Allocator numbers are requested Rust heap bytes (all
//! threads in this image), excluding Python/C allocators and allocator overhead.
//! Copy counters cover explicit selected payload copies, not implicit Clone or
//! realloc internals. Timing builds do not enable the counting allocator.
use serde::{de::DeserializeOwned, Serialize};
#[cfg(feature = "bench-instrumentation")]
use std::sync::atomic::{AtomicUsize, Ordering::Relaxed};

#[derive(Debug, Clone, Copy, Default, Serialize)]
pub struct Snapshot {
    pub enabled: bool,
    pub rust_live_bytes: usize,
    pub rust_start_bytes: usize,
    pub rust_peak_bytes: usize,
    pub rust_allocations: usize,
    pub rust_allocated_bytes: usize,
    pub json_encodes: usize,
    pub json_decodes: usize,
    pub json_value_builds: usize,
    pub json_admission_encodes: usize,
    pub materialized_input_bytes: usize,
    pub copied_payload_bytes: usize,
}
#[cfg(feature = "bench-instrumentation")]
mod counted {
    use super::*;
    use std::alloc::{GlobalAlloc, Layout, System};
    pub static LIVE: AtomicUsize = AtomicUsize::new(0);
    pub static START: AtomicUsize = AtomicUsize::new(0);
    pub static PEAK: AtomicUsize = AtomicUsize::new(0);
    pub static ALLOCS: AtomicUsize = AtomicUsize::new(0);
    pub static BYTES: AtomicUsize = AtomicUsize::new(0);
    pub static ENCODE: AtomicUsize = AtomicUsize::new(0);
    pub static DECODE: AtomicUsize = AtomicUsize::new(0);
    pub static VALUE: AtomicUsize = AtomicUsize::new(0);
    pub static ADMISSION: AtomicUsize = AtomicUsize::new(0);
    pub static INPUT: AtomicUsize = AtomicUsize::new(0);
    pub static COPY: AtomicUsize = AtomicUsize::new(0);
    pub struct Allocator;
    fn added(bytes: usize) {
        let live = LIVE.fetch_add(bytes, Relaxed) + bytes;
        PEAK.fetch_max(live, Relaxed);
        ALLOCS.fetch_add(1, Relaxed);
        BYTES.fetch_add(bytes, Relaxed);
    }
    // SAFETY: all operations delegate unchanged pointer/layout contracts to
    // System. Probes only use allocation-free atomics, including during panic.
    unsafe impl GlobalAlloc for Allocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            let ptr = unsafe { System.alloc(layout) };
            if !ptr.is_null() {
                added(layout.size());
            }
            ptr
        }
        unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
            let ptr = unsafe { System.alloc_zeroed(layout) };
            if !ptr.is_null() {
                added(layout.size());
            }
            ptr
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe {
                System.dealloc(ptr, layout);
            }
            LIVE.fetch_sub(layout.size(), Relaxed);
        }
        unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
            let result = unsafe { System.realloc(ptr, layout, size) };
            if !result.is_null() {
                LIVE.fetch_sub(layout.size(), Relaxed);
                added(size);
            }
            result
        }
    }
    #[global_allocator]
    static ALLOCATOR: Allocator = Allocator;
}
#[inline]
pub fn copy_payload(bytes: usize) {
    #[cfg(feature = "bench-instrumentation")]
    counted::COPY.fetch_add(bytes, Relaxed);
    let _ = bytes;
}
#[inline]
pub fn materialized_input(bytes: usize) {
    #[cfg(feature = "bench-instrumentation")]
    counted::INPUT.fetch_add(bytes, Relaxed);
    let _ = bytes;
}
#[inline]
pub fn value_build() {
    #[cfg(feature = "bench-instrumentation")]
    counted::VALUE.fetch_add(1, Relaxed);
}
pub fn encode<T: Serialize + ?Sized>(value: &T) -> serde_json::Result<Vec<u8>> {
    #[cfg(feature = "bench-instrumentation")]
    counted::ENCODE.fetch_add(1, Relaxed);
    serde_json::to_vec(value)
}
pub fn decode<T: DeserializeOwned>(bytes: &[u8]) -> serde_json::Result<T> {
    #[cfg(feature = "bench-instrumentation")]
    counted::DECODE.fetch_add(1, Relaxed);
    serde_json::from_slice(bytes)
}
pub fn from_value<T: DeserializeOwned>(value: serde_json::Value) -> serde_json::Result<T> {
    #[cfg(feature = "bench-instrumentation")]
    counted::DECODE.fetch_add(1, Relaxed);
    serde_json::from_value(value)
}
pub fn admission_encode() {
    #[cfg(feature = "bench-instrumentation")]
    counted::ADMISSION.fetch_add(1, Relaxed);
}
pub fn reset() -> Snapshot {
    #[cfg(feature = "bench-instrumentation")]
    {
        use counted::*;
        for value in [
            &ALLOCS, &BYTES, &ENCODE, &DECODE, &VALUE, &ADMISSION, &INPUT, &COPY,
        ] {
            value.store(0, Relaxed);
        }
        zipzi_project::resource_budget::rgba_copy_bytes(true);
        let live = LIVE.load(Relaxed);
        START.store(live, Relaxed);
        PEAK.store(live, Relaxed);
    }
    snapshot()
}
pub fn snapshot() -> Snapshot {
    #[cfg(feature = "bench-instrumentation")]
    {
        use counted::*;
        Snapshot {
            enabled: true,
            rust_live_bytes: LIVE.load(Relaxed),
            rust_start_bytes: START.load(Relaxed),
            rust_peak_bytes: PEAK.load(Relaxed),
            rust_allocations: ALLOCS.load(Relaxed),
            rust_allocated_bytes: BYTES.load(Relaxed),
            json_encodes: ENCODE.load(Relaxed),
            json_decodes: DECODE.load(Relaxed),
            json_value_builds: VALUE.load(Relaxed),
            json_admission_encodes: ADMISSION.load(Relaxed),
            materialized_input_bytes: INPUT.load(Relaxed),
            copied_payload_bytes: COPY.load(Relaxed)
                + zipzi_project::resource_budget::rgba_copy_bytes(false),
        }
    }
    #[cfg(not(feature = "bench-instrumentation"))]
    {
        Snapshot::default()
    }
}
