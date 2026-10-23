//! Opt-in synchronous editing spans. Absent from default builds.
//!
//! Inclusive time contains nested spans; self time excludes them. Only self
//! times may be summed across stages. This measures wall time on the caller's
//! thread, including any scheduling pauses, and never measures GPU execution.
use std::{cell::RefCell, collections::BTreeMap, marker::PhantomData, rc::Rc, time::Instant};

#[derive(Clone, Debug, Default)]
pub struct Measurement {
    pub calls: u64,
    pub inclusive_ns: u64,
    pub self_ns: u64,
}

#[derive(Default)]
struct Recorder {
    recording: bool,
    stack: Vec<u64>,
    stages: BTreeMap<&'static str, Measurement>,
}

thread_local! {
    static RECORDER: RefCell<Recorder> = RefCell::new(Recorder::default());
}

/// A recording owns the caller's thread-local recorder and is not Send.
pub struct Capture(PhantomData<Rc<()>>);

impl Capture {
    pub fn start() -> Self {
        RECORDER.with(|cell| {
            let mut recorder = cell.borrow_mut();
            assert!(!recorder.recording, "nested benchmark captures");
            assert!(recorder.stack.is_empty());
            // Retain allocations and stage keys between samples.
            recorder.stack.reserve(32);
            for value in recorder.stages.values_mut() {
                *value = Measurement::default();
            }
            recorder.recording = true;
        });
        Self(PhantomData)
    }

    pub fn finish(self) -> BTreeMap<&'static str, Measurement> {
        RECORDER.with(|cell| {
            let recorder = cell.borrow();
            assert!(recorder.stack.is_empty(), "benchmark span still alive");
            recorder
                .stages
                .iter()
                .filter(|(_, value)| value.calls > 0)
                .map(|(&key, value)| (key, value.clone()))
                .collect()
        })
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        RECORDER.with(|cell| cell.borrow_mut().recording = false);
    }
}

pub struct Span {
    name: &'static str,
    start: Instant,
    _thread: PhantomData<Rc<()>>,
}

pub fn span(name: &'static str) -> Option<Span> {
    RECORDER.with(|cell| {
        let mut recorder = cell.borrow_mut();
        if !recorder.recording {
            return None;
        }
        recorder.stack.push(0);
        Some(Span {
            name,
            start: Instant::now(),
            _thread: PhantomData,
        })
    })
}

/// Record units of work without reading the clock or changing the span stack.
/// Counter names must be distinct from timing span names. `calls` holds units;
/// inclusive_ns and self_ns stay zero for counter-only entries.
pub fn count(name: &'static str, units: usize) {
    RECORDER.with(|cell| {
        let mut recorder = cell.borrow_mut();
        if recorder.recording && units != 0 {
            recorder.stages.entry(name).or_default().calls += units as u64;
        }
    });
}

impl Drop for Span {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed().as_nanos() as u64;
        RECORDER.with(|cell| {
            let mut recorder = cell.borrow_mut();
            let children = recorder.stack.pop().expect("balanced benchmark spans");
            if let Some(parent) = recorder.stack.last_mut() {
                *parent += elapsed;
            }
            let measurement = recorder.stages.entry(self.name).or_default();
            measurement.calls += 1;
            measurement.inclusive_ns += elapsed;
            measurement.self_ns += elapsed.saturating_sub(children);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nested_spans_account_without_double_counting_and_reset_between_captures() {
        assert!(span("disabled").is_none());
        let capture = Capture::start();
        {
            let _outer = span("outer");
            for _ in 0..2 {
                let _inner = span("inner");
                std::hint::black_box(42);
            }
        }
        let values = capture.finish();
        assert_eq!(values["outer"].calls, 1);
        assert_eq!(values["inner"].calls, 2);
        assert_eq!(values["inner"].self_ns, values["inner"].inclusive_ns);
        assert_eq!(
            values["outer"].inclusive_ns,
            values["outer"].self_ns + values["inner"].inclusive_ns
        );
        assert!(span("disabled").is_none());
        assert!(Capture::start().finish().is_empty());
    }

    #[test]
    fn work_counters_accumulate_reset_and_do_not_use_the_timing_stack() {
        count("records", 99);
        let capture = Capture::start();
        {
            let _span = span("work");
            count("records", 2);
            count("records", 3);
            count("zero", 0);
        }
        let values = capture.finish();
        assert_eq!(values["records"].calls, 5);
        assert_eq!(values["records"].inclusive_ns, 0);
        assert_eq!(values["records"].self_ns, 0);
        assert!(!values.contains_key("zero"));
        assert_eq!(values["work"].inclusive_ns, values["work"].self_ns);
        assert!(Capture::start().finish().is_empty());
    }
}
