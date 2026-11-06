use super::*;
use crate::{NativeFileSystem, ResourceStore, ResourceView};
use std::{
    path::Path,
    sync::atomic::{AtomicUsize, Ordering},
};
use zipzi_core::{state::*, Mesh, ModelSetup, Vec2};

#[derive(Clone)]
enum Fault {
    Write,
    FileSync,
    DirectorySync(usize),
    Rename(Vec<usize>),
}
struct Files {
    fault: Fault,
    syncs: AtomicUsize,
    renames: AtomicUsize,
}
impl Files {
    fn new(fault: Fault) -> Self {
        Self {
            fault,
            syncs: 0.into(),
            renames: 0.into(),
        }
    }
}
impl FileSystem for Files {
    fn write_new(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        match self.fault {
            Fault::Write => {
                NativeFileSystem.write_new(path, &bytes[..bytes.len() / 2])?;
                Err(std::io::Error::other("short write"))
            }
            Fault::FileSync => {
                fs::write(path, bytes)?;
                Err(std::io::Error::other("file sync"))
            }
            _ => NativeFileSystem.write_new(path, bytes),
        }
    }
    fn sync_directory(&self, path: &Path) -> std::io::Result<()> {
        let n = self.syncs.fetch_add(1, Ordering::SeqCst) + 1;
        if matches!(self.fault,Fault::DirectorySync(at) if at==n) {
            return Err(std::io::Error::other("directory sync"));
        }
        NativeFileSystem.sync_directory(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        let n = self.renames.fetch_add(1, Ordering::SeqCst) + 1;
        if matches!(&self.fault,Fault::Rename(at) if at.contains(&n)) {
            return Err(std::io::Error::other("rename"));
        }
        NativeFileSystem.rename(from, to)
    }
}
fn plan() -> ExportPlan {
    let id = |n| format!("00000000-0000-4000-8000-{n:012x}");
    let image = ResourceStore::default()
        .prepare_rgba(2, 2, vec![255; 16].into())
        .unwrap();
    let resources = ResourceView::default().with_images([image.clone()]);
    let mut b = DocumentState::new(id(1), ModelSetup::default())
        .unwrap()
        .builder();
    b.create_asset(AssetRecord {
        id: id(2),
        name: "art".into(),
        content: AssetContent::Image {
            resource: image.key(),
            width: 2,
            height: 2,
        },
    })
    .unwrap();
    b.create_mesh(Mesh {
        id: id(3),
        runtime_id: "Mesh".into(),
        texture_asset_id: id(2).into(),
        vertex_ids: vec![1, 2, 3],
        base_positions: vec![Vec2::new(0., 0.), Vec2::new(2., 0.), Vec2::new(0., 2.)],
        uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
        triangles: vec![[1, 2, 3]],
        ..Default::default()
    })
    .unwrap();
    build_export_plan(&b.freeze().unwrap().state, &resources).unwrap()
}
fn validated(_: &ExportPlan) -> Result<PackageValidation, Status> {
    Ok(PackageValidation::structural(
        RuntimeValidation::NotPerformed,
    ))
}

#[test]
fn package_failures_rollback_and_report_recovery_copy_when_rollback_fails() {
    let plan = plan();
    for fault in [
        Fault::Write,
        Fault::FileSync,
        Fault::DirectorySync(1),
        Fault::DirectorySync(2),
        Fault::Rename(vec![1]),
        Fault::Rename(vec![2]),
        Fault::Rename(vec![2, 3]),
    ] {
        let rollback_fails = matches!(&fault,Fault::Rename(v) if v.len()==2);
        let tmp = tempfile::tempdir().unwrap();
        let destination = tmp.path().join("package");
        publish_export_plan(&plan, &destination, &validated).unwrap();
        fs::write(destination.join("old-marker"), b"original").unwrap();
        let error = publish_plan_with_filesystem(
            &plan,
            &destination,
            &validated,
            &Files::new(fault),
            false,
        )
        .unwrap_err();
        let retained = if rollback_fails {
            assert_eq!(error.code, "ROLLBACK_FAILED");
            PathBuf::from(error.message.split("recover from ").nth(1).unwrap())
        } else {
            destination
        };
        assert_eq!(fs::read(retained.join("old-marker")).unwrap(), b"original");
    }
}

#[test]
fn package_postpublication_sync_error_is_a_warning() {
    let tmp = tempfile::tempdir().unwrap();
    let destination = tmp.path().join("package");
    let receipt = publish_plan_with_filesystem(
        &plan(),
        &destination,
        &validated,
        &Files::new(Fault::DirectorySync(3)),
        false,
    )
    .unwrap();
    assert!(!receipt.durable());
    assert!(!receipt.warnings.is_empty());
    assert!(destination.join("model.moc3").is_file());
}
