//! Per-save worker feedback; byte counts describe uncompressed payload written.
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ProjectSaveStage {
    #[default]
    Queued,
    PreparingResources,
    Writing,
    Finishing,
    Published,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ProjectSaveState {
    pub stage: ProjectSaveStage,
    pub completed_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectSaveProgress(Arc<Mutex<ProjectSaveState>>);

impl ProjectSaveProgress {
    pub fn state(&self) -> ProjectSaveState {
        *self.0.lock().unwrap_or_else(|e| e.into_inner())
    }
    pub fn set_stage(&self, stage: ProjectSaveStage) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).stage = stage;
    }
    pub(super) fn writing(&self, total_bytes: u64) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = ProjectSaveState {
            stage: ProjectSaveStage::Writing,
            completed_bytes: 0,
            total_bytes,
        };
    }
    pub(super) fn advance(&self, count: usize) {
        let mut state = self.0.lock().unwrap_or_else(|e| e.into_inner());
        state.completed_bytes = state
            .completed_bytes
            .saturating_add(count as u64)
            .min(state.total_bytes);
    }
}

pub(super) struct ProgressWriter<'a, W> {
    pub writer: &'a mut W,
    pub progress: Option<&'a ProjectSaveProgress>,
}
impl<W: std::io::Write> std::io::Write for ProgressWriter<'_, W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let count = self.writer.write(bytes)?;
        if let Some(progress) = self.progress {
            progress.advance(count);
        }
        Ok(count)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.writer.flush()
    }
}
