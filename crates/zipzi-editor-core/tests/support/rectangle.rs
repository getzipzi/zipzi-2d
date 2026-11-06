use super::{
    fixture_session, image_fixture, insert_blend_binding, insert_blend_table, insert_glue,
    insert_image, publish_fixture, save_fixture,
};
use zipzi_core::*;
use zipzi_editor_core::Document;
use zipzi_sdk::rectangle_mesh;

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

#[allow(dead_code)]
pub fn cropped_alpha_document() -> anyhow::Result<(Document, tempfile::TempDir)> {
    rectangle_with_texture(
        false,
        include_bytes!("../../../../fixtures/mesh-generation/opaque32.png"),
    )
}

fn rectangle_with_texture(
    bound: bool,
    pixels: &[u8],
) -> anyhow::Result<(Document, tempfile::TempDir)> {
    let dir = tempfile::tempdir()?;
    let texture = dir.path().join("texture.png");
    std::fs::write(&texture, pixels)?;
    let mut session = fixture_session(&id(900), Canvas::new(256., 256., Vec2::default(), 1.))?;
    let asset = image_fixture(&id(901), "Artwork", &texture)?;
    let mut mesh = rectangle_mesh(
        &id(902),
        "Rectangle",
        &id(901),
        Vec2::new(24., 24.),
        Vec2::new(232., 232.),
    )?;
    mesh.part_id = id(903);
    publish_fixture(&mut session, "Fixture", |edit| {
        insert_image(edit, asset)?;
        super::insert_fixture_part(
            edit,
            Part {
                id: id(903),
                name: "Part".into(),
                ..Default::default()
            },
        )?;
        super::insert_fixture_mesh(edit, mesh.clone())?;
        if bound {
            super::insert_fixture_parameter(
                edit,
                Parameter {
                    id: id(904),
                    name: "Pose".into(),
                    minimum: 0.,
                    maximum: 1.,
                    ..Default::default()
                },
            )?;
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
            super::insert_fixture_parameter(
                edit,
                Parameter {
                    id: id(906),
                    name: "Blend".into(),
                    minimum: 0.,
                    maximum: 1.,
                    kind: ParameterKind::BlendShape,
                    ..Default::default()
                },
            )?;
            insert_blend_table(
                edit,
                BlendShapeKeyTable {
                    samples: Default::default(),
                    base_sample_id: Default::default(),
                    id: id(907),
                    parameter_id: id(906),
                    keys: vec![0., 1.],
                    base_key_idx: 0,
                },
            )?;
            insert_blend_binding(
                edit,
                BlendShapeBinding {
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
                },
            )?;
            let other = rectangle_mesh(
                &id(909),
                "Other",
                &id(901),
                Vec2::new(24., 24.),
                Vec2::new(232., 232.),
            )?;
            super::insert_fixture_mesh(edit, other)?;
            insert_glue(
                edit,
                Glue {
                    id: id(910),
                    runtime_id: "seam".into(),
                    name: "Seam".into(),
                    mesh_a_id: (mesh.id.clone()).into(),
                    mesh_b_id: (id(909)).into(),
                    pairs: vec![GlueVertexPair {
                        vertex_a: Some(0),
                        vertex_b: Some(0),
                        weight_a: 0.5,
                        weight_b: 0.5,
                        ..Default::default()
                    }],
                    intensity: 0.5,
                    binding: None,
                },
            )?;
        }
        Ok(())
    })?;
    let saved = save_fixture(&mut session, &dir.path().join("rectangle.zipzi"))?;
    let mut document = Document::open(&saved.path)?;
    document.select(&mesh.id)?;
    Ok((document, dir))
}

#[path = "../../../../fixtures/mesh-edit-dependencies.rs"]
mod dependency_fixture;
#[allow(dead_code)]
pub fn mixed_document() -> anyhow::Result<(Document, tempfile::TempDir)> {
    let dir = tempfile::tempdir()?;
    let id = dependency_fixture::id;
    let texture = dir.path().join("mixed.png");
    std::fs::write(&texture, include_bytes!("../fixtures/alpha-asymmetric.png"))?;
    let mut sdk = fixture_session(&id(1), zipzi_core::ModelSetup::default())?;
    publish_fixture(&mut sdk, "fixture", |e| {
        dependency_fixture::install(e)?;
        insert_image(e, image_fixture(&id(40), "Source", &texture)?)?;
        let mut m = e.read_document()?.mesh(&id(2)).unwrap().clone();
        m.texture_asset_id = id(40).into();
        e.replace_mesh(m)?;
        insert_image(e, image_fixture(&id(41), "Context", &texture)?)?;
        let mut context = e.read_document()?.mesh(&id(4)).unwrap().clone();
        context.texture_asset_id = id(41).into();
        e.replace_mesh(context)
    })?;
    let saved = save_fixture(&mut sdk, &dir.path().join("mixed.zipzi"))?;
    let mut doc = Document::open(&saved.path)?;
    doc.select(&id(2))?;
    Ok((doc, dir))
}
