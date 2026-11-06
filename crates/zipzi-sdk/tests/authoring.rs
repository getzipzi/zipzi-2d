use std::{
    fs,
    path::Path,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use zipzi_core::{
    state::{AssetContent, AssetRecord, DocumentState, OwnedDocument},
    Mesh, Part, Vec2,
};
use zipzi_project::archive::{
    ArchiveFileSystem, ArchiveLock, ArchiveStaging, NativeArchiveFileSystem,
};
use zipzi_sdk::{
    AuthoringSession, NameTarget, PreparedProject, ProjectIo, ResourceStore, ResourceView, Snapshot,
};

fn id(n: u32) -> String {
    format!("00000000-0000-4000-8000-{n:012x}")
}
fn session() -> AuthoringSession {
    let state = DocumentState::from_owned(OwnedDocument {
        id: id(1),
        parts: vec![Part {
            id: id(2),
            runtime_id: "part".into(),
            ..Default::default()
        }],
        meshes: vec![Mesh {
            id: id(3),
            runtime_id: "mesh".into(),
            name: "mesh".into(),
            part_id: id(2),
            texture_asset_id: zipzi_core::SceneReference::Resolved { object_id: id(4) },
            vertex_ids: vec![1, 2, 3],
            base_positions: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
            uvs: vec![Vec2::new(0., 0.), Vec2::new(1., 0.), Vec2::new(0., 1.)],
            triangles: vec![[1, 2, 3]],
            ..Default::default()
        }],
        assets: vec![AssetRecord {
            id: id(4),
            name: "image".into(),
            content: AssetContent::Empty,
        }],
        ..Default::default()
    })
    .unwrap();
    AuthoringSession::new(Snapshot::new(state, ResourceView::default()).unwrap())
}
fn rename(s: &mut AuthoringSession, name: &str) {
    let mut draft = s.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.rename(NameTarget::Mesh(&id(3)), name.into())
        })
        .unwrap();
    s.publish(draft.prepare("rename").unwrap()).unwrap();
}
fn image(s: &mut AuthoringSession, store: &ResourceStore, bytes: [u8; 4]) {
    let resource = store.prepare_rgba(1, 1, Arc::from(bytes)).unwrap();
    let mut draft = s.begin_draft();
    draft
        .batch(draft.source(), |c| c.replace_image(&id(4), resource))
        .unwrap();
    s.publish(draft.prepare("image").unwrap()).unwrap();
}

