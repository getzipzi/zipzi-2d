//! Injectable streaming publication primitives. A publish implementation must
//! never report failure after installing the target.
use std::{
    fs::{File, OpenOptions},
    io::{Seek, Write},
    path::{Path, PathBuf},
};

pub trait ArchiveStream: Write + Seek {}
impl<T: Write + Seek> ArchiveStream for T {}

pub trait ArchiveFileSystem: Send + Sync {
    fn create_staging(&self, parent: &Path) -> std::io::Result<ArchiveStaging>;
    fn staging_stream<'a>(&self, file: &'a mut File) -> Box<dyn ArchiveStream + 'a> {
        Box::new(file)
    }
    fn sync_file(&self, file: &File) -> std::io::Result<()>;
    fn publish_no_replace(&self, staging: &Path, target: &Path) -> std::io::Result<()>;
    fn publish_replace(&self, staging: &Path, target: &Path) -> std::io::Result<()>;
    fn sync_directory(&self, parent: &Path) -> std::io::Result<()>;
    fn acquire_lock(&self, target: &Path) -> std::io::Result<ArchiveLock>;
}

pub struct ArchiveStaging {
    pub path: PathBuf,
    pub file: File,
    // TempPath removes an unpublished staging file after the File closes.
    _cleanup: tempfile::TempPath,
    _owner_record: tempfile::TempPath,
}

pub struct ArchiveLock {
    _file: File,
}

#[derive(Default)]
pub struct NativeArchiveFileSystem;

impl ArchiveFileSystem for NativeArchiveFileSystem {
    fn create_staging(&self, parent: &Path) -> std::io::Result<ArchiveStaging> {
        // The selected save directory is known only when I/O starts. Reclaim
        // confirmed leftovers here, off the UI thread, before allocating a stage.
        // Maintenance cannot turn a valid save into a failure or durability warning.
        match Self::clean_abandoned_staging(parent) {
            Ok(warnings) => {
                for warning in warnings {
                    eprintln!("Project staging cleanup: {warning}");
                }
            }
            Err(error) => eprintln!("Project staging cleanup: {error}"),
        }
        let stage = tempfile::Builder::new()
            .prefix(".zipzi-stage-")
            .suffix(".zipzi")
            .tempfile_in(parent)?;
        stage
            .as_file()
            .try_lock()
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let name = stage
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let owner_path = parent.join(format!("{name}.owner.json"));
        let mut owner = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&owner_path)?;
        let owner_record = tempfile::TempPath::try_from_path(owner_path)?;
        serde_json::to_writer(
            &mut owner,
            &serde_json::json!({"format": "zipzi-archive-stage", "version": 1, "path": name}),
        )?;
        owner.sync_all()?;
        let (file, cleanup) = stage.into_parts();
        Ok(ArchiveStaging {
            path: cleanup.to_path_buf(),
            file,
            _cleanup: cleanup,
            _owner_record: owner_record,
        })
    }

    fn sync_file(&self, file: &File) -> std::io::Result<()> {
        file.sync_all()
    }

    fn publish_no_replace(&self, staging: &Path, target: &Path) -> std::io::Result<()> {
        // A hard-link install is an atomic no-replace operation on the local
        // filesystems supported by Zipzi. The stage and target share a parent.
        std::fs::hard_link(staging, target)?;
        let _ = std::fs::remove_file(staging);
        Ok(())
    }

    fn publish_replace(&self, staging: &Path, target: &Path) -> std::io::Result<()> {
        std::fs::rename(staging, target)
    }

    fn sync_directory(&self, parent: &Path) -> std::io::Result<()> {
        #[cfg(unix)]
        {
            File::open(parent)?.sync_all()
        }
        #[cfg(not(unix))]
        {
            let _ = parent;
            Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "directory synchronization unavailable on this platform",
            ))
        }
    }

    fn acquire_lock(&self, target: &Path) -> std::io::Result<ArchiveLock> {
        let parent = target
            .parent()
            .ok_or_else(|| std::io::Error::other("missing project parent"))?;
        std::fs::create_dir_all(parent)?;
        let name = target
            .file_name()
            .ok_or_else(|| std::io::Error::other("missing project filename"))?;
        let mut lock_name = std::ffi::OsString::from(".");
        lock_name.push(name);
        lock_name.push(".lock");
        let lock_path = parent.join(lock_name);
        crate::filesystem::reject_symlink(&lock_path)
            .map_err(|status| std::io::Error::other(status.message))?;
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        let file = options.open(lock_path)?;
        #[cfg(not(windows))]
        {
            file.try_lock()
                .map(|()| ArchiveLock { _file: file })
                .map_err(|error| std::io::Error::other(error.to_string()))
        }
        #[cfg(windows)]
        {
            Ok(ArchiveLock { _file: file })
        }
    }
}

