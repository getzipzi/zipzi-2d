//! Publication primitives shared by project saves and package exports.
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zipzi_core::Status;

/// Injectable commit operations. A successful write_new must include file synchronization.
/// Implementations must not report a rename failure after publishing the destination.
pub trait FileSystem: Send + Sync {
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()>;
    fn sync_directory(&self, path: &Path) -> io::Result<()>;
    fn rename(&self, from: &Path, to: &Path) -> io::Result<()>;
}

#[derive(Default)]
pub struct NativeFileSystem;

impl FileSystem for NativeFileSystem {
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(bytes)?;
        file.sync_all()
    }

    fn sync_directory(&self, path: &Path) -> io::Result<()> {
        #[cfg(unix)]
        {
            File::open(path)?.sync_all()
        }
        #[cfg(not(unix))]
        {
            let _ = path;
            Ok(())
        }
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        fs::rename(from, to)
    }
}

pub(crate) fn io_error(error: io::Error) -> Status {
    Status::error("PROJECT_IO", error.to_string())
}

pub(crate) fn reject_symlink(path: &Path) -> Result<(), Status> {
    match fs::symlink_metadata(path) {
        Ok(info) if info.file_type().is_symlink() => Err(Status::error(
            "INVALID_PATH",
            format!("Symlink is not allowed: {}", path.display()),
        )),
        Ok(_) => Ok(()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_error(e)),
    }
}

/// Resolve existing ancestors, including aliases, without requiring the final path to exist.
pub(crate) fn local_path(path: &Path) -> Result<PathBuf, Status> {
    if !path.is_absolute()
        || path.to_string_lossy().contains("://")
        || path.to_string_lossy().contains('\0')
    {
        return Err(Status::error(
            "INVALID_PATH",
            "A native absolute path is required",
        ));
    }
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => (),
            // A Windows prefix such as `\\?\C:` is not a path to inspect by
            // itself. Wait for RootDir before accessing the filesystem.
            Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            Component::ParentDir => {
                result.pop();
            }
            other => {
                result.push(other.as_os_str());
                match fs::symlink_metadata(&result) {
                    Ok(_) => {
                        result = fs::canonicalize(&result).map_err(|error| {
                            Status::error(
                                "PROJECT_IO",
                                format!("Cannot resolve {}: {error}", result.display()),
                            )
                        })?;
                    }
                    Err(e) if e.kind() == io::ErrorKind::NotFound => (),
                    Err(e) => {
                        return Err(Status::error(
                            "PROJECT_IO",
                            format!("Cannot inspect {}: {e}", result.display()),
                        ));
                    }
                }
            }
        }
    }
    Ok(result)
}

pub(crate) fn lock(directory: &Path) -> Result<File, Status> {
    let path = directory.join(".zipzi.lock");
    reject_symlink(&path)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    // Match the existing C++ lock protocol: flock on Unix, exclusive handle on Windows.
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.share_mode(0);
    }
    let file = options
        .open(path)
        .map_err(|e| Status::error("PROJECT_BUSY", e.to_string()))?;
    #[cfg(not(windows))]
    file.try_lock()
        .map_err(|e| Status::error("PROJECT_BUSY", e.to_string()))?;
    // Never unlink: all writers must continue locking the same inode.
    Ok(file)
}

