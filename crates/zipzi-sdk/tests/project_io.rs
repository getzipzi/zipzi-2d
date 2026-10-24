use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use zipzi_core::{Canvas, Vec2};
use zipzi_project::{FileSystem, NativeFileSystem};
use zipzi_sdk::{
    prepare_png_asset, prepare_png_asset_from_base, prepare_relocated_asset, rectangle_mesh,
    AuthoringSession, ObjectKind,
};

const DOCUMENT: &str = "00000000-0000-4000-8000-000000000001";
const ASSET: &str = "00000000-0000-4000-8000-000000000002";
const MESH: &str = "00000000-0000-4000-8000-000000000003";
const LAYERED_PSD: &[u8] = include_bytes!("../../zipzi-psd/tests/fixtures/layered.psd");
static NEXT_TEMP: AtomicUsize = AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "zipzi-sdk-io-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(fs::canonicalize(path).unwrap())
    }

    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }

    fn png(&self, name: &str, color: u8) -> PathBuf {
        let path = self.path(name);
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            writer.write_image_data(&[color; 16]).unwrap();
        }
        fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn psd_draft_keeps_assets_in_memory_until_save_and_relocates_history() {
    let temp = TempDir::new();
    let source = temp.path("art.psd");
    fs::write(&source, LAYERED_PSD).unwrap();
    let mut sdk =
        AuthoringSession::new(DOCUMENT, Canvas::new(8.0, 8.0, Vec2::default(), 1.0)).unwrap();
    let receipt = sdk.import_psd_in_memory(&source, None).unwrap();
    assert!(!receipt.project.published);
    assert!(receipt.manifest.as_os_str().is_empty());
    assert!(sdk.project_path().is_none());
    assert!(sdk.modified());
    assert_eq!(fs::read_dir(&temp.0).unwrap().count(), 1);
    let asset_id = sdk.asset_ids()[0].clone();
    let mesh_id = sdk.mesh_ids()[0].clone();
    let texture = sdk.read_texture(&asset_id).unwrap();
    let texture_again = sdk.read_texture(&asset_id).unwrap();
    assert!(Arc::ptr_eq(&texture.rgba, &texture_again.rgba));
    assert!(!texture.cache_key.is_empty());
    assert!(sdk
        .asset(&asset_id)
        .unwrap()
        .content
        .as_ref()
        .unwrap()
        .sha256
        .is_empty());
    let original = sdk.read_asset(&asset_id).unwrap();
    assert_eq!(&*texture.rgba, original.rgba.as_slice());
    fs::remove_file(&source).unwrap();
    assert!(sdk.diagnose_resources().is_empty());
    sdk.edit("rename", None, |edit| edit.rename_mesh(&mesh_id, "Edited"))
        .unwrap();
    let saved = sdk.save_project(&temp.path("project"), None).unwrap();
    assert!(saved.warnings.is_empty(), "{:?}", saved.warnings);
    assert!(
        saved.history_warnings.is_empty(),
        "{:?}",
        saved.history_warnings
    );
    assert_eq!(
        sdk.asset(&asset_id)
            .unwrap()
            .content
            .as_ref()
            .unwrap()
            .sha256,
        original.sha256
    );
    assert!(!sdk.modified());
    assert_eq!(sdk.history_lengths(), (1, 0));
    assert_eq!(sdk.read_asset(&asset_id).unwrap().rgba, original.rgba);
    sdk.undo().unwrap();
    assert_eq!(sdk.mesh(&mesh_id).unwrap().name, "face");
    assert!(sdk.modified());
    assert!(sdk.diagnose_resources().is_empty());
    sdk.redo().unwrap();
    assert!(!sdk.modified());
    let mut reopened =
        AuthoringSession::new(DOCUMENT, Canvas::new(8.0, 8.0, Vec2::default(), 1.0)).unwrap();
    reopened.open_project(&saved.manifest, None).unwrap();
    assert_eq!(reopened.read_asset(&asset_id).unwrap().rgba, original.rgba);
    let asset = sdk.asset(&asset_id).unwrap();
    fs::write(
        saved
            .manifest
            .parent()
            .unwrap()
            .join(&asset.content.as_ref().unwrap().source),
        b"changed",
    )
    .unwrap();
    assert!(sdk.read_asset(&asset_id).is_err()); // Saved files cannot be hidden by the memory cache.
    assert!(sdk.read_texture(&asset_id).is_err());
}

