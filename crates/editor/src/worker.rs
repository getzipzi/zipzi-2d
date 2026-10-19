//! Scheduling and opt-in CPU accounting for the visible preview pipeline.
use gpui_kit::frame_trace;

/// Optional synchronous stage accounting; clocks are untouched without tracing.
pub(crate) fn trace_operation<T>(stage: &'static str, operation: impl FnOnce() -> T) -> T {
    let id = frame_trace::next_id();
    if id == 0 {
        return operation();
    }
    let input = frame_trace::current_input();
    let wall = frame_trace::now_ns();
    let cpu = cpu_time(id);
    let result = operation();
    let elapsed = frame_trace::now_ns().saturating_sub(wall);
    let cpu = cpu
        .zip(cpu_time(id))
        .map_or(0, |(a, b)| b.saturating_sub(a));
    frame_trace::emit(
        id,
        stage,
        &[("input", input), ("wall_ns", elapsed), ("cpu_ns", cpu)],
    );
    result
}

pub(crate) fn configure(worker: u64) {
    // Only the benchmark can opt out, so the same binary supports an A/B run.
    let baseline = frame_trace::enabled()
        && std::env::var("KASANE_TRACE_DEFAULT_WORKER_QOS").is_ok_and(|v| v == "1");
    if baseline {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        // These dedicated threads produce visible frames and sleep on their
        // mailboxes when idle. Do not apply this policy to imports or other jobs.
        // SAFETY: changes only the calling thread; both arguments are valid QoS values.
        let status = unsafe {
            libc::pthread_set_qos_class_self_np(libc::qos_class_t::QOS_CLASS_USER_INTERACTIVE, 0)
        };
        frame_trace::emit(
            frame_trace::next_id(),
            "worker_qos",
            &[("worker", worker), ("status", status as u64)],
        );
        if status != 0 {
            eprintln!("preview worker {worker}: could not set interactive QoS ({status})");
        }
    }
}

pub(crate) fn cpu_time(trace_id: u64) -> Option<u64> {
    if trace_id == 0 {
        return None;
    }
    #[cfg(target_os = "macos")]
    {
        let mut time = libc::timespec {
            tv_sec: 0,
            tv_nsec: 0,
        };
        // SAFETY: a valid writable timespec and the current thread's CPU clock.
        let status = unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut time) };
        (status == 0).then(|| time.tv_sec as u64 * 1_000_000_000 + time.tv_nsec as u64)
    }
    #[cfg(not(target_os = "macos"))]
    None
}

pub(crate) fn record_cpu_time(trace_id: u64, stage: &'static str, start: Option<u64>) {
    if let Some((start, end)) = start.zip(cpu_time(trace_id)) {
        frame_trace::emit(trace_id, stage, &[("cpu_ns", end.saturating_sub(start))]);
    }
}