pub(crate) fn unique_name() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    format!(
        "{}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

pub(crate) struct Stage(pub PathBuf);
impl Stage {
    pub fn preserve(mut self) -> PathBuf {
        std::mem::take(&mut self.0)
    }
    pub fn new(parent: &Path) -> Result<Self, Status> {
        let path = parent.join(format!(".zipzi-stage-{}", unique_name()));
        fs::create_dir(&path).map_err(io_error)?;
        Ok(Self(path))
    }
}
impl Drop for Stage {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[derive(Debug, Default)]
pub struct Publication {
    pub warnings: Vec<String>,
    /// Content conversion notes; these do not indicate a durability failure.
    pub notices: Vec<String>,
}
impl Publication {
    pub fn durable(&self) -> bool {
        self.warnings.is_empty()
    }

    pub(crate) fn finish(files: &dyn FileSystem, parent: &Path) -> Self {
        let mut result = Self::default();
        if let Err(e) = files.sync_directory(parent) {
            result.warnings.push(format!(
                "Published, but directory synchronization failed: {e}"
            ));
        }
        #[cfg(not(unix))]
        result
            .warnings
            .push("Published; directory synchronization is not available on this platform".into());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn existing_case_aliases_resolve_to_same_project_path() {
        let folder = tempfile::tempdir().unwrap();
        let directory = folder.path().join("MixedCase 中文 Project");
        fs::create_dir(&directory).unwrap();
        let alias = folder.path().join("mixedcase 中文 project");
        assert_eq!(local_path(&directory).unwrap(), local_path(&alias).unwrap());
        assert!(local_path(Path::new("C:relative.zipzi")).is_err());
        assert!(local_path(Path::new("relative.zipzi")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn occupied_destination_preserves_both_files_until_retry() {
        use std::os::windows::fs::OpenOptionsExt;
        let folder = tempfile::tempdir().unwrap();
        let old = folder.path().join("old 中文");
        let new = folder.path().join("new 中文");
        fs::write(&old, b"old").unwrap();
        NativeFileSystem.write_new(&new, b"new").unwrap();
        let occupied = OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&old)
            .unwrap();
        assert!(NativeFileSystem.rename(&new, &old).is_err());
        assert_eq!(fs::read(&new).unwrap(), b"new");
        drop(occupied);
        assert_eq!(fs::read(&old).unwrap(), b"old");
        NativeFileSystem.rename(&new, &old).unwrap();
        assert_eq!(fs::read(&old).unwrap(), b"new");
        assert!(!new.exists());
        assert!(!Publication::finish(&NativeFileSystem, folder.path()).durable());
    }

    #[cfg(windows)]
    #[test]
    fn symlink_rejection_when_creation_is_permitted() {
        let folder = tempfile::tempdir().unwrap();
        let target = folder.path().join("target");
        let link = folder.path().join("link");
        fs::create_dir(&target).unwrap();
        match std::os::windows::fs::symlink_dir(&target, &link) {
            Ok(()) => {}
            Err(error) if error.raw_os_error() == Some(1314) => {
                eprintln!(
                    "MISSING COVERAGE: Windows symlink creation needs Developer Mode or privilege"
                );
                return;
            }
            Err(error) => panic!("create symlink: {error}"),
        }
        assert!(reject_symlink(&link).is_err());
        assert_eq!(local_path(&link).unwrap(), local_path(&target).unwrap());
    }

    #[test]
    #[cfg(windows)]
    fn local_path_resolves_windows_drive_and_verbatim_prefixes() {
        let directory = std::env::temp_dir();
        let canonical = fs::canonicalize(&directory).unwrap();
        let suffix = format!("zipzi-missing-{}", unique_name());
        for root in [directory, canonical.clone()] {
            assert_eq!(
                local_path(&root.join(&suffix).join("project.zipzi.json")).unwrap(),
                canonical.join(&suffix).join("project.zipzi.json"),
            );
        }
    }

    #[test]
    fn project_lock_child_probe() {
        let Some(path) = std::env::var_os("ZIPZI_TEST_PROJECT_LOCK") else {
            return;
        };
        let acquired = lock(Path::new(&path)).is_ok();
        assert_eq!(
            acquired,
            std::env::var_os("ZIPZI_TEST_EXPECT_LOCK").is_some()
        );
    }

    #[test]
    fn project_lock_excludes_another_process_then_releases() {
        let folder = tempfile::tempdir().unwrap();
        let directory = folder.path().join("中文 project");
        fs::create_dir(&directory).unwrap();
        let run = |expect_lock: bool| {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "filesystem::tests::project_lock_child_probe"])
                .env("ZIPZI_TEST_PROJECT_LOCK", &directory)
                .env_remove("ZIPZI_TEST_EXPECT_LOCK");
            if expect_lock {
                command.env("ZIPZI_TEST_EXPECT_LOCK", "1");
            }
            let output = command.output().unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
        };
        let held = lock(&directory).unwrap();
        run(false);
        drop(held);
        run(true);
        assert!(directory.join(".zipzi.lock").is_file());
    }
}
