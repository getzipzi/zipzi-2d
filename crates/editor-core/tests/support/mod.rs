use editor_core::Document;
use kasane_core::{Canvas, Mesh, Vec2};
use kasane_sdk::{AuthoringSession, prepare_png_asset};
use tempfile::TempDir;
use uuid::Uuid;

pub fn sample_document() -> anyhow::Result<(Document, TempDir)> {
    sample_document_with_options(false, false)
}

#[allow(dead_code)]
pub fn sample_document_with_binding(bound: bool) -> anyhow::Result<(Document, TempDir)> {
    sample_document_with_options(bound, false)
}

#[allow(dead_code)]
pub fn sample_document_with_two_layers() -> anyhow::Result<(Document, TempDir)> {
    sample_document_with_options(false, true)
}

fn sample_document_with_options(
    bound: bool,
    two_layers: bool,
) -> anyhow::Result<(Document, TempDir)> {
    let directory = tempfile::tempdir()?;
    let texture_path = directory.path().join("accent.png");
    std::fs::write(&texture_path, include_bytes!("../fixtures/accent.png"))?;
    let asset_id = Uuid::new_v4().to_string();
    let mesh_id = Uuid::new_v4().to_string();
    let asset = prepare_png_asset(&asset_id, "Accent", &texture_path)?;
    let mesh = Mesh {
        id: mesh_id,
        name: "Diamond".into(),
        texture_asset_id: asset_id,
        vertex_ids: vec![0, 1, 2, 3],
        base_positions: vec![
            Vec2::new(128., 24.),
            Vec2::new(232., 128.),
            Vec2::new(128., 232.),
            Vec2::new(24., 128.),
        ],
        uvs: vec![
            Vec2::new(0.5, 0.),
            Vec2::new(1., 0.5),
            Vec2::new(0.5, 1.),
            Vec2::new(0., 0.5),
        ],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        ..Default::default()
    };
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::new(0., 0.), 1.),
    )?;
    session.edit("Create test layer", None, |edit| {
        edit.create_asset(asset)?;
        edit.create_mesh(mesh.clone())?;
        if two_layers {
            let mut second = mesh.clone();
            second.id = Uuid::new_v4().to_string();
            second.name = "Second".into();
            edit.create_mesh(second)?;
        }
        if bound {
            use kasane_core::{BindingAxis, MeshBinding, MeshKeyform, Parameter};
            let parameter_id = Uuid::new_v4().to_string();
            edit.create_parameter(Parameter {
                id: parameter_id.clone(),
                name: "Pose".into(),
                minimum: 0.,
                maximum: 1.,
                default_value: 0.,
                ..Default::default()
            })?;
            edit.create_binding(MeshBinding {
                id: Uuid::new_v4().to_string(),
                mesh_id: mesh.id.clone(),
                axes: vec![BindingAxis {
                    parameter_id,
                    keys: vec![0., 1.],
                }],
                keyforms: vec![0., 1.]
                    .into_iter()
                    .map(|key| MeshKeyform {
                        keys: vec![key],
                        positions: mesh.base_positions.clone(),
                        ..Default::default()
                    })
                    .collect(),
                ..Default::default()
            })?;
        }
        Ok(())
    })?;
    let receipt = session.save_project(&directory.path().join("fixture.kasane"), None)?;
    Ok((Document::open(&receipt.manifest)?, directory))
}