impl NativeArchiveFileSystem {
    /// Best-effort maintenance for one explicitly selected project parent.
    /// Never infer ownership from a filename alone or remove a project lock.
    pub fn clean_abandoned_staging(parent: &Path) -> std::io::Result<Vec<String>> {
        let root = std::fs::canonicalize(parent)?;
        let mut warnings = Vec::new();
        for child in std::fs::read_dir(&root)? {
            let child = child?;
            let name = child.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            let Some(stage_name) = name.strip_suffix(".owner.json") else {
                continue;
            };
            if !stage_name.starts_with(".zipzi-stage-")
                || !stage_name.ends_with(".zipzi")
                || !child.file_type()?.is_file()
                || child.metadata()?.len() > 1024
            {
                continue;
            }
            let Ok(bytes) = std::fs::read(child.path()) else {
                continue;
            };
            let Ok(record) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
                continue;
            };
            if record
                != serde_json::json!({"format": "zipzi-archive-stage", "version": 1, "path": stage_name})
            {
                continue;
            }
            let stage_path = root.join(stage_name);
            if std::fs::symlink_metadata(&stage_path)
                .map_or(true, |m| !m.is_file() || m.file_type().is_symlink())
            {
                continue;
            }
            let Ok(stage) = OpenOptions::new().read(true).write(true).open(&stage_path) else {
                continue;
            };
            if stage.try_lock().is_err() {
                continue;
            }
            // Retire under the owned root while holding the lock. Native staging
            // handles permit delete sharing on Windows; active owners stay locked.
            let retired = root.join(format!(
                ".zipzi-retired-{}",
                crate::filesystem::unique_name()
            ));
            if let Err(e) = std::fs::rename(&stage_path, &retired) {
                warnings.push(e.to_string());
                continue;
            }
            drop(stage);
            if let Err(e) =
                std::fs::remove_file(retired).and_then(|_| std::fs::remove_file(child.path()))
            {
                warnings.push(e.to_string());
            }
        }
        Ok(warnings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn abandoned_staging_cleanup_requires_metadata_and_released_owner_lock() {
        let root = tempfile::tempdir().unwrap();
        let live = NativeArchiveFileSystem.create_staging(root.path()).unwrap();
        let stale = NativeArchiveFileSystem.create_staging(root.path()).unwrap();
        let ArchiveStaging {
            file,
            _cleanup,
            _owner_record,
            ..
        } = stale;
        let stale_path = _cleanup.keep().unwrap();
        let record_path = _owner_record.keep().unwrap();
        drop(file);
        let foreign = root.path().join(".zipzi-stage-foreign.zipzi");
        std::fs::write(&foreign, b"foreign").unwrap();
        assert!(
            NativeArchiveFileSystem::clean_abandoned_staging(root.path())
                .unwrap()
                .is_empty()
        );
        assert!(live.path.exists());
        assert!(!stale_path.exists());
        assert!(!record_path.exists());
        assert!(foreign.exists());
    }
}