#[test]
fn failed_psd_draft_import_and_save_preserve_memory_assets_and_history() {
    let temp = TempDir::new();
    let source = temp.path("art.psd");
    fs::write(&source, LAYERED_PSD).unwrap();
    let mut sdk =
        AuthoringSession::new(DOCUMENT, Canvas::new(8.0, 8.0, Vec2::default(), 1.0)).unwrap();
    sdk.import_psd_in_memory(&source, None).unwrap();
    let mesh_id = sdk.mesh_ids()[0].clone();
    let asset_id = sdk.asset_ids()[0].clone();
    sdk.edit("rename", None, |edit| edit.rename_mesh(&mesh_id, "Keep me"))
        .unwrap();
    let version = sdk.version();
    let handle = sdk.handle(ObjectKind::Mesh, &mesh_id).unwrap();
    let stale = zipzi_sdk::Version {
        revision: version.revision + 1,
        ..version
    };
    assert_eq!(
        sdk.import_psd_in_memory(&source, Some(stale))
            .unwrap_err()
            .code
            .as_ref(),
        "STALE_VERSION"
    );
    fs::write(&source, b"bad").unwrap();
    assert_eq!(
        sdk.import_psd_in_memory(&source, None)
            .unwrap_err()
            .code
            .as_ref(),
        "INVALID_PSD"
    );
    assert_eq!(sdk.version(), version);
    sdk.resolve_handle(&handle).unwrap();
    let blocked = temp.path("blocked");
    fs::write(&blocked, b"file").unwrap();
    assert!(sdk.save_project(&blocked.join("project"), None).is_err());
    assert!(sdk.project_path().is_none());
    assert_eq!(sdk.history_lengths(), (1, 0));
    assert_eq!(sdk.mesh(&mesh_id).unwrap().name, "Keep me");
    assert!(sdk.read_asset(&asset_id).is_ok());
    sdk.save_project(&temp.path("retry"), None).unwrap();
}

#[test]
fn psd_draft_retains_replaced_texture_for_undo_after_first_save() {
    let temp = TempDir::new();
    let source = temp.path("art.psd");
    fs::write(&source, LAYERED_PSD).unwrap();
    let mut sdk =
        AuthoringSession::new(DOCUMENT, Canvas::new(8.0, 8.0, Vec2::default(), 1.0)).unwrap();
    sdk.import_psd_in_memory(&source, None).unwrap();
    let id = sdk.asset_ids()[0].clone();
    let original = sdk.read_asset(&id).unwrap();
    let replacement =
        prepare_png_asset(&id, "replacement", &temp.png("replacement.png", 200)).unwrap();
    sdk.edit("replace", None, |edit| edit.replace_asset(replacement))
        .unwrap();
    sdk.save_project(&temp.path("project"), None).unwrap();
    sdk.undo().unwrap();
    assert_eq!(sdk.read_asset(&id).unwrap().rgba, original.rgba);
    assert!(sdk.diagnose_resources().is_empty());
    sdk.save_project(&temp.path("restored"), None).unwrap();
    sdk.redo().unwrap();
    assert_ne!(sdk.read_asset(&id).unwrap().rgba, original.rgba);
}

#[test]
fn psd_import_publishes_assets_and_replaces_session_only_on_success() {
    let temp = TempDir::new();
    let mut sdk = session(&temp.png("old.png", 10), true);
    let before = sdk.version();
    let source = temp.path("art.psd");
    let destination = temp.path("art-project");
    fs::write(&source, LAYERED_PSD).unwrap();

    assert_eq!(
        sdk.import_psd(
            &source,
            &destination,
            Some(zipzi_sdk::Version {
                revision: before.revision + 1,
                ..before
            })
        )
        .unwrap_err()
        .code
        .as_ref(),
        "STALE_VERSION"
    );
    assert!(!destination.exists());
    fs::write(&source, b"bad").unwrap();
    assert_eq!(
        sdk.import_psd(&source, &destination, None)
            .unwrap_err()
            .code
            .as_ref(),
        "INVALID_PSD"
    );
    assert_eq!(sdk.version(), before);
    assert!(sdk.mesh(MESH).is_some());
    assert!(!destination.exists());

    fs::write(&source, LAYERED_PSD).unwrap();
    let receipt = sdk.import_psd(&source, &destination, Some(before)).unwrap();
    assert_eq!(receipt.before, before);
    assert_eq!(receipt.after.generation, before.generation + 1);
    assert_eq!((receipt.report.width, receipt.report.height), (8, 8));
    assert_eq!(receipt.report.raster_layers, 1);
    assert!(receipt.project.durable);
    assert_eq!(receipt.manifest, destination.join("project.zipzi.json"));
    assert!(!sdk.modified());
    assert_eq!(sdk.history_lengths(), (0, 0));
    assert!(sdk.mesh(MESH).is_none());
    assert!(sdk.diagnose_resources().is_empty());
    let asset = sdk.asset(&sdk.asset_ids()[0]).unwrap();
    assert!(destination
        .join(&asset.content.as_ref().unwrap().source)
        .exists());
    assert_eq!(sdk.mesh(&sdk.mesh_ids()[0]).unwrap().name, "face");

    let mut reopened =
        AuthoringSession::new(DOCUMENT, Canvas::new(1.0, 1.0, Vec2::new(0.0, 0.0), 1.0)).unwrap();
    reopened.open_project(&receipt.manifest, None).unwrap();
    assert_eq!(reopened.mesh_ids(), sdk.mesh_ids());
    assert!(reopened.diagnose_resources().is_empty());

    assert_eq!(
        sdk.import_psd(&source, &destination, None)
            .unwrap_err()
            .code
            .as_ref(),
        "DESTINATION_EXISTS"
    );
    assert_eq!(sdk.version(), receipt.after);
}

