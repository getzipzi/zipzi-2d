use editor_core::Document;
use kasane_core::*;
use kasane_sdk::{AuthoringSession, prepare_png_asset, rectangle_mesh};

fn id(n: u128) -> String {
    uuid::Uuid::from_u128(n).to_string()
}

#[allow(dead_code)]
pub fn rectangle_document(bound: bool) -> anyhow::Result<(Document, tempfile::TempDir)> {
    rectangle_with_texture(bound, include_bytes!("../fixtures/accent.png"))
}

#[allow(dead_code)]
pub fn alpha_document() -> anyhow::Result<(Document, tempfile::TempDir)> {
    rectangle_with_texture(false, include_bytes!("../fixtures/alpha-asymmetric.png"))
}

fn rectangle_with_texture(
    bound: bool,
    pixels: &[u8],
) -> anyhow::Result<(Document, tempfile::TempDir)> {
    let dir = tempfile::tempdir()?;
    let texture = dir.path().join("texture.png");
    std::fs::write(&texture, pixels)?;
    let mut session =
        AuthoringSession::new(&id(900), Canvas::new(256., 256., Vec2::default(), 1.))?;
    let asset = prepare_png_asset(&id(901), "Artwork", &texture)?;
    let mut mesh = rectangle_mesh(
        &id(902),
        "Rectangle",
        &id(901),
        Vec2::new(24., 24.),
        Vec2::new(232., 232.),
    )?;
    mesh.part_id = id(903);
    session.edit("Fixture", None, |edit| {
        edit.create_asset(asset)?;
        edit.create_part(Part {
            id: id(903),
            name: "Part".into(),
            ..Default::default()
        })?;
        edit.create_mesh(mesh.clone())?;
        if bound {
            edit.create_parameter(Parameter {
                id: id(904),
                name: "Pose".into(),
                minimum: 0.,
                maximum: 1.,
                ..Default::default()
            })?;
            edit.create_binding(MeshBinding {
                id: id(905),
                mesh_id: mesh.id.clone(),
                axes: vec![BindingAxis {
                    id: Default::default(),
                    samples: Default::default(),
                    parameter_id: id(904),
                    keys: vec![0., 1.],
                }],
                keyforms: vec![
                    MeshKeyform {
                        keys: vec![0.],
                        positions: mesh.base_positions.clone(),
                        ..Default::default()
                    },
                    MeshKeyform {
                        keys: vec![1.],
                        positions: mesh
                            .base_positions
                            .iter()
                            .map(|p| Vec2::new(p.x + 10., p.y + 5.))
                            .collect(),
                        ..Default::default()
                    },
                ],
            })?;
            edit.create_parameter(Parameter {
                id: id(906),
                name: "Blend".into(),
                minimum: 0.,
                maximum: 1.,
                kind: ParameterKind::BlendShape,
                ..Default::default()
            })?;
            edit.create_blend_key_table(BlendShapeKeyTable {
                samples: Default::default(),
                base_sample_id: Default::default(),
                id: id(907),
                parameter_id: id(906),
                keys: vec![0., 1.],
                base_key_idx: 0,
            })?;
            edit.create_blend_binding(BlendShapeBinding {
                id: id(908),
                target_id: mesh.id.clone(),
                target_kind: BlendShapeTargetKind::Mesh,
                key_table_id: id(907),
                constraint_ids: vec![],
                keyforms: DeltaKeyforms::Mesh(vec![
                    DeltaMeshKeyform {
                        positions: vec![Vec2::default(); 4],
                        ..Default::default()
                    },
                    DeltaMeshKeyform {
                        positions: vec![Vec2::new(2., 3.); 4],
                        ..Default::default()
                    },
                ]),
            })?;
            let other = rectangle_mesh(
                &id(909),
                "Other",
                &id(901),
                Vec2::new(24., 24.),
                Vec2::new(232., 232.),
            )?;
            edit.create_mesh(other)?;
            edit.create_glue(Glue {
                id: id(910),
                runtime_id: "seam".into(),
                name: "Seam".into(),
                mesh_a_id: (mesh.id.clone()).into(),
                mesh_b_id: (id(909)).into(),
                pairs: vec![GlueVertexPair {
                    vertex_a: 0,
                    vertex_b: 0,
                    weight_a: 0.5,
                    weight_b: 0.5,
                }],
                intensity: 0.5,
                binding: None,
            })?;
        }
        Ok(())
    })?;
    let saved = session.save_project(&dir.path().join("rectangle.kasane"), None)?;
    let mut document = Document::open(&saved.manifest)?;
    document.select(&mesh.id)?;
    Ok((document, dir))
}
