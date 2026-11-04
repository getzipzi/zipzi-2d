//! Private immutable resources, lifetime ownership and a host-shareable disk budget.
use super::{invalid, ArchiveError, Result};
use std::{
    fs::{self, File, OpenOptions},
    path::Path,
    sync::{Arc, Mutex},
};
use tempfile::TempDir;

#[derive(Clone, Debug)]
pub struct ProjectDiskBudget(Arc<Mutex<DiskUsage>>);
#[derive(Debug)]
struct DiskUsage {
    limit: u64,
    used: u64,
}
impl ProjectDiskBudget {
    pub fn new(bytes: u64) -> Self {
        Self(Arc::new(Mutex::new(DiskUsage {
            limit: bytes,
            used: 0,
        })))
    }
    pub fn used_bytes(&self) -> u64 {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).used
    }
    pub(super) fn reserve(&self, bytes: u64) -> Result<DiskReservation> {
        let mut usage = self.0.lock().unwrap_or_else(|p| p.into_inner());
        usage.used = usage
            .used
            .checked_add(bytes)
            .filter(|n| *n <= usage.limit)
            .ok_or(ArchiveError::Limit(
                "working set disk budget; close unused sessions or increase the host budget",
            ))?;
        Ok(DiskReservation {
            budget: self.clone(),
            bytes,
        })
    }
}
impl Default for ProjectDiskBudget {
    fn default() -> Self {
        Self::new(32 << 30)
    }
}
pub(super) struct DiskReservation {
    budget: ProjectDiskBudget,
    bytes: u64,
}
impl Drop for DiskReservation {
    fn drop(&mut self) {
        self.budget.0.lock().unwrap_or_else(|p| p.into_inner()).used -= self.bytes;
    }
}
struct WorkingSet {
    // Handles close before TempDir removes files (required on Windows).
    _owner: Option<File>,
    directory: TempDir,
    budget: ProjectDiskBudget,
    reservations: Mutex<Vec<DiskReservation>>,
}
#[derive(Clone)]
pub struct ProjectResourceLease {
    inner: Arc<WorkingSet>,
}
impl ProjectResourceLease {
    pub fn create(parent: &Path, budget: ProjectDiskBudget) -> Result<Self> {
        if !parent.is_absolute() {
            return Err(invalid("Working set parent must be absolute"));
        }
        fs::create_dir_all(parent)?;
        let directory = tempfile::Builder::new()
            .prefix("zipzi-working-")
            .tempdir_in(parent)?;
        let owner = owner_lock(&directory.path().join("owner.lock"))?;
        fs::write(
            directory.path().join("owner.json"),
            b"{\"format\":\"zipzi-private-working-set\",\"version\":1}\n",
        )?;
        fs::create_dir(directory.path().join("assets"))?;
        Ok(Self {
            inner: Arc::new(WorkingSet {
                _owner: Some(owner),
                directory,
                budget,
                reservations: Mutex::new(Vec::new()),
            }),
        })
    }
    pub fn path(&self) -> &Path {
        self.inner.directory.path()
    }
    pub(super) fn reserve(&self, bytes: u64) -> Result<DiskReservation> {
        self.inner.budget.reserve(bytes)
    }
    pub(super) fn retain(&self, reservation: DiskReservation) {
        self.inner
            .reservations
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(reservation);
    }
    /// Explicit host maintenance. Only recognized, unlocked, direct children of
    /// this root are candidates; arbitrary temporary directories are never touched.
    pub fn clean_abandoned(parent: &Path) -> Result<Vec<String>> {
        let root = fs::canonicalize(parent)?;
        let mut warnings = Vec::new();
        for child in fs::read_dir(&root)? {
            let child = child?;
            if !child
                .file_name()
                .to_string_lossy()
                .starts_with("zipzi-working-")
                || !child.file_type()?.is_dir()
            {
                continue;
            }
            let path = child.path();
            let metadata = path.join("owner.json");
            let lock = path.join("owner.lock");
            if [metadata.as_path(), lock.as_path()].iter().any(|p| {
                fs::symlink_metadata(p).map_or(true, |m| {
                    !m.is_file() || m.file_type().is_symlink() || m.len() > 1024
                })
            }) {
                continue;
            }
            if fs::read(&metadata).ok().as_deref()
                != Some(b"{\"format\":\"zipzi-private-working-set\",\"version\":1}\n")
            {
                continue;
            }
            let Ok(owner) = owner_lock(&lock) else {
                continue;
            };
            // Keep ownership locked while retiring the directory. Rename is
            // within the configured root and avoids claiming a newly reused path.
            let retired = root.join(format!(
                ".zipzi-retired-{}",
                crate::filesystem::unique_name()
            ));
            if let Err(e) = fs::rename(&path, &retired) {
                warnings.push(e.to_string());
                continue;
            }
            drop(owner);
            if let Err(e) = fs::remove_dir_all(retired) {
                warnings.push(e.to_string());
            }
        }
        Ok(warnings)
    }
}
fn owner_lock(path: &Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(4);
    }
    let file = options.open(path)?;
    #[cfg(not(windows))]
    file.try_lock()
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn budget_is_shared_and_final_lease_releases_directory_and_reservation() {
        let root = tempfile::tempdir().unwrap();
        let budget = ProjectDiskBudget::new(1024);
        let first = ProjectResourceLease::create(root.path(), budget.clone()).unwrap();
        let second = ProjectResourceLease::create(root.path(), budget.clone()).unwrap();
        first.retain(first.reserve(800).unwrap());
        assert!(second.reserve(225).is_err());
        let temporary = second.reserve(224).unwrap();
        assert_eq!(budget.used_bytes(), 1024);
        drop(temporary);
        let path = first.path().to_path_buf();
        let snapshot = first.clone();
        drop(first);
        assert!(path.exists());
        assert_eq!(budget.used_bytes(), 800);
        drop(snapshot);
        assert!(!path.exists());
        assert_eq!(budget.used_bytes(), 0);
    }
    #[test]
    fn cleanup_only_removes_recognized_unlocked_owned_directories() {
        let root = tempfile::tempdir().unwrap();
        let live = ProjectResourceLease::create(root.path(), ProjectDiskBudget::default()).unwrap();
        let abandoned =
            ProjectResourceLease::create(root.path(), ProjectDiskBudget::default()).unwrap();
        // Simulate process death: keep the on-disk files while dropping its lock.
        let inner = Arc::try_unwrap(abandoned.inner).ok().unwrap();
        let WorkingSet {
            _owner, directory, ..
        } = inner;
        let stale = directory.keep();
        drop(_owner);
        let foreign = root.path().join("zipzi-working-foreign");
        fs::create_dir(&foreign).unwrap();
        let unrelated = root.path().join("unrelated");
        fs::create_dir(&unrelated).unwrap();
        assert!(ProjectResourceLease::clean_abandoned(root.path())
            .unwrap()
            .is_empty());
        assert!(live.path().exists());
        assert!(!stale.exists());
        assert!(foreign.exists());
        assert!(unrelated.exists());
        #[cfg(unix)]
        {
            let outside = tempfile::tempdir().unwrap();
            let alias = root.path().join("zipzi-working-symlink");
            std::os::unix::fs::symlink(outside.path(), alias).unwrap();
            ProjectResourceLease::clean_abandoned(root.path()).unwrap();
            assert!(outside.path().exists());
        }
    }
}