struct FailPsdPublish;

impl FileSystem for FailPsdPublish {
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        NativeFileSystem.write_new(path, bytes)
    }

    fn sync_directory(&self, path: &Path) -> io::Result<()> {
        NativeFileSystem.sync_directory(path)
    }

    fn rename(&self, _from: &Path, _to: &Path) -> io::Result<()> {
        Err(io::Error::other("injected project publication failure"))
    }
}

#[test]
fn psd_import_publish_failure_preserves_session_and_destination() {
    let temp = TempDir::new();
    let source = temp.path("art.psd");
    let destination = temp.path("art-project");
    fs::write(&source, LAYERED_PSD).unwrap();
    let mut sdk = AuthoringSession::with_filesystem(
        DOCUMENT,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
        Arc::new(FailPsdPublish),
    )
    .unwrap();
    let before = sdk.version();
    assert_eq!(
        sdk.import_psd(&source, &destination, None)
            .unwrap_err()
            .code
            .as_ref(),
        "PROJECT_IO"
    );
    assert_eq!(sdk.version(), before);
    assert!(!destination.exists());
    assert!(!fs::read_dir(&temp.0).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".zipzi-stage-")
    }));
}

fn session(texture: &Path, with_mesh: bool) -> AuthoringSession {
    let mut sdk = AuthoringSession::new(
        DOCUMENT,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    let asset = prepare_png_asset(ASSET, "texture", texture).unwrap();
    sdk.edit("create", None, |edit| {
        edit.create_asset(asset)?;
        if with_mesh {
            edit.create_mesh(
                rectangle_mesh(
                    MESH,
                    "mesh",
                    ASSET,
                    Vec2::new(0.0, 0.0),
                    Vec2::new(2.0, 2.0),
                )
                .unwrap(),
            )?;
        }
        Ok(())
    })
    .unwrap();
    sdk
}

#[test]
fn mixed_gesture_commits_one_checkpoint_and_tracks_saved_baselines() {
    let temp = TempDir::new();
    let mut sdk = session(&temp.png("texture.png", 10), true);
    let path = temp.path("project");
    sdk.save_project(&path, None).unwrap();
    let original = sdk.document().clone();
    let original_mesh = sdk.mesh(MESH).unwrap();
    let before = sdk.version();
    let history = sdk.history_lengths();
    sdk.drain_events();

    let mut gesture = sdk.begin_owned_edit("drag and rename", None).unwrap();
    for step in 1..100 {
        gesture.rename_mesh(MESH, format!("name{step}")).unwrap();
        gesture
            .update_positions(MESH, &[0], &[Vec2::new(step as f32, 1.)])
            .unwrap();
    }
    // Canvas and opacity used to be barriers in the field-based history.
    let canvas = Canvas::new(200., 150., Vec2::new(50., 50.), 10.);
    gesture.replace_model_setup(canvas).unwrap();
    gesture.set_mesh_opacity(MESH, 0.4).unwrap();
    assert!(sdk.document().same_content(&original));
    assert_eq!(sdk.version(), before);
    assert_eq!(sdk.history_lengths(), history);
    assert!(!sdk.modified());
    assert!(sdk.drain_events().is_empty());

    gesture.commit_to(&mut sdk).unwrap();
    let edited = sdk.document().clone();
    assert_eq!(sdk.version().revision, before.revision + 1);
    assert_eq!(sdk.history_lengths(), (history.0 + 1, 0));
    assert_eq!(sdk.drain_events().len(), 1);
    assert_eq!(sdk.mesh(MESH).unwrap().name, "name99");
    assert_eq!(
        sdk.mesh(MESH).unwrap().base_positions[0],
        Vec2::new(99., 1.)
    );
    assert_eq!(sdk.mesh(MESH).unwrap().appearance.opacity, 0.4);
    assert_eq!(sdk.source_canvas().unwrap(), canvas);

    sdk.undo().unwrap();
    assert_eq!(sdk.mesh(MESH).unwrap(), original_mesh);
    assert!(sdk.document().same_content(&original));
    assert!(!sdk.modified());
    sdk.redo().unwrap();
    assert!(sdk.document().same_content(&edited));
    assert!(sdk.modified());

    sdk.save_project(&path, None).unwrap();
    sdk.undo().unwrap();
    assert!(sdk.modified());
    sdk.redo().unwrap();
    assert!(!sdk.modified());
    assert!(sdk.diagnose_resources().is_empty());
}

#[test]
fn cancelled_reverted_and_failed_mixed_edits_preserve_redo() {
    let temp = TempDir::new();
    let mut sdk = session(&temp.png("texture.png", 10), true);
    sdk.save_project(&temp.path("project"), None).unwrap();
    sdk.edit("rename", None, |edit| edit.rename_mesh(MESH, "redo name"))
        .unwrap();
    sdk.undo().unwrap();
    let version = sdk.version();
    let history = sdk.history_lengths();
    let original = sdk.document().clone();
    let mesh = sdk.mesh(MESH).unwrap();
    sdk.drain_events();

    let mut cancelled = sdk.begin_owned_edit("cancel", None).unwrap();
    cancelled.rename_mesh(MESH, "discarded").unwrap();
    cancelled.set_mesh_opacity(MESH, 0.2).unwrap();
    cancelled
        .update_positions(MESH, &[0], &[Vec2::new(20., 1.)])
        .unwrap();
    drop(cancelled);

    let (_, noop) = sdk
        .edit("return to origin", None, |edit| {
            edit.rename_mesh(MESH, "temporary")?;
            edit.set_mesh_opacity(MESH, 0.2)?;
            edit.update_positions(MESH, &[0], &[Vec2::new(20., 1.)])?;
            edit.rename_mesh(MESH, &mesh.name)?;
            edit.set_mesh_opacity(MESH, mesh.appearance.opacity)?;
            edit.update_positions(MESH, &[0], &[mesh.base_positions[0]])
        })
        .unwrap();
    assert!(!noop.changed);

    let failure = sdk
        .edit("invalid batch", None, |edit| {
            edit.rename_mesh(MESH, "must roll back")?;
            edit.set_mesh_opacity(MESH, 0.2)?;
            edit.update_positions(MESH, &[0, 999], &[Vec2::new(30., 1.); 2])
        })
        .unwrap_err();
    assert_eq!(failure.code.as_ref(), "MISSING_VERTEX");
    assert_eq!(sdk.version(), version);
    assert_eq!(sdk.history_lengths(), history);
    assert!(sdk.document().same_content(&original));
    assert!(!sdk.modified());
    assert!(sdk.drain_events().is_empty());
    sdk.redo().unwrap();
    assert_eq!(sdk.mesh(MESH).unwrap().name, "redo name");
    sdk.undo().unwrap();
    sdk.edit("new branch", None, |edit| edit.set_mesh_opacity(MESH, 0.5))
        .unwrap();
    assert_eq!(sdk.history_lengths().1, 0);
}

#[test]
fn publication_failure_preserves_checkpoint_history_and_saved_baseline() {
    let temp = TempDir::new();
    let path = temp.path("project");
    let mut saved = session(&temp.png("texture.png", 10), true);
    saved.save_project(&path, None).unwrap();
    let original = saved.mesh(MESH).unwrap();
    let manifest = fs::read(saved.project_path().unwrap()).unwrap();
    let mut sdk = AuthoringSession::with_filesystem(
        DOCUMENT,
        saved.source_canvas().unwrap(),
        Arc::new(FailPsdPublish),
    )
    .unwrap();
    sdk.open_project(&path, None).unwrap();
    sdk.edit("unsaved", None, |edit| {
        edit.rename_mesh(MESH, "unsaved")?;
        edit.set_mesh_opacity(MESH, 0.4)
    })
    .unwrap();
    let version = sdk.version();
    let history = sdk.history_lengths();
    sdk.drain_events();

    assert_eq!(
        sdk.save_project(&path, None).unwrap_err().code.as_ref(),
        "PROJECT_IO"
    );
    assert_eq!(sdk.version(), version);
    assert_eq!(sdk.history_lengths(), history);
    assert!(sdk.modified());
    assert!(sdk.drain_events().is_empty());
    assert_eq!(fs::read(sdk.project_path().unwrap()).unwrap(), manifest);
    sdk.undo().unwrap();
    assert_eq!(sdk.mesh(MESH).unwrap(), original);
    assert!(!sdk.modified());
    sdk.redo().unwrap();
    assert_eq!(sdk.mesh(MESH).unwrap().name, "unsaved");
    assert_eq!(sdk.mesh(MESH).unwrap().appearance.opacity, 0.4);
    assert!(sdk.modified());
    assert!(sdk.diagnose_resources().is_empty());
}

#[test]
fn save_relocates_unhashed_history_and_redo_reaches_saved_baseline() {
    let temp = TempDir::new();
    let texture = temp.png("a.png", 10);
    let mut sdk = session(&texture, true);
    let mut asset = sdk.asset(ASSET).unwrap();
    asset.content.as_mut().unwrap().sha256.clear();
    sdk.edit("legacy resource", None, |edit| edit.replace_asset(asset))
        .unwrap();
    let mesh = sdk.mesh(MESH).unwrap();
    let moved: Vec<_> = mesh
        .base_positions
        .iter()
        .map(|p| Vec2::new(p.x + 5.0, p.y))
        .collect();
    sdk.edit("move", None, |edit| {
        edit.update_positions(MESH, &mesh.vertex_ids, &moved)
    })
    .unwrap();
    let before = sdk.version();
    let receipt = sdk.save_project(&temp.path("first"), Some(before)).unwrap();
    assert_eq!(receipt.before, before);
    assert_eq!(receipt.after, sdk.version());
    assert!(receipt.manifest.exists());
    assert!(receipt.durable);
    assert_eq!(receipt.history_warnings.len(), 1);
    assert!(receipt.history_warnings[0].contains(ASSET));
    assert!(!sdk.modified());
    assert!(sdk.diagnose_resources().is_empty());
    let saved_asset = sdk.asset(ASSET).unwrap();
    assert!(saved_asset
        .content
        .as_ref()
        .unwrap()
        .source
        .starts_with("assets/"));
    assert_eq!(saved_asset.content.as_ref().unwrap().sha256.len(), 64);

    sdk.undo().unwrap();
    assert_eq!(sdk.mesh(MESH).unwrap().base_positions, mesh.base_positions);
    assert_eq!(sdk.asset(ASSET).unwrap(), saved_asset);
    assert!(sdk.modified());
    assert!(sdk.diagnose_resources().is_empty());
    sdk.redo().unwrap();
    assert_eq!(sdk.mesh(MESH).unwrap().base_positions, moved);
    assert_eq!(sdk.asset(ASSET).unwrap(), saved_asset);
    assert!(!sdk.modified());

    let expected = sdk.evaluate(&Default::default()).unwrap();
    let mut reopened = AuthoringSession::new(
        DOCUMENT,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
    )
    .unwrap();
    assert!(reopened
        .open_project(&receipt.manifest, None)
        .unwrap()
        .diagnostics
        .is_empty());
    assert_eq!(reopened.asset(ASSET), sdk.asset(ASSET));
    assert_eq!(reopened.mesh(MESH), sdk.mesh(MESH));
    assert_eq!(
        reopened.evaluate(&Default::default()).unwrap().drawables,
        expected.drawables
    );
}

#[test]
fn save_relocates_redo_branch_and_retains_saved_content() {
    let temp = TempDir::new();
    let texture = temp.png("a.png", 10);
    let mut sdk = session(&texture, true);
    sdk.save_project(&temp.path("first"), None).unwrap();
    sdk.edit("rename", None, |edit| edit.rename_mesh(MESH, "redo name"))
        .unwrap();
    sdk.undo().unwrap();
    assert_eq!(sdk.history_lengths().1, 1);
    sdk.save_project(&temp.path("second"), None).unwrap();
    let saved = sdk.asset(ASSET).unwrap();
    assert!(!sdk.modified());
    sdk.redo().unwrap();
    assert_eq!(sdk.mesh(MESH).unwrap().name, "redo name");
    assert_eq!(sdk.asset(ASSET).unwrap(), saved);
    assert!(sdk.diagnose_resources().is_empty());
    assert!(sdk.modified());
    sdk.undo().unwrap();
    assert!(!sdk.modified());
}

#[test]
fn save_as_keeps_old_resource_for_same_id_and_new_resource_for_redo() {
    let temp = TempDir::new();
    let a = temp.png("a.png", 10);
    let b = temp.png("b.png", 200);
    let mut sdk = session(&a, true);
    sdk.save_project(&temp.path("first"), None).unwrap();
    let old = sdk.asset(ASSET).unwrap();
    let replacement = prepare_png_asset(ASSET, "texture", &b).unwrap();
    sdk.edit("replace texture", None, |edit| {
        edit.replace_asset(replacement)
    })
    .unwrap();
    let second = temp.path("second");
    sdk.save_project(&second, None).unwrap();
    let saved = sdk.asset(ASSET).unwrap();
    assert_ne!(
        old.content.as_ref().unwrap().sha256,
        saved.content.as_ref().unwrap().sha256
    );
    assert_eq!(
        sdk.project_path(),
        Some(second.join("project.zipzi.json").as_path())
    );
    assert!(!sdk.modified());

    sdk.undo().unwrap();
    let restored = sdk.asset(ASSET).unwrap();
    assert_eq!(
        restored.content.as_ref().unwrap().sha256,
        old.content.as_ref().unwrap().sha256
    );
    assert_eq!(
        restored.content.as_ref().unwrap().source,
        temp.path("first")
            .join(&old.content.as_ref().unwrap().source)
            .to_str()
            .unwrap()
    );
    assert!(sdk.diagnose_resources().is_empty());
    assert!(sdk.modified());
    sdk.redo().unwrap();
    assert_eq!(sdk.asset(ASSET).unwrap(), saved);
    assert!(sdk.diagnose_resources().is_empty());
    assert!(!sdk.modified());
}

#[test]
fn deleted_historical_asset_is_rebased_to_its_original_project() {
    let temp = TempDir::new();
    let texture = temp.png("a.png", 10);
    let mut sdk = session(&texture, false);
    sdk.save_project(&temp.path("first"), None).unwrap();
    let old = sdk.asset(ASSET).unwrap();
    sdk.edit("erase asset", None, |edit| edit.erase_object(ASSET))
        .unwrap();
    sdk.save_project(&temp.path("second"), None).unwrap();
    assert!(sdk.asset_ids().is_empty());
    sdk.undo().unwrap();
    assert_eq!(
        sdk.asset(ASSET).unwrap().content.as_ref().unwrap().sha256,
        old.content.as_ref().unwrap().sha256
    );
    assert_eq!(
        sdk.asset(ASSET).unwrap().content.as_ref().unwrap().source,
        temp.path("first")
            .join(&old.content.as_ref().unwrap().source)
            .to_str()
            .unwrap()
    );
    assert!(sdk.diagnose_resources().is_empty());
    sdk.redo().unwrap();
    assert!(sdk.asset_ids().is_empty());
    assert!(!sdk.modified());
}

#[test]
fn failed_save_and_open_preserve_state_successful_open_resets_generation() {
    let temp = TempDir::new();
    let texture = temp.png("a.png", 10);
    let mut sdk = session(&texture, true);
    let path = temp.path("first");
    sdk.save_project(&path, None).unwrap();
    let manifest = sdk.project_path().unwrap().to_path_buf();
    let original_manifest = fs::read(&manifest).unwrap();
    let old_handle = sdk.handle(ObjectKind::Mesh, MESH).unwrap();
    sdk.edit("rename", None, |edit| edit.rename_mesh(MESH, "edited"))
        .unwrap();
    let before = sdk.version();
    let history = sdk.history_lengths();
    let frame = sdk.preview_frame().unwrap();

    let existing = temp.path("existing");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("project.zipzi.json"), &original_manifest).unwrap();
    assert_eq!(
        sdk.save_project(&existing, None).unwrap_err().code.as_ref(),
        "DESTINATION_EXISTS"
    );
    fs::write(&manifest, [original_manifest.as_slice(), b" "].concat()).unwrap();
    assert_eq!(
        sdk.save_project(&path, None).unwrap_err().code.as_ref(),
        "PROJECT_CONFLICT"
    );
    assert_eq!(
        sdk.open_project(&temp.path("missing"), None)
            .unwrap_err()
            .code
            .as_ref(),
        "PROJECT_IO"
    );
    assert_eq!(sdk.version(), before);
    assert_eq!(sdk.history_lengths(), history);
    assert!(Arc::ptr_eq(&frame, &sdk.preview_frame().unwrap()));
    sdk.resolve_handle(&old_handle).unwrap();

    fs::write(&manifest, original_manifest).unwrap();
    let result = sdk.open_project(&path, Some(before)).unwrap();
    assert!(result.status.is_ok());
    assert!(result.diagnostics.is_empty());
    assert_eq!(sdk.version().generation, before.generation + 1);
    assert_eq!(sdk.history_lengths(), (0, 0));
    assert_eq!(sdk.mesh(MESH).unwrap().name, "mesh");
    assert_eq!(
        sdk.resolve_handle(&old_handle).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );

    let asset = sdk.asset(ASSET).unwrap();
    fs::remove_file(path.join(&asset.content.as_ref().unwrap().source)).unwrap();
    let opened = sdk.open_project(&path, None).unwrap();
    assert_eq!(opened.diagnostics.len(), 1);
    assert_eq!(opened.diagnostics[0].asset_id, ASSET);
    assert_eq!(sdk.diagnose_resources().len(), 1);
}

