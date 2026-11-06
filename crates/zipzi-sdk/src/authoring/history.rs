use super::*;
use std::{collections::VecDeque, sync::Arc};
use zipzi_core::state::WriteSet;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryLimits {
    pub max_steps: usize,
    /// A trimming target, never permission to begin or commit an edit.
    pub target_bytes: usize,
}
impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            max_steps: 50,
            target_bytes: 256 * 1024 * 1024,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryState {
    pub undo_steps: usize,
    pub redo_steps: usize,
    pub estimated_bytes: usize,
    pub limits: HistoryLimits,
    pub over_target: bool,
}
#[derive(Debug, Clone)]
pub(super) struct Step {
    pub before: Snapshot,
    pub after: Snapshot,
    pub description: Arc<str>,
    pub writes: Arc<WriteSet>,
}
#[derive(Debug, Clone, Default)]
pub(super) struct History {
    pub steps: VecDeque<Step>,
    pub cursor: usize,
    pub limits: HistoryLimits,
}
impl History {
    pub fn state(&self) -> HistoryState {
        let estimated_bytes = self.steps.iter().fold(0usize, |n, step| {
            n.saturating_add(step.writes.estimated_bytes())
        });
        HistoryState {
            undo_steps: self.cursor,
            redo_steps: self.steps.len() - self.cursor,
            estimated_bytes,
            limits: self.limits,
            over_target: estimated_bytes > self.limits.target_bytes,
        }
    }
    pub fn push(&mut self, step: Step) {
        self.steps.truncate(self.cursor);
        self.steps.push_back(step);
        self.cursor = self.steps.len();
        self.trim();
    }
    /// Explicit maintenance preserves a contiguous interval around the current
    /// node. A single oversized step survives the byte target. max_steps == 0
    /// explicitly disables history, independently of the byte target.
    pub fn trim(&mut self) {
        while self.steps.len() > self.limits.max_steps
            || (self.steps.len() > 1 && self.state().over_target)
        {
            if self.cursor > 0 {
                self.steps.pop_front();
                self.cursor -= 1;
            } else {
                self.steps.pop_back();
            }
        }
    }
}