#[test]
fn native_image_load_edit_batch_history_save_and_save_as() {
    let root = tempfile::tempdir().unwrap();
    let store = ResourceStore::default();
    let io = ProjectIo::new(root.path().join("cache"), store.clone());
    let path = root.path().join("first.zipzi");
    let second = root.path().join("second.zipzi");
    let mut s = session();
    assert!(s.is_dirty());
    image(&mut s, &store, [255, 0, 0, 255]);
    let initial = s.read_snapshot();
    let before_history = s.history_state();
    let before_version = s.version();
    s.save_as(&io, &path, None, || true).unwrap();
    assert!(!s.is_dirty());
    assert_eq!(s.version(), before_version);
    assert_eq!(s.history_state(), before_history);
    let (mut loaded, diagnostics) = AuthoringSession::open(&io, &path, || true).unwrap();
    assert!(diagnostics.is_empty());
    assert!(!loaded.is_dirty());
    assert_eq!(
        loaded.snapshot().document().mesh(&id(3)),
        s.snapshot().document().mesh(&id(3))
    );
    let mut draft = loaded.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.rename(NameTarget::Mesh(&id(3)), "edited".into())?;
            c.set_vertex_positions(&id(3), &[(2, Vec2::new(2., 0.))])?;
            c.create_part(Part {
                id: id(6),
                runtime_id: "created".into(),
                ..Default::default()
            })
        })
        .unwrap();
    loaded.publish(draft.prepare("batch").unwrap()).unwrap();
    assert_eq!(loaded.history_state().undo_steps, 1);
    assert!(loaded.is_dirty());
    loaded.undo(loaded.version()).unwrap();
    assert!(!loaded.is_dirty());
    loaded.redo(loaded.version()).unwrap();
    assert!(loaded.is_dirty());
    image(&mut loaded, &store, [0, 0, 255, 255]);
    let frozen = loaded.read_snapshot();
    let history = loaded.history_state();
    loaded.save_as(&io, &second, None, || true).unwrap();
    assert_eq!(loaded.history_state(), history);
    assert_eq!(loaded.snapshot().content_id(), frozen.snapshot.content_id());
    assert_eq!(
        loaded.project().unwrap().path(),
        fs::canonicalize(&second).unwrap()
    );
    loaded.undo(loaded.version()).unwrap();
    assert!(loaded.is_dirty());
    loaded.redo(loaded.version()).unwrap();
    assert!(!loaded.is_dirty());
    loaded.save(&io, || true).unwrap();
    let reopened = io.load(&second, || true).unwrap();
    let key = reopened.document.image_resources().next().unwrap().0;
    assert_eq!(
        &*reopened.resources.image(key).unwrap().texture().rgba,
        &[0, 0, 255, 255]
    );
    drop(s);
    drop(loaded);
    drop(io);
    drop(store);
    fs::remove_file(path).unwrap();
    fs::remove_file(second).unwrap();
    let old_key = initial
        .snapshot
        .document()
        .image_resources()
        .next()
        .unwrap()
        .0;
    assert_eq!(
        &*initial
            .snapshot
            .resources()
            .image(old_key)
            .unwrap()
            .texture()
            .rgba,
        &[255, 0, 0, 255]
    );
}

#[test]
fn failed_cancelled_and_conflicting_saves_do_not_change_markers_or_history() {
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let path = root.path().join("project.zipzi");
    let mut s = session();
    s.save_as(&io, &path, None, || true).unwrap();
    let bytes = fs::read(&path).unwrap();
    let marker = s.saved_marker();
    let context = s.project().unwrap().clone();
    rename(&mut s, "dirty");
    let content = s.snapshot().content_id();
    let history = s.history_state();
    assert_eq!(s.save(&io, || false).unwrap_err().code, "CANCELLED");
    assert_eq!(fs::read(&path).unwrap(), bytes);
    let occupied = root.path().join("occupied.zipzi");
    fs::write(&occupied, b"external").unwrap();
    assert_eq!(
        s.save_as(&io, &occupied, None, || true).unwrap_err().code,
        "PROJECT_EXISTS"
    );
    fs::write(&path, b"externally changed").unwrap();
    assert_eq!(s.save(&io, || true).unwrap_err().code, "PROJECT_CONFLICT");
    assert_eq!(s.saved_marker(), marker);
    assert_eq!(s.project(), Some(&context));
    assert_eq!(s.snapshot().content_id(), content);
    assert_eq!(s.history_state(), history);
    assert!(s.is_dirty());
    let fp = io.fingerprint(&occupied, || true).unwrap();
    s.save_as(&io, &occupied, Some(&fp), || true).unwrap();
    assert!(!s.is_dirty());
}

#[test]
fn late_prepared_load_cannot_replace_a_newer_document_and_failure_is_atomic() {
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let path = root.path().join("project.zipzi");
    let mut s = session();
    s.save_as(&io, &path, None, || true).unwrap();
    let prepared = PreparedProject::load(s.version(), &io, &path, || true).unwrap();
    rename(&mut s, "newer");
    let before = s.version();
    assert_eq!(s.publish_loaded(prepared).unwrap_err().code, "STALE_SOURCE");
    assert_eq!(s.version(), before);
    assert!(s
        .load(&io, &root.path().join("missing.zipzi"), || true)
        .is_err());
    assert_eq!(s.version(), before);
    s.load(&io, &path, || true).unwrap();
    assert_eq!(s.version().generation, before.generation + 1);
    assert_eq!(s.history_state().undo_steps, 0);
    assert!(!s.is_dirty());
}