fn external_model() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/external_v50")
}

#[test]
fn model3_import_edits_saves_and_exports_without_changing_session_on_export() {
    let temp = TempDir::new();
    let mut sdk = session(&temp.png("initial.png", 10), true);
    let old_handle = sdk.handle(ObjectKind::Mesh, MESH).unwrap();
    let before = sdk.version();
    let imported = sdk
        .import_model3(&external_model().join("model.model3.json"), Some(before))
        .unwrap();
    assert_eq!(imported.before, before);
    assert_eq!(imported.after, sdk.version());
    assert_eq!(sdk.version().generation, before.generation + 1);
    assert_eq!(imported.report.moc_version, 5);
    assert_eq!(imported.project.diagnostics.len(), 2);
    assert_eq!(sdk.missing_attachments().len(), 2);
    assert!(sdk.project_path().is_none());
    assert_eq!(sdk.history_lengths(), (0, 0));
    assert_eq!(
        sdk.resolve_handle(&old_handle).unwrap_err().code.as_ref(),
        "STALE_HANDLE"
    );
    let mesh_id = sdk.mesh_ids()[0].clone();
    let old_name = sdk.mesh(&mesh_id).unwrap().name;
    sdk.edit("rename imported", None, |edit| {
        edit.rename_mesh(&mesh_id, "edited imported")
    })
    .unwrap();
    sdk.save_project(&temp.path("saved"), None).unwrap();
    let saved_version = sdk.version();
    let package = temp.path("exported");
    assert_eq!(
        sdk.export_package(&package, Some(saved_version))
            .unwrap_err()
            .code
            .as_ref(),
        "MISSING_PACKAGE_ATTACHMENT"
    );
    let missing = sdk.missing_attachments();
    sdk.edit("discard absent source attachments", None, |edit| {
        for path in &missing {
            edit.discard_missing_attachment(path)?;
        }
        Ok(())
    })
    .unwrap();
    let exported_version = sdk.version();
    let exported_history = sdk.history_lengths();
    let publication = sdk
        .export_package(&package, Some(exported_version))
        .unwrap();
    assert!(publication.published);
    assert!(package.join("model.moc3").exists());
    assert_eq!(sdk.version(), exported_version);
    assert_eq!(sdk.history_lengths(), exported_history);
    sdk.undo().unwrap();
    assert_eq!(sdk.missing_attachments().len(), 2);
    assert_eq!(sdk.mesh(&mesh_id).unwrap().name, "edited imported");
    sdk.undo().unwrap();
    assert_eq!(sdk.mesh(&mesh_id).unwrap().name, old_name);
    assert!(sdk.diagnose_resources().is_empty());
}

