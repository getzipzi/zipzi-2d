//! Test-only, per-thread requested-byte accounting. The codec is synchronous;
//! other concurrently running unit tests neither contribute nor reset counters.
use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

#[derive(Clone, Copy, Default, Debug)]
pub(super) struct Usage {
    pub live: isize,
    pub peak: usize,
}
thread_local! {
    static USAGE: Cell<Option<Usage>> = const { Cell::new(None) };
}

fn account(delta: isize) {
    let _ = USAGE.try_with(|slot| {
        if let Some(mut usage) = slot.get() {
            usage.live += delta;
            usage.peak = usage.peak.max(usage.live.max(0) as usize);
            slot.set(Some(usage));
        }
    });
}

struct MeteredSystem;
#[global_allocator]
static ALLOCATOR: MeteredSystem = MeteredSystem;

// SAFETY: every operation forwards its original pointer and layout to System.
// Accounting uses allocation-free TLS and does not change allocation ownership.
unsafe impl GlobalAlloc for MeteredSystem {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc(layout) };
        if !ptr.is_null() {
            account(layout.size() as isize);
        }
        ptr
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { System.alloc_zeroed(layout) };
        if !ptr.is_null() {
            account(layout.size() as isize);
        }
        ptr
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
        account(-(layout.size() as isize));
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let next = unsafe { System.realloc(ptr, layout, new_size) };
        if !next.is_null() {
            // Conservatively include both buffers, even when System grows in place.
            account(new_size as isize);
            account(-(layout.size() as isize));
        }
        next
    }
}

/// `initial` accounts for live output transferred from a preceding measurement.
/// Callers must keep all unrelated fixture setup and assertion formatting outside.
pub(super) fn measure<T>(initial: usize, f: impl FnOnce() -> T) -> (T, Usage) {
    struct Guard;
    impl Drop for Guard {
        fn drop(&mut self) {
            USAGE.with(|slot| slot.set(None));
        }
    }
    USAGE.with(|slot| {
        assert!(slot.get().is_none(), "nested allocation measurement");
        slot.set(Some(Usage {
            live: initial as isize,
            peak: initial,
        }));
    });
    let guard = Guard;
    let result = f();
    let usage = USAGE.with(|slot| slot.get().unwrap());
    drop(guard);
    (result, usage)
}

#[test]
fn tracks_live_buffers_growth_and_release() {
    let ((), usage) = measure(0, || {
        let mut bytes = vec![0u8; 4096];
        std::hint::black_box(&mut bytes);
        bytes.reserve_exact(8192);
        std::hint::black_box(&mut bytes);
        drop(bytes);
    });
    assert_eq!(usage.live, 0);
    assert!(usage.peak >= 12288);
}
