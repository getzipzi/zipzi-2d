use super::tests::{fixture, native_bytes};
use super::*;
use std::{
    io::{self, SeekFrom},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Copy, PartialEq)]
enum Fault {
    None,
    Create,
    Write,
    Finalize,
    Sync,
    Publish,
    Directory,
    Race,
    ExternalChange,
    CancelAfterPublish,
}
struct Injected {
    fault: Fault,
    target: PathBuf,
    cancelled: Arc<AtomicBool>,
}
struct Stream<'a> {
    file: &'a mut File,
    fault: Fault,
}
impl Write for Stream<'_> {
    fn write(&mut self, b: &[u8]) -> io::Result<usize> {
        if self.fault == Fault::Write
            || (self.fault == Fault::Finalize && b.starts_with(b"PK\x01\x02"))
        {
            return Err(io::Error::other("injected stream failure"));
        }
        self.file.write(b)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
impl Seek for Stream<'_> {
    fn seek(&mut self, p: SeekFrom) -> io::Result<u64> {
        self.file.seek(p)
    }
}
impl ArchiveFileSystem for Injected {
    fn create_staging(&self, parent: &Path) -> io::Result<ArchiveStaging> {
        if self.fault == Fault::Create {
            return Err(io::Error::other("injected create failure"));
        }
        NativeArchiveFileSystem.create_staging(parent)
    }
    fn staging_stream<'a>(&self, file: &'a mut File) -> Box<dyn ArchiveStream + 'a> {
        Box::new(Stream {
            file,
            fault: self.fault,
        })
    }
    fn sync_file(&self, file: &File) -> io::Result<()> {
        if self.fault == Fault::Sync {
            return Err(io::Error::other("injected sync failure"));
        }
        if self.fault == Fault::ExternalChange {
            fs::write(&self.target, b"changed by another writer")?;
        }
        NativeArchiveFileSystem.sync_file(file)
    }
    fn publish_no_replace(&self, stage: &Path, target: &Path) -> io::Result<()> {
        if self.fault == Fault::Race {
            fs::write(target, b"racing creator")?;
        }
        if self.fault == Fault::Publish {
            return Err(io::Error::other("injected publish failure"));
        }
        NativeArchiveFileSystem.publish_no_replace(stage, target)?;
        if self.fault == Fault::CancelAfterPublish {
            self.cancelled.store(true, Ordering::SeqCst);
        }
        Ok(())
    }
    fn publish_replace(&self, stage: &Path, target: &Path) -> io::Result<()> {
        if self.fault == Fault::Publish {
            return Err(io::Error::other("injected publish failure"));
        }
        NativeArchiveFileSystem.publish_replace(stage, target)?;
        if self.fault == Fault::CancelAfterPublish {
            self.cancelled.store(true, Ordering::SeqCst);
        }
        Ok(())
    }
    fn sync_directory(&self, parent: &Path) -> io::Result<()> {
        if self.fault == Fault::Directory {
            return Err(io::Error::other("injected directory sync failure"));
        }
        NativeArchiveFileSystem.sync_directory(parent)
    }
    fn acquire_lock(&self, target: &Path) -> io::Result<ArchiveLock> {
        NativeArchiveFileSystem.acquire_lock(target)
    }
}
fn stage_count(root: &Path) -> usize {
    fs::read_dir(root)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(".zipzi-stage-"))
        .count()
}

#[test]
fn failures_before_publication_preserve_old_target_and_remove_staging() {
    let root = tempfile::tempdir().unwrap();
    let doc = fixture(root.path());
    let input = root.path().join("input.zipzi");
    fs::write(&input, native_bytes(&doc, root.path())).unwrap();
    let opened = ProjectArchiveStore::new(root.path())
        .open(&input, || true)
        .unwrap();
    let target = root.path().join("目标 with spaces.zipzi");
    for existing in [false, true] {
        for fault in [
            Fault::Create,
            Fault::Write,
            Fault::Finalize,
            Fault::Sync,
            Fault::Publish,
        ] {
            let baseline = if existing {
                fs::write(&target, b"old complete archive").unwrap();
                Some(
                    ProjectArchiveStore::new(root.path())
                        .fingerprint(&target, || true)
                        .unwrap(),
                )
            } else {
                let _ = fs::remove_file(&target);
                None
            };
            let injected = Injected {
                fault,
                target: target.clone(),
                cancelled: Arc::new(AtomicBool::new(false)),
            };
            let store = ProjectArchiveStore::new(root.path()).with_filesystem(Arc::new(injected));
            assert!(store
                .save(
                    &opened.document,
                    &opened.resource_lease(),
                    &target,
                    baseline.as_ref(),
                    || true
                )
                .is_err());
            if existing {
                assert_eq!(fs::read(&target).unwrap(), b"old complete archive");
            } else {
                assert!(!target.exists());
            }
            assert_eq!(stage_count(root.path()), 0);
        }
    }
}