#[test]
fn bare_moc3_import_rejects_relative_texture_paths_without_losing_state() {
    let temp = TempDir::new();
    let mut sdk = session(&temp.png("initial.png", 10), true);
    let before = sdk.version();
    let moc3 = external_model().join("model.moc3");
    let relative_map = HashMap::from([(0, PathBuf::from("texture_00.png"))]);
    assert_eq!(
        sdk.import_bare_moc3(&moc3, &relative_map, None)
            .unwrap_err()
            .code
            .as_ref(),
        "INVALID_PATH"
    );
    assert_eq!(sdk.version(), before);
    assert!(sdk.mesh(MESH).is_some());
    let texture_map = HashMap::from([(0, external_model().join("texture_00.png"))]);
    let imported = sdk
        .import_bare_moc3(&moc3, &texture_map, Some(before))
        .unwrap();
    assert_eq!(imported.report.moc_version, 5);
    assert!(imported.project.diagnostics.is_empty());
    assert_eq!(sdk.version().generation, before.generation + 1);
    assert!(!sdk.mesh_ids().is_empty());
    assert!(sdk.diagnose_resources().is_empty());
}

#[test]
fn relocation_requires_same_bytes_and_replacement_accepts_new_content() {
    let temp = TempDir::new();
    let original = temp.png("a.png", 10);
    let changed = temp.png("b.png", 200);
    let relocated_path = temp.path("same.png");
    fs::copy(&original, &relocated_path).unwrap();
    let mut sdk = session(&original, true);
    let asset = sdk.asset(ASSET).unwrap();
    let before = sdk.version();
    assert_eq!(
        prepare_relocated_asset(&asset, &changed)
            .unwrap_err()
            .code
            .as_ref(),
        "RESOURCE_HASH"
    );
    assert_eq!(sdk.version(), before);
    let relocated = prepare_relocated_asset(&asset, &relocated_path).unwrap();
    assert_eq!(
        relocated.content.as_ref().unwrap().sha256,
        asset.content.as_ref().unwrap().sha256
    );
    assert_ne!(
        relocated.content.as_ref().unwrap().source,
        asset.content.as_ref().unwrap().source
    );
    sdk.edit("relocate", None, |edit| edit.replace_asset(relocated))
        .unwrap();
    assert!(sdk.diagnose_resources().is_empty());
    let replacement = prepare_png_asset(ASSET, "texture", &changed).unwrap();
    sdk.edit("replace", None, |edit| edit.replace_asset(replacement))
        .unwrap();
    assert_ne!(
        sdk.asset(ASSET).unwrap().content.as_ref().unwrap().sha256,
        asset.content.as_ref().unwrap().sha256
    );
    sdk.undo().unwrap();
    assert_eq!(
        sdk.asset(ASSET).unwrap().content.as_ref().unwrap().sha256,
        asset.content.as_ref().unwrap().sha256
    );
}

