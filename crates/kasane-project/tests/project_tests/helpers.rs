use kasane_core::{RotationKeyform, SceneTrack, WarpKeyform};
use kasane_core::{RotationTransform, TransformData};
use std::fs;
use std::path::Path;

use kasane_core::types::{
    Appearance, BindingAxis, Canvas, ImageAsset, Mesh, MeshBinding, MeshKeyform, Parameter, Part,
    RotationPose, Transform, Vec2,
};
use kasane_core::Document;
use kasane_project::{content_sha256, DocumentSession, DocumentStore};

pub(crate) fn id(n: i32) -> String {
    format!("{n:08x}-1111-4111-8111-111111111111")
}

pub(crate) fn sid(n: i32) -> String {
    format!("{n:08x}-2222-4222-8222-222222222222")
}

pub(crate) fn create_test_png(path: &Path) -> (Vec<u8>, String) {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, 8, 8);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        let data = vec![123u8; 8 * 8 * 4];
        writer.write_image_data(&data).unwrap();
    }
    let sha = content_sha256(&buf);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(path, &buf).unwrap();
    (buf, sha)
}

pub(crate) fn fixture_doc(sha1: &str, sha2: &str) -> Document {
    let mut doc = Document::new();
    assert!(doc
        .initialize(
            id(1),
            Canvas {
                width: 640.0,
                height: 480.0,
                origin: Vec2::new(271.0, 193.0),
                pixels_per_unit: 100.0,
                flag: 1,
            }
        )
        .is_ok());

    assert!(doc
        .add_asset(ImageAsset {
            unresolved_source: None,
            id: id(2),
            name: "A".to_string(),
            content: Some(kasane_core::ImageContent {
                source: "assets/0.png".to_string(),
                width: 8,
                height: 8,
                sha256: sha1.to_string(),
            }),
        })
        .status
        .is_ok());

    assert!(doc
        .add_asset(ImageAsset {
            unresolved_source: None,
            id: id(3),
            name: "B".to_string(),
            content: Some(kasane_core::ImageContent {
                source: "assets/1.png".to_string(),
                width: 8,
                height: 8,
                sha256: sha2.to_string(),
            }),
        })
        .status
        .is_ok());

    assert!(doc
        .create_parameter(Parameter {
            id: id(6),
            runtime_id: "ParamAngle".to_string(),
            name: "Angle".to_string(),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 2,
            ..Default::default()
        })
        .status
        .is_ok());

    assert!(doc
        .create_part(Part {
            id: sid(1),
            runtime_id: "PartRoot".to_string(),
            name: "Root Part".to_string(),
            parent_id: String::new(),
            enabled: true,
            draw_order: 10.0,
        })
        .status
        .is_ok());

    let rot = Transform {
        id: sid(2),
        runtime_id: "RootRotation".to_string(),
        name: "Root Rotation".to_string(),
        part_id: kasane_core::PartId::optional(sid(1)),
        data: TransformData::Rotation(RotationTransform {
            base_angle: 0.0,
            pose: RotationPose {
                origin: Vec2::new(320.0, 240.0).into(),
                angle: 0.0,
                scale: 1.0,
                reflect_x: false,
                reflect_y: false,
            },
        }),
        ..Default::default()
    };
    assert!(doc.create_transform(rot).status.is_ok());

    let m1 = Mesh {
        id: id(4),
        name: "Quad".to_string(),
        runtime_id: "ArtMeshA".to_string(),
        texture_asset_id: (id(2)).into(),
        part_id: sid(1),
        deformer_id: sid(2),
        vertex_ids: vec![1, 2, 3, 4],
        base_positions: vec![
            Vec2::new(10.0, 10.0),
            Vec2::new(100.0, 10.0),
            Vec2::new(100.0, 100.0),
            Vec2::new(10.0, 100.0),
        ],
        uvs: vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(1.0, 0.0),
            Vec2::new(1.0, 1.0),
            Vec2::new(0.0, 1.0),
        ],
        triangles: vec![[1, 2, 3], [1, 3, 4]],
        ..Default::default()
    };
    assert!(doc.create_mesh(m1).status.is_ok());

    let mut b = MeshBinding {
        id: id(7),
        mesh_id: id(4),
        axes: vec![BindingAxis {
            id: Default::default(),
            samples: Default::default(),
            parameter_id: id(6),
            keys: vec![-1.0, 0.0, 1.0],
        }],
        keyforms: Vec::new(),
    };
    for &k in &[-1.0f32, 0.0, 1.0] {
        let mut kf = MeshKeyform {
            samples: Default::default(),
            keys: vec![k],
            positions: doc.get_mesh(&id(4)).unwrap().base_positions.clone(),
            appearance: Appearance::default(),
            draw_order: None,
        };
        kf.positions[0].x += k * 5.0;
        b.keyforms.push(kf);
    }
    assert!(doc.create_binding(b).status.is_ok());

    doc
}

