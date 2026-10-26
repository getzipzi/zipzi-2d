//! Opt-in frame timing hooks. No logging, allocation or clock reads without a sink.
//! On macOS timestamps share Metal's host-time clock (seconds since mach boot).
use std::{
    cell::Cell,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
};

/// Labels carry preview identity with the Metal resource, across address reuse.
/// Used only while a frame trace sink is installed.
pub const PREVIEW_TEXTURE_LABEL_PREFIX: &str = "gpui-frame-trace-preview:";

/// One timestamped boundary and its numeric correlation fields.
pub struct Event {
    /// Host timestamp in nanoseconds.
    pub at_ns: u64,
    /// Process-wide correlation ID; zero means tracing is disabled.
    pub id: u64,
    /// Stable boundary name.
    pub stage: &'static str,
    /// Boundary-specific values (IDs, counts or nanoseconds).
    pub fields: Vec<(&'static str, u64)>,
}

static SINK: OnceLock<fn(Event)> = OnceLock::new();
static NEXT: AtomicU64 = AtomicU64::new(1);
thread_local! { static INPUT: Cell<u64> = const { Cell::new(0) }; }

/// Install a nonblocking process-wide sink before opening windows.
pub fn install(sink: fn(Event)) -> bool {
    SINK.set(sink).is_ok()
}
/// Whether a sink was installed.
pub fn enabled() -> bool {
    SINK.get().is_some()
}
/// Allocate a unique correlation ID only when enabled.
pub fn next_id() -> u64 {
    if enabled() {
        NEXT.fetch_add(1, Ordering::Relaxed)
    } else {
        0
    }
}
/// The synchronous input dispatch responsible for the current operation.
pub fn current_input() -> u64 {
    INPUT.get()
}

/// Read the monotonic host clock used by Metal GPU and drawable timestamps.
pub fn now_ns() -> u64 {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "QuartzCore", kind = "framework")]
        unsafe extern "C" {
            fn CACurrentMediaTime() -> f64;
        }
        unsafe { (CACurrentMediaTime() * 1e9) as u64 }
    }
    #[cfg(not(target_os = "macos"))]
    {
        static START: OnceLock<std::time::Instant> = OnceLock::new();
        START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_nanos() as u64
    }
}
/// Emit a boundary with a host timestamp supplied by the caller or GPU.
pub fn emit_at(id: u64, stage: &'static str, at_ns: u64, fields: &[(&'static str, u64)]) {
    if id != 0 {
        if let Some(sink) = SINK.get() {
            sink(Event {
                at_ns,
                id,
                stage,
                fields: fields.to_vec(),
            });
        }
    }
}
/// Emit a CPU boundary.
pub fn emit(id: u64, stage: &'static str, fields: &[(&'static str, u64)]) {
    if id != 0 {
        emit_at(id, stage, now_ns(), fields);
    }
}

/// Restores the parent input scope, including when dispatch returns early.
pub struct InputScope {
    id: u64,
    parent: u64,
}
impl InputScope {
    /// Begin an input scope; benchmark drivers use kind 100+ to identify synthetic input.
    pub fn begin(kind: u64) -> Self {
        let id = next_id();
        let parent = if id != 0 { INPUT.replace(id) } else { 0 };
        emit(id, "input_begin", &[("parent", parent), ("kind", kind)]);
        Self { id, parent }
    }
    /// This input's ID.
    pub fn id(&self) -> u64 {
        self.id
    }
}
impl Drop for InputScope {
    fn drop(&mut self) {
        if self.id != 0 {
            emit(self.id, "input_end", &[]);
            INPUT.set(self.parent);
        }
    }
}