#[test]
fn relative_png_paths_require_an_explicit_absolute_base() {
    let temp = TempDir::new();
    let absolute = temp.png("texture.png", 10);
    assert_eq!(
        prepare_png_asset(ASSET, "texture", Path::new("texture.png"))
            .unwrap_err()
            .code
            .as_ref(),
        "INVALID_PATH"
    );
    let from_base =
        prepare_png_asset_from_base(ASSET, "texture", &temp.0, Path::new("texture.png")).unwrap();
    assert_eq!(
        from_base,
        prepare_png_asset(ASSET, "texture", &absolute).unwrap()
    );
    assert_eq!(
        prepare_png_asset_from_base(
            ASSET,
            "texture",
            Path::new("relative-base"),
            Path::new("texture.png"),
        )
        .unwrap_err()
        .code
        .as_ref(),
        "INVALID_PATH"
    );
}

#[derive(Default)]
struct PostCommitWarning {
    syncs: AtomicUsize,
}

impl FileSystem for PostCommitWarning {
    fn write_new(&self, path: &Path, bytes: &[u8]) -> io::Result<()> {
        NativeFileSystem.write_new(path, bytes)
    }

    fn sync_directory(&self, path: &Path) -> io::Result<()> {
        let count = self.syncs.fetch_add(1, Ordering::SeqCst) + 1;
        if count == 3 {
            return Err(io::Error::other("injected post-publication warning"));
        }
        NativeFileSystem.sync_directory(path)
    }