#[test]
fn saved_identity_is_conservative_and_noop_does_not_dirty() {
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let mut s = session();
    s.save_as(&io, &root.path().join("project.zipzi"), None, || true)
        .unwrap();
    rename(&mut s, "mesh");
    assert!(!s.is_dirty());
    rename(&mut s, "other");
    rename(&mut s, "mesh");
    assert!(s.is_dirty());
    s.undo(s.version()).unwrap();
    s.undo(s.version()).unwrap();
    assert!(!s.is_dirty());
    let replacement = session().snapshot().clone();
    s.replace(s.version(), replacement).unwrap();
    assert!(s.project().is_none());
    assert!(s.saved_marker().is_none());
    assert!(s.is_dirty());
}

#[test]
fn checked_in_native_scene_fixtures_open_edit_and_save_through_public_sdk() {
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/observe_inspection/projects");
    for name in [
        "rotated-warp.zipzi",
        "pixel-binding.zipzi",
        "masked-offscreen.zipzi",
    ] {
        let (mut s, _) = AuthoringSession::open(&io, &fixtures.join(name), || true).unwrap();
        let mesh = s.snapshot().document().mesh_order()[0].clone();
        let mut draft = s.begin_draft();
        draft
            .batch(draft.source(), |c| {
                c.rename(NameTarget::Mesh(&mesh), "fixture edited".into())
            })
            .unwrap();
        s.publish(draft.prepare("fixture rename").unwrap()).unwrap();
        let path = root.path().join(name);
        s.save_as(&io, &path, None, || true).unwrap();
        let loaded = io.load(&path, || true).unwrap();
        assert_eq!(loaded.document.mesh(&mesh).unwrap().name, "fixture edited");
        assert_eq!(
            s.snapshot().document().mesh_order(),
            loaded.document.mesh_order()
        );
    }
}

