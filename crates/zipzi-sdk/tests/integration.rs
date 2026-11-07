//! Ordinary CPU suites share one binary; alpha mesh keeps its separate geometry target.
//! Register new CPU suites here instead of adding another Cargo test target.

#[path = "vertical_slice.rs"]
mod vertical_slice;

mod support;
