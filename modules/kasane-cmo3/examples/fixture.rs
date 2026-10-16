//! Generate a small editable model for inspection in Cubism Editor.
use kasane_cmo3::{encode_cmo3, Cmo3Options};
use kasane_core::{types::*, Document};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: fixture OUTPUT.cmo3")?;
    let mut doc = Document::new();
    assert!(doc
        .initialize(
            "00000000-0000-4000-8000-000000001000",
            Canvas::new(128., 128., Vec2::new(64., 64.), 64.)
        )
        .is_ok());
    let asset = "00000000-0000-4000-8000-000000001001";
    assert!(doc
        .add_asset(ImageAsset {
            id: asset.into(),
            name: "Artwork".into(),
            source: "assets/texture.png".into(),
            width: 2,
            height: 2,
            ..Default::default()
        })
        .status
        .is_ok());
    let mesh = "00000000-0000-4000-8000-000000001002";
    let rotation = std::env::args().nth(2).is_some();
    let deformer = "00000000-0000-4000-8000-000000001005";
    if rotation {
        assert!(doc
            .create_transform(Transform {
                id: deformer.into(),
                runtime_id: "Rotation".into(),
                name: "Rotation".into(),
                data: TransformData::Rotation(RotationTransform {
                    base_angle: 10.,
                    pose: RotationPose {
                        origin: PreciseVec2::new(64., 64.),
                        angle: 20.,
                        scale: 1.,
                        ..Default::default()
                    }
                }),
                ..Default::default()
            })
            .status
            .is_ok());
    }
    let nested = std::env::args().nth(2).as_deref() == Some("nested");
    let child = "00000000-0000-4000-8000-000000001006";
    if nested {
        assert!(doc
            .create_transform(Transform {
                id: child.into(),
                runtime_id: "ChildRotation".into(),
                name: "Child".into(),
                parent_id: Some(deformer.into()),
                data: TransformData::Rotation(RotationTransform {
                    base_angle: 0.,
                    pose: RotationPose {
                        origin: PreciseVec2::new(0.2, -0.1),
                        angle: -15.,
                        scale: 0.5,
                        ..Default::default()
                    }
                }),
                ..Default::default()
            })
            .status
            .is_ok());
    }
    let mut positions = vec![
        Vec2::new(16., 16.),
        Vec2::new(112., 16.),
        Vec2::new(112., 112.),
        Vec2::new(16., 112.),
    ];
    if rotation {
        positions = vec![
            Vec2::new(-0.75, 0.75),
            Vec2::new(0.75, 0.75),
            Vec2::new(0.75, -0.75),
            Vec2::new(-0.75, -0.75),
        ];
    }
    assert!(doc
        .create_mesh(Mesh {
            id: mesh.into(),
            runtime_id: "ArtMesh".into(),
            name: "可编辑 & <网格>".into(),
            texture_asset_id: asset.into(),
            deformer_id: if nested {
                child.into()
            } else if rotation {
                deformer.into()
            } else {
                String::new()
            },
            vertex_ids: vec![10, 20, 30, 40],
            base_positions: positions.clone(),
            uvs: vec![
                Vec2::new(0., 0.),
                Vec2::new(1., 0.),
                Vec2::new(1., 1.),
                Vec2::new(0., 1.)
            ],
            triangles: vec![[10, 20, 30], [10, 30, 40]],
            draw_order: Some(500.),
            ..Default::default()
        })
        .status
        .is_ok());
    let parameter = "00000000-0000-4000-8000-000000001003";
    assert!(doc
        .create_parameter(Parameter {
            id: parameter.into(),
            runtime_id: "ParamX".into(),
            name: "水平移动".into(),
            ..Default::default()
        })
        .status
        .is_ok());
    assert!(doc
        .create_binding(MeshBinding {
            id: "00000000-0000-4000-8000-000000001004".into(),
            mesh_id: mesh.into(),
            axes: vec![BindingAxis {
                parameter_id: parameter.into(),
                keys: vec![-1., 0., 1.]
            }],
            keyforms: [-1., 0., 1.]
                .into_iter()
                .map(|key| MeshKeyform {
                    keys: vec![key],
                    positions: positions
                        .iter()
                        .map(|p| Vec2::new(p.x + key * 12., p.y))
                        .collect(),
                    draw_order: Some(500.),
                    ..Default::default()
                })
                .collect()
        })
        .status
        .is_ok());
    let mut png = Vec::new();
    {
        let mut e = png::Encoder::new(&mut png, 2, 2);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        e.write_header()?.write_image_data(&[
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 0, 128,
        ])?;
    }
    let artifact = encode_cmo3(
        &doc,
        &|_: &ImageAsset| Ok(png.clone()),
        &Cmo3Options::default(),
    )?;
    let mut frame = kasane_core::DrawableFrame::default();
    assert!(kasane_core::evaluation::evaluate_frame_including_hidden(
        &doc,
        &Default::default(),
        &mut frame
    )
    .is_ok());
    for drawable in frame.drawables {
        eprintln!(
            "Kasane pixel positions: {:?}",
            drawable
                .positions
                .iter()
                .map(|p| (64. + p.x * 64., 64. - p.y * 64.))
                .collect::<Vec<_>>()
        );
    }
    std::fs::write(path, artifact.bytes)?;
    Ok(())
}
