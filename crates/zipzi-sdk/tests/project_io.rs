//! Resource ownership across native persistence. Checkpoint relocation is retired:
//! captured versions, history and independent processes must retain exact bytes.
use crate::support::{archive, prepared};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Arc,
};
use zipzi_core::{state::DocumentState, Vec2};
use zipzi_project::{ProjectImporter, TransferFormat, TransferSnapshot};
use zipzi_sdk::{AuthoringSession, NameTarget, ProjectIo, ResourceStore, Snapshot};
const DOCUMENT: &str = "00000000-0000-4000-8000-000000000001";
const ASSET: &str = "00000000-0000-4000-8000-000000000002";
const MESH: &str = "00000000-0000-4000-8000-000000000003";
const LAYERED_PSD: &[u8] = include_bytes!("../../zipzi-psd/tests/fixtures/layered.psd");
fn io(root: &Path) -> ProjectIo {
    ProjectIo::new(root.join("cache"), ResourceStore::default())
}
fn png(path: &Path, color: u8) -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut bytes, 2, 2);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        encoder
            .write_header()
            .unwrap()
            .write_image_data(&[color; 16])
            .unwrap();
    }
    fs::write(path, &bytes).unwrap();
    bytes
}
fn session(texture: &Path) -> AuthoringSession {
    let mut s = AuthoringSession::new(
        Snapshot::new(
            DocumentState::new(DOCUMENT, Default::default()).unwrap(),
            Default::default(),
        )
        .unwrap(),
    );
    let image = ResourceStore::default().import_image(texture).unwrap();
    let (_, change) = prepared(&s, |c| {
        c.create_empty_asset(ASSET.into(), "texture".into())?;
        c.replace_image(ASSET, image)?;
        c.create_mesh(zipzi_sdk::rectangle_mesh(
            MESH,
            "mesh",
            ASSET,
            Vec2::default(),
            Vec2::new(2., 2.),
        )?)
    })
    .unwrap();
    s.publish(change).unwrap();
    s
}
fn bytes(snapshot: &Snapshot, id: &str) -> Vec<u8> {
    snapshot
        .resources()
        .read_asset(snapshot.document().asset(id).unwrap())
        .unwrap()
        .bytes
        .to_vec()
}
fn replace(s: &mut AuthoringSession, color: u8) {
    let image = ResourceStore::default()
        .prepare_rgba(2, 2, Arc::from([color; 16]))
        .unwrap();
    let (_, change) = prepared(s, |c| c.replace_image(ASSET, image)).unwrap();
    s.publish(change).unwrap();
}
#[test]
fn save_as_preserves_resource_versions_on_both_history_branches_without_sources() {
    for save_from_undo in [false, true] {
        let root = tempfile::tempdir().unwrap();
        let io = io(root.path());
        let source = root.path().join("source.png");
        png(&source, 31);
        let mut s = session(&source);
        let original = s.snapshot().clone();
        let first = root.path().join("first.zipzi");
        s.save_as(&io, &first, None, || true).unwrap();
        replace(&mut s, 72);
        let changed = s.snapshot().clone();
        fs::remove_file(source).unwrap();
        if save_from_undo {
            s.undo(s.version()).unwrap();
        }
        let saved = s.snapshot().content_id();
        let history = s.history_state();
        let second = root.path().join("second.zipzi");
        s.save_as(&io, &second, None, || true).unwrap();
        assert_eq!(s.history_state(), history);
        assert_eq!(s.snapshot().content_id(), saved);
        assert!(!s.is_dirty());
        fs::remove_file(first).unwrap();
        if save_from_undo {
            s.redo(s.version()).unwrap();
        }
        assert_eq!(bytes(s.snapshot(), ASSET), bytes(&changed, ASSET));
        s.undo(s.version()).unwrap();
        assert_eq!(bytes(s.snapshot(), ASSET), bytes(&original, ASSET));
        assert_eq!(s.is_dirty(), !save_from_undo);
        s.redo(s.version()).unwrap();
        assert_eq!(bytes(s.snapshot(), ASSET), bytes(&changed, ASSET));
        assert_eq!(s.is_dirty(), save_from_undo);
        let reopened = io.load(&second, || true).unwrap();
        assert_eq!(
            reopened
                .resources
                .read_asset(reopened.document.asset(ASSET).unwrap())
                .unwrap()
                .bytes,
            bytes(if save_from_undo { &original } else { &changed }, ASSET)
        );
    }
}
#[test]
fn deletion_history_and_detached_candidates_keep_images_after_save_and_replacement() {
    let root = tempfile::tempdir().unwrap();
    let io = io(root.path());
    let source = root.path().join("source.png");
    png(&source, 41);
    let mut s = session(&source);
    let captured = s.read_snapshot();
    let mut candidate = s.begin_draft();
    candidate
        .batch(candidate.source(), |c| {
            c.rename(NameTarget::Mesh(MESH), "candidate".into())
        })
        .unwrap();
    let (_, remove) = prepared(&s, |c| {
        let mut mesh = c.read_document()?.mesh(MESH).unwrap().clone();
        mesh.texture_asset_id = Default::default();
        c.replace_mesh(mesh)?;
        c.delete_asset(ASSET, None)
    })
    .unwrap();
    s.publish(remove).unwrap();
    let path = root.path().join("deleted.zipzi");
    s.save_as(&io, &path, None, || true).unwrap();
    fs::remove_file(source).unwrap();
    s.undo(s.version()).unwrap();
    assert_eq!(bytes(s.snapshot(), ASSET), bytes(&captured.snapshot, ASSET));
    let empty = Snapshot::new(
        DocumentState::new(uuid::Uuid::new_v4().to_string(), Default::default()).unwrap(),
        Default::default(),
    )
    .unwrap();
    s.replace(s.version(), empty).unwrap();
    assert_eq!(
        bytes(&candidate.read_snapshot().unwrap().snapshot, ASSET),
        bytes(&captured.snapshot, ASSET)
    );
    assert_eq!(
        s.publish(candidate.prepare("old document").unwrap())
            .unwrap_err()
            .code,
        "STALE_SOURCE"
    );
    drop(s);
    fs::remove_file(path).unwrap();
    assert_eq!(
        crate::support::frame(&captured.snapshot, &Default::default())
            .unwrap()
            .drawables
            .len(),
        1
    );
    assert!(!bytes(&captured.snapshot, ASSET).is_empty());
}
#[test]
fn psd_capture_can_edit_save_and_export_after_source_and_session_are_gone() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("art.psd");
    fs::write(&source, LAYERED_PSD).unwrap();
    let (imported, _) = ProjectImporter::default().import_psd(&source).unwrap();
    let mut s =
        AuthoringSession::new(Snapshot::new(imported.document, imported.resources).unwrap());
    assert!(s.project().is_none());
    assert!(s.is_dirty());
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);
    let asset = s.snapshot().document().asset_order()[0].clone();
    let mesh = s.snapshot().document().mesh_order()[0].clone();
    let original = s.snapshot().clone();
    let texture = s
        .snapshot()
        .resources()
        .read_texture(s.snapshot().document().asset(&asset).unwrap())
        .unwrap();
    let again = s
        .snapshot()
        .resources()
        .read_texture(s.snapshot().document().asset(&asset).unwrap())
        .unwrap();
    assert!(Arc::ptr_eq(&texture.rgba, &again.rgba));
    fs::remove_file(source).unwrap();
    let (_, change) = prepared(&s, |c| c.rename(NameTarget::Mesh(&mesh), "edited".into())).unwrap();
    s.publish(change).unwrap();
    let io = io(root.path());
    let path = root.path().join("psd.zipzi");
    s.save_as(&io, &path, None, || true).unwrap();
    s.undo(s.version()).unwrap();
    assert_eq!(bytes(s.snapshot(), &asset), bytes(&original, &asset));
    assert!(s.is_dirty());
    s.redo(s.version()).unwrap();
    assert!(!s.is_dirty());
    let captured = s.snapshot().clone();
    drop(s);
    fs::remove_file(path).unwrap();
    for format in [TransferFormat::Runtime, TransferFormat::Cubism] {
        let transfer = TransferSnapshot::new(
            captured.document().clone(),
            captured.resources().clone(),
            vec![],
        )
        .unwrap();
        let destination = root.path().join(format!("{format:?}"));
        transfer.publish(format, &destination).unwrap();
        assert!(fs::read_dir(destination).unwrap().next().is_some());
    }
}
#[test]
fn failed_import_or_native_load_preserves_an_existing_session_and_its_redo() {
    let root = tempfile::tempdir().unwrap();
    let io = io(root.path());
    let source = root.path().join("source.png");
    png(&source, 30);
    let mut s = session(&source);
    replace(&mut s, 40);
    s.undo(s.version()).unwrap();
    let before = s.read_snapshot();
    let history = s.history_state();
    let malformed = root.path().join("bad.psd");
    fs::write(&malformed, b"bad PSD").unwrap();
    assert!(ProjectImporter::default().import_psd(&malformed).is_err());
    assert!(io.load(&malformed, || true).is_err());
    assert_eq!(s.read_snapshot().source, before.source);
    assert_eq!(s.history_state(), history);
    assert_eq!(bytes(s.snapshot(), ASSET), bytes(&before.snapshot, ASSET));
    let occupied = root.path().join("occupied.zipzi");
    fs::write(&occupied, b"keep me").unwrap();
    assert!(s.save_as(&io, &occupied, None, || true).is_err());
    assert_eq!(fs::read(occupied).unwrap(), b"keep me");
    assert!(s.project().is_none());
    assert_eq!(s.read_snapshot().source, before.source);
    assert_eq!(s.history_state(), history);
    s.redo(s.version()).unwrap();
    assert_ne!(bytes(s.snapshot(), ASSET), bytes(&before.snapshot, ASSET));
}
#[test]
fn captured_file_versions_do_not_follow_external_replacements() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.png");
    png(&source, 52);
    let mut s = session(&source);
    let captured = s.snapshot().clone();
    let original = bytes(&captured, ASSET);
    png(&source, 78);
    assert_eq!(bytes(s.snapshot(), ASSET), original);
    let replacement = ResourceStore::default().import_image(&source).unwrap();
    let (_, change) = prepared(&s, |c| c.replace_image(ASSET, replacement)).unwrap();
    s.publish(change).unwrap();
    fs::remove_file(source).unwrap();
    assert_ne!(bytes(s.snapshot(), ASSET), original);
    s.undo(s.version()).unwrap();
    assert_eq!(bytes(s.snapshot(), ASSET), original);
    assert_eq!(bytes(&captured, ASSET), original);
}
#[test]
fn native_archive_child_process() {
    let Some(mode) = std::env::var_os("ZIPZI_ARCHIVE_TEST_CHILD") else {
        return;
    };
    let root = PathBuf::from(std::env::var_os("ZIPZI_ARCHIVE_TEST_ROOT").unwrap());
    let io = io(&root);
    let project = root.join("角色 moved.zipzi");
    let (mut s, _) = AuthoringSession::open(&io, &project, || true).unwrap();
    if mode == "conflict" {
        fs::write(root.join("child-ready"), b"ready").unwrap();
        wait_for(&root.join("parent-saved"));
        assert_eq!(s.save(&io, || true).unwrap_err().code, "PROJECT_CONFLICT");
    } else {
        let canonical = archive::project_json(&archive::native_bytes_with_resources(
            s.snapshot().document(),
            s.snapshot().resources(),
        ));
        let expected: serde_json::Value =
            serde_json::from_slice(&fs::read(root.join("expected.json")).unwrap()).unwrap();
        assert_eq!(canonical, expected);
        assert!(s.snapshot().document().editor_state().objects[ASSET].locked);
        assert_eq!(
            bytes(s.snapshot(), ASSET),
            fs::read(root.join("expected.png")).unwrap()
        );
        assert_eq!(
            crate::support::frame(s.snapshot(), &Default::default())
                .unwrap()
                .drawables
                .len(),
            1
        );
    }
}
fn wait_for(path: &Path) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while !path.exists() {
        assert!(
            std::time::Instant::now() < deadline,
            "peer did not reach {}",
            path.display()
        );
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}
#[test]
fn moved_archive_opens_in_new_process_without_sources_and_stale_process_conflicts() {
    use std::process::{Command, Stdio};
    let root = tempfile::tempdir().unwrap();
    let io = io(root.path());
    let source = root.path().join("source.png");
    png(&source, 79);
    let mut s = session(&source);
    let (_, change) = prepared(&s, |c| {
        let mut editor = c.read_document()?.editor_state().clone();
        editor.objects.entry(ASSET.into()).or_default().locked = true;
        c.replace_editor_state(editor)
    })
    .unwrap();
    s.publish(change).unwrap();
    let original = root.path().join("original.zipzi");
    s.save_as(&io, &original, None, || true).unwrap();
    fs::write(
        root.path().join("expected.json"),
        serde_json::to_vec(&archive::project_json(&fs::read(&original).unwrap())).unwrap(),
    )
    .unwrap();
    fs::write(root.path().join("expected.png"), bytes(s.snapshot(), ASSET)).unwrap();
    fs::remove_file(source).unwrap();
    drop(s);
    let moved = root.path().join("角色 moved.zipzi");
    fs::rename(original, &moved).unwrap();
    let child = |mode: &str| {
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "project_io::native_archive_child_process",
                "--nocapture",
            ])
            .env("ZIPZI_ARCHIVE_TEST_CHILD", mode)
            .env("ZIPZI_ARCHIVE_TEST_ROOT", root.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap()
    };
    let opened = child("verify").wait_with_output().unwrap();
    assert!(
        opened.status.success(),
        "{} {}",
        String::from_utf8_lossy(&opened.stdout),
        String::from_utf8_lossy(&opened.stderr)
    );
    let stale = child("conflict");
    wait_for(&root.path().join("child-ready"));
    let (mut current, _) = AuthoringSession::open(&io, &moved, || true).unwrap();
    let (_, change) = prepared(&current, |c| {
        c.rename(NameTarget::Mesh(MESH), "saved by parent".into())
    })
    .unwrap();
    current.publish(change).unwrap();
    current.save(&io, || true).unwrap();
    let saved = fs::read(&moved).unwrap();
    fs::write(root.path().join("parent-saved"), b"saved").unwrap();
    let stale = stale.wait_with_output().unwrap();
    assert!(
        stale.status.success(),
        "{} {}",
        String::from_utf8_lossy(&stale.stdout),
        String::from_utf8_lossy(&stale.stderr)
    );
    assert_eq!(fs::read(moved).unwrap(), saved);
}