struct Faults {
    fail_before: bool,
    cancel: Arc<AtomicBool>,
}
impl ArchiveFileSystem for Faults {
    fn create_staging(&self, path: &Path) -> std::io::Result<ArchiveStaging> {
        NativeArchiveFileSystem.create_staging(path)
    }
    fn acquire_lock(&self, path: &Path) -> std::io::Result<ArchiveLock> {
        NativeArchiveFileSystem.acquire_lock(path)
    }
    fn sync_file(&self, file: &fs::File) -> std::io::Result<()> {
        if self.fail_before {
            Err(std::io::Error::other("injected sync failure"))
        } else {
            NativeArchiveFileSystem.sync_file(file)
        }
    }
    fn publish_no_replace(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        NativeArchiveFileSystem.publish_no_replace(from, to)?;
        self.cancel.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn publish_replace(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        NativeArchiveFileSystem.publish_replace(from, to)?;
        self.cancel.store(true, Ordering::SeqCst);
        Ok(())
    }
    fn sync_directory(&self, _: &Path) -> std::io::Result<()> {
        Err(std::io::Error::other("injected durability warning"))
    }
}
#[test]
fn publication_failure_preserves_file_but_late_cancel_and_warning_install_saved_marker() {
    let root = tempfile::tempdir().unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let base_io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let path = root.path().join("project.zipzi");
    let mut s = session();
    s.save_as(&base_io, &path, None, || true).unwrap();
    let bytes = fs::read(&path).unwrap();
    let marker = s.saved_marker();
    rename(&mut s, "changed");
    let broken = base_io.clone().with_filesystem(Arc::new(Faults {
        fail_before: true,
        cancel: cancel.clone(),
    }));
    assert!(s.save(&broken, || true).is_err());
    assert_eq!(fs::read(&path).unwrap(), bytes);
    assert_eq!(s.saved_marker(), marker);
    assert!(s.is_dirty());
    let late = base_io.with_filesystem(Arc::new(Faults {
        fail_before: false,
        cancel: cancel.clone(),
    }));
    let receipt = s.save(&late, || !cancel.load(Ordering::SeqCst)).unwrap();
    assert!(cancel.load(Ordering::SeqCst));
    assert!(receipt.published);
    assert!(!receipt.durable);
    assert!(!receipt.warnings.is_empty());
    assert!(!s.is_dirty());
    assert_ne!(fs::read(&path).unwrap(), bytes);
}

#[path = "parameter_editing.rs"]
mod parameter_editing;

#[test]
fn authored_parameter_mesh_and_scene_binding_identities_survive_native_save() {
    use zipzi_core::{BindingAxis, Parameter, PartKeyform, SceneBinding, SceneTrack};
    use zipzi_sdk::{MeshBindingAxisSpec, MeshBindingCreateSpec, MeshKeyformSpec};
    let mut s = session();
    let mut draft = s.begin_draft();
    draft
        .batch(draft.source(), |c| {
            c.create_parameter(Parameter {
                id: id(5),
                minimum: 0.,
                maximum: 1.,
                default_value: 0.,
                ..Default::default()
            })?;
            c.create_mesh_binding_spec(MeshBindingCreateSpec {
                id: Some(id(6)),
                mesh_id: id(3),
                axes: vec![MeshBindingAxisSpec {
                    parameter_id: id(5),
                    keys: vec![0., 1.],
                }],
                keyforms: [0., 1.]
                    .into_iter()
                    .map(|v| MeshKeyformSpec {
                        keys: vec![v],
                        positions: vec![Vec2::new(v, 0.), Vec2::new(v + 1., 0.), Vec2::new(v, 1.)],
                        opacity: None,
                        multiply: None,
                        screen: None,
                        draw_order: None,
                    })
                    .collect(),
            })?;
            c.create_scene_binding(SceneBinding {
                id: id(7),
                axes: vec![BindingAxis {
                    parameter_id: id(5),
                    keys: vec![0., 1.],
                    ..Default::default()
                }],
                track: SceneTrack::Part {
                    target_id: id(2).into(),
                    keyforms: [0., 1.]
                        .into_iter()
                        .map(|v| PartKeyform {
                            keys: vec![v],
                            draw_order: v * 100.,
                            ..Default::default()
                        })
                        .collect(),
                },
            })
        })
        .unwrap();
    s.publish(draft.prepare("parameter and ordinary bindings").unwrap())
        .unwrap();
    let authored = s.snapshot().clone();
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path().join("cache"), ResourceStore::default());
    let path = root.path().join("bindings.zipzi");
    s.save_as(&io, &path, None, || true).unwrap();
    let (loaded, _) = AuthoringSession::open(&io, &path, || true).unwrap();
    assert_eq!(
        loaded.snapshot().document().parameter(&id(5)),
        authored.document().parameter(&id(5))
    );
    assert_eq!(
        loaded.snapshot().document().binding(&id(6)),
        authored.document().binding(&id(6))
    );
    assert_eq!(
        loaded.snapshot().document().scene_binding(&id(7)),
        authored.document().scene_binding(&id(7))
    );
    s.undo(s.version()).unwrap();
    assert!(s.snapshot().document().binding(&id(6)).is_none());
    assert!(s.is_dirty());
    s.redo(s.version()).unwrap();
    assert!(!s.is_dirty());
    assert_eq!(
        s.snapshot().document().binding(&id(6)),
        authored.document().binding(&id(6))
    );
}

#[path = "authoring/expressions.rs"]
mod expressions;

#[path = "authoring/animation_domains.rs"]
mod animation_domains;

#[path = "authoring/save_worker.rs"]
mod save_worker;
