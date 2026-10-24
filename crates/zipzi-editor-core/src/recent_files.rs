//! Recent files history and path normalization.
use serde::{Deserialize, Serialize};
use std::path::{Component, Path, PathBuf};

pub const MAX_RECENT_FILES: usize = 10;

/// Pure lexical path normalization without resolving away target symlinks.
/// Converts relative paths to absolute using the current working directory,
/// and eliminates `.` and `..` components.
pub fn normalize_path(path: &Path) -> PathBuf {
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    };
    let mut normalized = PathBuf::new();
    for component in abs.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            other => {
                normalized.push(other.as_os_str());
            }
        }
    }
    normalized
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecentFiles {
    #[serde(default)]
    pub paths: Vec<PathBuf>,
}

impl RecentFiles {
    pub fn new() -> Self {
        Self { paths: Vec::new() }
    }

    /// Add a path to the recent files list.
    /// If the path is already in the list, it is moved to the front.
    /// The list is truncated to [`MAX_RECENT_FILES`].
    pub fn push(&mut self, path: impl AsRef<Path>) {
        let normalized = normalize_path(path.as_ref());
        self.paths.retain(|p| p != &normalized);
        self.paths.insert(0, normalized);
        if self.paths.len() > MAX_RECENT_FILES {
            self.paths.truncate(MAX_RECENT_FILES);
        }
    }

    /// Remove a path from the recent files list.
    pub fn remove(&mut self, path: &Path) {
        let normalized = normalize_path(path);
        self.paths.retain(|p| p != path && p != &normalized);
    }

    /// Clear all recent files.
    pub fn clear(&mut self) {
        self.paths.clear();
    }

    /// Returns true if there are no recent files.
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    /// Returns the number of recent files.
    pub fn len(&self) -> usize {
        self.paths.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use anyhow::Result;

    #[test]
    fn path_normalization_resolves_dots_and_relative() {
        let current = std::env::current_dir().unwrap();
        assert_eq!(normalize_path(Path::new("a/b/../c")), current.join("a/c"));
        assert_eq!(
            normalize_path(Path::new("/x/y/./z/../w")),
            PathBuf::from("/x/y/w")
        );
    }

    #[test]
    fn recent_files_pushes_moves_to_front_and_truncates() {
        let mut recent = RecentFiles::new();
        assert!(recent.is_empty());

        for i in 1..=12 {
            recent.push(format!("/models/project_{i}.zipzi"));
        }
        assert_eq!(recent.len(), MAX_RECENT_FILES);
        assert_eq!(recent.paths[0], PathBuf::from("/models/project_12.zipzi"));
        assert_eq!(
            recent.paths[MAX_RECENT_FILES - 1],
            PathBuf::from("/models/project_3.zipzi")
        );

        // Push existing file moves it to the front
        recent.push("/models/project_5.zipzi");
        assert_eq!(recent.len(), MAX_RECENT_FILES);
        assert_eq!(recent.paths[0], PathBuf::from("/models/project_5.zipzi"));

        // Remove
        recent.remove(Path::new("/models/project_5.zipzi"));
        assert_eq!(recent.len(), MAX_RECENT_FILES - 1);
        assert_ne!(recent.paths[0], PathBuf::from("/models/project_5.zipzi"));

        // Clear
        recent.clear();
        assert!(recent.is_empty());
    }

    #[test]
    fn recent_files_roundtrip_with_config_store() -> Result<()> {
        use crate::ConfigStore;
        let dir = tempfile::tempdir()?;
        let store = ConfigStore::new(dir.path());

        let mut initial = RecentFiles::new();
        initial.push("/models/a.zipzi");
        initial.push("/models/b.zipzi");
        store.write("recent-files.json", &initial)?;

        let loaded: Option<RecentFiles> = store.read("recent-files.json")?;
        assert_eq!(loaded, Some(initial));

        Ok(())
    }
}