    fn rename(&self, from: &Path, to: &Path) -> io::Result<()> {
        NativeFileSystem.rename(from, to)
    }
}

#[test]
fn publication_warning_keeps_saved_baseline_and_undo_history() {
    let temp = TempDir::new();
    let texture = temp.png("a.png", 10);
    let filesystem = Arc::new(PostCommitWarning::default());
    let mut sdk = AuthoringSession::with_filesystem(
        DOCUMENT,
        Canvas::new(100.0, 100.0, Vec2::new(50.0, 50.0), 10.0),
        filesystem,
    )
    .unwrap();
    let asset = prepare_png_asset(ASSET, "texture", &texture).unwrap();
    sdk.edit("create", None, |edit| edit.create_asset(asset))
        .unwrap();
    let project = temp.path("warning-project");
    let warning = sdk.save_project(&project, None).unwrap();
    assert!(!warning.durable);
    assert!(!warning.warnings.is_empty());
    assert!(warning.manifest.exists());
    assert!(!sdk.modified());
    sdk.undo().unwrap();
    assert!(sdk.asset_ids().is_empty());
    sdk.redo().unwrap();
    assert!(sdk.diagnose_resources().is_empty());
    assert!(!sdk.modified());
    let retried = sdk.save_project(&project, None).unwrap();
    #[cfg(unix)]
    assert!(retried.durable);
    #[cfg(unix)]
    assert!(retried.warnings.is_empty());
    #[cfg(not(unix))]
    assert!(!retried.durable);
}

