//! Bounded desktop file requests, shared by platform events and authenticated IPC.
use std::{collections::VecDeque, path::PathBuf};

pub const MAX_OPEN_PROJECTS: usize = 128;
pub const MAX_OPEN_PATH_BYTES: usize = 32 * 1024;
pub const MAX_OPEN_REQUEST_BYTES: usize = 1024 * 1024;

#[derive(Default)]
pub struct ProjectOpenQueue {
    paths: VecDeque<PathBuf>,
    bytes: usize,
}
impl ProjectOpenQueue {
    /// Admission is atomic: a rejected batch never drops previously queued paths.
    pub fn enqueue(&mut self, paths: Vec<PathBuf>) -> Result<(), &'static str> {
        if paths.is_empty() || paths.len() > MAX_OPEN_PROJECTS {
            return Err("Expected 1–128 project paths");
        }
        let mut bytes = 0usize;
        for path in &paths {
            let size = path.as_os_str().as_encoded_bytes().len();
            if !path.is_absolute()
                || size > MAX_OPEN_PATH_BYTES
                || path.as_os_str().as_encoded_bytes().contains(&0)
            {
                return Err("Expected an absolute project path within the path limit");
            }
            bytes = bytes
                .checked_add(size)
                .ok_or("Project request is too large")?;
        }
        if self.paths.len() + paths.len() > MAX_OPEN_PROJECTS
            || self.bytes + bytes > MAX_OPEN_REQUEST_BYTES
        {
            return Err("Project open queue is full; retry after pending projects open");
        }
        self.bytes += bytes;
        self.paths.extend(paths);
        Ok(())
    }
    pub fn pop(&mut self) -> Option<PathBuf> {
        let path = self.paths.pop_front()?;
        self.bytes -= path.as_os_str().as_encoded_bytes().len();
        Some(path)
    }
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }
}