#[test]
fn cancellation_directory_failure_and_no_replace_race_obey_commit_boundary() {
    let root = tempfile::tempdir().unwrap();
    let doc = fixture(root.path());
    let input = root.path().join("input.zipzi");
    fs::write(&input, native_bytes(&doc, root.path())).unwrap();
    let opened = ProjectArchiveStore::new(root.path())
        .open(&input, || true)
        .unwrap();
    for fault in [
        Fault::None,
        Fault::Directory,
        Fault::Race,
        Fault::CancelAfterPublish,
    ] {
        let target = root.path().join("save.zipzi");
        let _ = fs::remove_file(&target);
        let cancelled = Arc::new(AtomicBool::new(false));
        let store = ProjectArchiveStore::new(root.path()).with_filesystem(Arc::new(Injected {
            fault,
            target: target.clone(),
            cancelled: cancelled.clone(),
        }));
        let result = store.save(
            &opened.document,
            &opened.resource_lease(),
            &target,
            None,
            || !cancelled.load(Ordering::SeqCst),
        );
        if fault == Fault::Race {
            assert!(result.is_err());
            assert_eq!(fs::read(&target).unwrap(), b"racing creator");
        } else {
            let receipt = result.unwrap();
            assert!(receipt.published);
            assert_eq!(receipt.durable, cfg!(unix) && fault != Fault::Directory);
            assert_eq!(
                receipt.fingerprint,
                store.fingerprint(&target, || true).unwrap()
            );
        }
        assert_eq!(stage_count(root.path()), 0);
    }
    let target = root.path().join("cancelled.zipzi");
    let mut count = 0;
    assert!(ProjectArchiveStore::new(root.path())
        .save(
            &opened.document,
            &opened.resource_lease(),
            &target,
            None,
            || {
                count += 1;
                count < 8
            }
        )
        .is_err());
    assert!(!target.exists());
    assert_eq!(stage_count(root.path()), 0);
}

#[test]
fn full_fingerprint_is_rechecked_after_sync_and_deleted_baselines_conflict() {
    let root = tempfile::tempdir().unwrap();
    let doc = fixture(root.path());
    let target = root.path().join("input.zipzi");
    fs::write(&target, native_bytes(&doc, root.path())).unwrap();
    let store = ProjectArchiveStore::new(root.path());
    let opened = store.open(&target, || true).unwrap();
    let store = store.with_filesystem(Arc::new(Injected {
        fault: Fault::ExternalChange,
        target: target.clone(),
        cancelled: Arc::new(AtomicBool::new(false)),
    }));
    assert!(
        matches!(store.save(&opened.document, &opened.resource_lease(), &target, Some(&opened.fingerprint), || true), Err(ArchiveError::Document {code, ..}) if code == "PROJECT_CONFLICT")
    );
    assert_eq!(fs::read(&target).unwrap(), b"changed by another writer");
    fs::remove_file(&target).unwrap();
    assert!(
        matches!(store.save(&opened.document, &opened.resource_lease(), &target, Some(&opened.fingerprint), || true), Err(ArchiveError::Document {code, ..}) if code == "PROJECT_CONFLICT")
    );
    assert!(!target.exists());
}

#[test]
fn directory_symlink_and_hardlink_targets_are_never_replaced() {
    let root = tempfile::tempdir().unwrap();
    let resources =
        ProjectResourceLease::create(root.path(), ProjectDiskBudget::default()).unwrap();
    let doc = fixture(root.path());
    let target = root.path().join("directory.zipzi");
    fs::create_dir(&target).unwrap();
    let store = ProjectArchiveStore::new(root.path());
    assert!(store
        .save(&doc, &resources, &target, None, || true)
        .is_err());
    assert!(target.is_dir());
    #[cfg(unix)]
    {
        let file = root.path().join("original");
        fs::write(&file, b"original").unwrap();
        let symlink = root.path().join("symlink");
        std::os::unix::fs::symlink(&file, &symlink).unwrap();
        let hardlink = root.path().join("hardlink");
        fs::hard_link(&file, &hardlink).unwrap();
        for path in [&symlink, &hardlink] {
            assert!(store.save(&doc, &resources, path, None, || true).is_err());
        }
        assert_eq!(fs::read(file).unwrap(), b"original");
    }
}

#[test]
fn archive_lock_child_probe() {
    let Some(path) = std::env::var_os("ZIPZI_TEST_ARCHIVE_LOCK") else {
        return;
    };
    assert_eq!(
        NativeArchiveFileSystem
            .acquire_lock(Path::new(&path))
            .is_ok(),
        std::env::var_os("ZIPZI_TEST_EXPECT_LOCK").is_some()
    );
}
#[test]
fn archive_lock_has_stable_identity_across_processes() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("中文 with spaces.zipzi");
    let run = |success| {
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command
            .args([
                "--exact",
                "archive::publication_tests::archive_lock_child_probe",
            ])
            .env("ZIPZI_TEST_ARCHIVE_LOCK", &path)
            .env_remove("ZIPZI_TEST_EXPECT_LOCK");
        if success {
            command.env("ZIPZI_TEST_EXPECT_LOCK", "1");
        }
        let out = command.output().unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stdout)
        );
    };
    let held = NativeArchiveFileSystem.acquire_lock(&path).unwrap();
    run(false);
    drop(held);
    run(true);
    assert!(root.path().join(".中文 with spaces.zipzi.lock").is_file());
}