pub(crate) fn scene_binding(doc: &mut Document) {
    let transform = doc.get_transform(&sid(2)).unwrap();
    let track = match &transform.data {
        TransformData::Warp(w) => SceneTrack::Warp {
            target_id: sid(2).into(),
            keyforms: [-1., 1.]
                .into_iter()
                .map(|key| WarpKeyform {
                    samples: Default::default(),
                    bezier: None,
                    keys: vec![key],
                    positions: w.points.clone(),
                    appearance: transform.appearance,
                })
                .collect(),
        },
        TransformData::Rotation(r) => SceneTrack::Rotation {
            target_id: sid(2).into(),
            keyforms: [-1., 1.]
                .into_iter()
                .map(|key| RotationKeyform {
                    samples: Default::default(),
                    keys: vec![key],
                    rotation: r.pose,
                    appearance: transform.appearance,
                })
                .collect(),
        },
    };
    let binding = kasane_core::SceneBinding {
        id: sid(99),
        axes: vec![BindingAxis {
            id: Default::default(),
            samples: Default::default(),
            parameter_id: id(6),
            keys: vec![-1., 1.],
        }],
        track,
    };
    assert!(doc.create_scene_binding(binding).status.is_ok());
}

#[derive(Clone)]
pub(crate) enum Failure {
    ShortWrite,
    FileSync,
    DirectorySync(usize),
    Rename(Vec<usize>),
    ExternalChange(std::path::PathBuf),
}
pub(crate) struct FailingFileSystem {
    failure: Failure,
    syncs: std::sync::atomic::AtomicUsize,
    renames: std::sync::atomic::AtomicUsize,
}
impl FailingFileSystem {
    pub(crate) fn new(failure: Failure) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            failure,
            syncs: 0.into(),
            renames: 0.into(),
        })
    }
}
impl kasane_project::FileSystem for FailingFileSystem {
    fn write_new(&self, path: &Path, bytes: &[u8]) -> std::io::Result<()> {
        match &self.failure {
            Failure::ShortWrite => {
                kasane_project::NativeFileSystem.write_new(path, &bytes[..bytes.len() / 2])?;
                Err(std::io::Error::other("injected short write"))
            }
            Failure::FileSync => {
                fs::write(path, bytes)?;
                Err(std::io::Error::other("injected file sync failure"))
            }
            Failure::ExternalChange(manifest)
                if path.file_name().is_some_and(|s| s == "manifest.json") =>
            {
                kasane_project::NativeFileSystem.write_new(path, bytes)?;
                fs::write(manifest, b"external change")
            }
            _ => kasane_project::NativeFileSystem.write_new(path, bytes),
        }
    }
    fn sync_directory(&self, path: &Path) -> std::io::Result<()> {
        let n = self.syncs.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        if matches!(self.failure, Failure::DirectorySync(at) if at == n) {
            return Err(std::io::Error::other("injected directory sync failure"));
        }
        kasane_project::NativeFileSystem.sync_directory(path)
    }
    fn rename(&self, from: &Path, to: &Path) -> std::io::Result<()> {
        let n = self
            .renames
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        if matches!(&self.failure, Failure::Rename(at) if at.contains(&n)) {
            return Err(std::io::Error::other("injected rename failure"));
        }
        kasane_project::NativeFileSystem.rename(from, to)
    }
}

pub(crate) struct TestDirectory(pub(crate) std::path::PathBuf);
impl TestDirectory {
    pub(crate) fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "kasane-regression-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    pub(crate) fn fixture(&self) -> (Document, std::path::PathBuf) {
        let input = self.0.join("input");
        let (_, sha1) = create_test_png(&input.join("assets/0.png"));
        let (_, sha2) = create_test_png(&input.join("assets/1.png"));
        (fixture_doc(&sha1, &sha2), input)
    }
    pub(crate) fn saved_session(&self) -> DocumentSession {
        let (doc, input) = self.fixture();
        let path = self.0.join("project");
        let result = DocumentStore::new().save(&doc, &input, &path, None).0;
        assert!(result.status.is_ok(), "{:?}", result);
        let mut session = DocumentSession::new();
        assert!(session.open(&path).status.is_ok());
        session
    }
}
impl Drop for TestDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