#[test]
fn unsaved_psd_snapshot_exports_both_formats_after_source_session_is_dropped() {
    let temp = TempDir::new();
    let source = temp.path("art.psd");
    fs::write(&source, LAYERED_PSD).unwrap();
    let mut session = AuthoringSession::new(DOCUMENT, zipzi_core::ModelSetup::default()).unwrap();
    session.import_psd_in_memory(&source, None).unwrap();
    let original = session.read_texture(&session.asset_ids()[0]).unwrap();
    let runtime = session.transfer_snapshot().unwrap();
    let cubism = session.transfer_snapshot().unwrap();
    drop(session);
    fs::remove_file(source).unwrap();
    let output = temp.path("runtime");
    runtime
        .publish(zipzi_project::TransferFormat::Runtime, &output)
        .unwrap();
    cubism
        .publish(zipzi_project::TransferFormat::Cubism, &temp.path("artwork"))
        .unwrap();
    assert!(temp.path("artwork/artwork.cmo3").is_file());
    assert!(!temp.path("artwork/artwork.can3").exists());
    let model: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("model.model3.json")).unwrap()).unwrap();
    let texture = model["FileReferences"]["Textures"][0].as_str().unwrap();
    let decoded = zipzi_project::decode_png(&fs::read(output.join(texture)).unwrap()).unwrap();
    assert_eq!(decoded.rgba, original.rgba.as_ref());
}
