use std::hint::black_box;
use std::time::Instant;
use zipzi_core::{RotationTransform, TransformData, WarpTransform};

use zipzi_core::state::*;
use zipzi_core::{
    BindingAxis, Canvas, Mesh, MeshBinding, MeshKeyform, Parameter, Part, RotationPose,
    SceneBinding, Transform, Vec2, VertexId,
};
use zipzi_project::{ProjectIo, ResourceStore, ResourceView};

fn make_id(kind: u32, n: u32) -> String {
    format!("{:04x}{:04x}-1111-4111-8111-111111111111", kind, n)
}

fn build_benchmark_document(out_params: &mut Vec<String>) -> (DocumentState, ResourceView) {
    let doc_id = make_id(0x0001, 1);
    let mut doc = DocumentState::new(
        doc_id,
        Canvas::new(1280., 720., Vec2::new(640., 360.), 100.).into(),
    )
    .unwrap()
    .builder();
    let store = ResourceStore::default();
    let images = [
        store
            .prepare_rgba(512, 512, vec![127; 512 * 512 * 4].into())
            .unwrap(),
        store
            .prepare_rgba(512, 512, vec![255; 512 * 512 * 4].into())
            .unwrap(),
    ];
    let asset1 = make_id(0x0002, 1);
    let asset2 = make_id(0x0002, 2);
    doc.create_asset(AssetRecord {
        id: asset1.clone(),
        name: "texture_0".to_string(),
        content: AssetContent::Image {
            resource: images[0].key(),
            width: 512,
            height: 512,
        },
    })
    .unwrap();
    doc.create_asset(AssetRecord {
        id: asset2.clone(),
        name: "texture_1".to_string(),
        content: AssetContent::Image {
            resource: images[1].key(),
            width: 512,
            height: 512,
        },
    })
    .unwrap();

    // 4 Parameters
    out_params.clear();
    for i in 0..4 {
        let pid = make_id(0x0003, i + 1);
        out_params.push(pid.clone());
        doc.create_parameter(Parameter {
            id: pid.clone(),
            runtime_id: format!("Param{}", i),
            name: format!("Parameter {}", i),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            decimal_places: 2,
            ..Default::default()
        })
        .unwrap();
    }

    // 2 Parts
    let part_root = make_id(0x0004, 1);
    let part_child = make_id(0x0004, 2);
    doc.create_part(Part {
        id: part_root.clone(),
        runtime_id: "PartRoot".to_string(),
        name: "Root Part".to_string(),
        parent_id: String::new(),
        enabled: true,
        draw_order: 10.0,
    })
    .unwrap();
    doc.create_part(Part {
        id: part_child.clone(),
        runtime_id: "PartChild".to_string(),
        name: "Child Part".to_string(),
        parent_id: part_root.clone(),
        enabled: true,
        draw_order: 20.0,
    })
    .unwrap();

    // Transform 1: Root Rotation
    let rot_root = make_id(0x0005, 1);
    let tfm_rot = Transform {
        id: rot_root.clone(),
        runtime_id: "RootRotation".to_string(),
        name: "Root Rotation".to_string(),
        part_id: zipzi_core::PartId::optional(part_root.clone()),
        data: TransformData::Rotation(RotationTransform {
            base_angle: 0.0,
            pose: RotationPose {
                origin: Vec2::new(640.0, 360.0).into(),
                angle: 0.0,
                scale: 1.0,
                reflect_x: false,
                reflect_y: false,
            },
        }),
        ..Default::default()
    };
    doc.create_transform(tfm_rot.clone()).unwrap();

    // Transform 2: Child Warp (3x3 grid = 16 points)
    let warp_child = make_id(0x0005, 2);
    let mut tfm_warp = Transform {
        id: warp_child.clone(),
        runtime_id: "ChildWarp".to_string(),
        name: "Child Warp".to_string(),
        parent_id: zipzi_core::TransformId::optional(rot_root.clone()),
        part_id: zipzi_core::PartId::optional(part_child.clone()),
        data: TransformData::Warp(WarpTransform {
            bezier: None,
            rows: 3,
            columns: 3,
            quad: true,
            points: Vec::new(),
        }),
        ..Default::default()
    };
    for r in 0..=3 {
        for c in 0..=3 {
            tfm_warp
                .warp_mut()
                .unwrap()
                .points
                .push(Vec2::new(c as f32 * 50.0 - 75.0, r as f32 * 50.0 - 75.0));
        }
    }
    doc.create_transform(tfm_warp.clone()).unwrap();

    // Bindings for transforms
    for t in 1..=2 {
        let tid = if t == 1 {
            rot_root.clone()
        } else {
            warp_child.clone()
        };
        let track = if t == 1 {
            zipzi_core::SceneTrack::Rotation {
                target_id: tid.into(),
                keyforms: [-1.0f32, 0., 1.]
                    .into_iter()
                    .map(|key| {
                        let mut rotation = tfm_rot.rotation().unwrap().pose;
                        rotation.angle = key * 25.;
                        rotation.scale = 1. + key * 0.1;
                        zipzi_core::RotationKeyform {
                            keys: vec![key],
                            rotation,
                            ..Default::default()
                        }
                    })
                    .collect(),
            }
        } else {
            zipzi_core::SceneTrack::Warp {
                target_id: tid.into(),
                keyforms: [-1.0f32, 0., 1.]
                    .into_iter()
                    .map(|key| {
                        let mut positions = tfm_warp.warp().unwrap().points.clone();
                        for pt in &mut positions {
                            pt.x += key * 15.;
                        }
                        zipzi_core::WarpKeyform {
                            bezier: None,
                            keys: vec![key],
                            positions,
                            ..Default::default()
                        }
                    })
                    .collect(),
            }
        };
        let sb = SceneBinding {
            id: make_id(0x0006, t),
            axes: vec![BindingAxis {
                id: Default::default(),
                samples: Default::default(),
                parameter_id: out_params[0].clone(),
                keys: vec![-1., 0., 1.],
            }],
            track,
        };
        doc.create_scene_binding(sb).unwrap();
    }

    // 10 Meshes (each with 4x4 vertex grid = 16 vertices, 18 triangles)
    for m in 0..10 {
        let mesh_id = make_id(0x0007, m + 1);
        let mut mesh = Mesh {
            id: mesh_id.clone(),
            runtime_id: format!("Mesh_{}", m),
            name: format!("Mesh {}", m),
            texture_asset_id: if m % 2 == 0 {
                asset1.clone().into()
            } else {
                asset2.clone().into()
            },
            part_id: part_child.clone(),
            deformer_id: warp_child.clone(),
            draw_order: Some(m as f32),
            ..Default::default()
        };

        let mut vid: VertexId = 0;
        for r in 0..4 {
            for c in 0..4 {
                mesh.vertex_ids.push(vid);
                vid += 1;
                mesh.base_positions
                    .push(Vec2::new(c as f32 * 20.0 - 30.0, r as f32 * 20.0 - 30.0));
                mesh.uvs.push(Vec2::new(c as f32 / 3.0, r as f32 / 3.0));
            }
        }
        for r in 0..3 {
            for c in 0..3 {
                let v0 = (r * 4 + c) as VertexId;
                let v1 = (r * 4 + c + 1) as VertexId;
                let v2 = ((r + 1) * 4 + c) as VertexId;
                let v3 = ((r + 1) * 4 + c + 1) as VertexId;
                mesh.triangles.push([v0, v1, v2]);
                mesh.triangles.push([v1, v3, v2]);
            }
        }

        doc.create_mesh(mesh.clone()).unwrap();

        let mut mb = MeshBinding {
            id: make_id(0x0008, m + 1),
            mesh_id: mesh_id.clone(),
            axes: vec![
                BindingAxis {
                    id: Default::default(),
                    samples: Default::default(),
                    parameter_id: out_params[1].clone(),
                    keys: vec![-1.0, 0.0, 1.0],
                },
                BindingAxis {
                    id: Default::default(),
                    samples: Default::default(),
                    parameter_id: out_params[2].clone(),
                    keys: vec![-1.0, 1.0],
                },
            ],
            ..Default::default()
        };
        for &k1 in &[-1.0f32, 0.0, 1.0] {
            for &k2 in &[-1.0f32, 1.0] {
                let mut kf = MeshKeyform {
                    keys: vec![k1, k2],
                    positions: mesh.base_positions.clone(),
                    ..Default::default()
                };
                for (v, pt) in kf.positions.iter_mut().enumerate() {
                    pt.x += k1 * 8.0 + k2 * 4.0 + (v as f32 * 0.5);
                    pt.y += k1 * 4.0 - k2 * 6.0;
                }
                mb.keyforms.push(kf);
            }
        }
        doc.create_binding(mb).unwrap();
    }

    (
        doc.freeze().unwrap().state,
        ResourceView::default().with_images(images),
    )
}

fn main() {
    let (doc, resources) = build_benchmark_document(&mut Vec::new());
    let root = tempfile::tempdir().unwrap();
    let io = ProjectIo::new(root.path(), ResourceStore::default());
    let path = root.path().join("benchmark.zipzi");
    let mut receipt = io.save(&doc, &resources, &path, None, || true).unwrap();
    let iterations = std::env::var("ZIPZI_BENCH_ITERATIONS")
        .ok()
        .map(|s| s.parse::<usize>().unwrap())
        .unwrap_or(50);
    assert!(iterations > 0);
    let start = Instant::now();
    for _ in 0..iterations {
        receipt = black_box(
            io.save(
                black_box(&doc),
                &resources,
                &path,
                Some(&receipt.fingerprint),
                || true,
            )
            .unwrap(),
        );
    }
    let save_us = start.elapsed().as_secs_f64() * 1e6 / iterations as f64;
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(io.load(black_box(&path), || true).unwrap());
    }
    let load_us = start.elapsed().as_secs_f64() * 1e6 / iterations as f64;
    println!("Native project boundary ({iterations} iterations): save {save_us:.3} us, load {load_us:.3} us");
    println!(
        "Includes ZIP, resource validation and durable file publication; not a codec-only timing."
    );
}
