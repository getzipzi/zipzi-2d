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

#[allow(dead_code)]
pub fn sample_document_with_parameters(count: usize) -> anyhow::Result<(Document, TempDir)> {
    use kasane_core::Parameter;

    let directory = tempfile::tempdir()?;
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::new(0., 0.), 1.),
    )?;
    session.edit("Create parameters", None, |edit| {
        for index in 0..count {
            edit.create_parameter(Parameter {
                id: Uuid::from_u128(index as u128 + 1).to_string(),
                name: format!("Parameter {index}"),
                minimum: 0.,
                maximum: 1.,
                default_value: 0.,
                ..Default::default()
            })?;
        }
        Ok(())
    })?;
    let receipt = session.save_project(&directory.path().join("fixture.kasane"), None)?;
    Ok((Document::open(&receipt.manifest)?, directory))
}

#[allow(dead_code)]
pub fn sample_document_with_bound_and_unbound_layers() -> anyhow::Result<(Document, TempDir)> {
    sample_document_with_options(true, true)
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

#[allow(dead_code)]
pub fn sample_document_with_nonuniform_parameter_keys() -> anyhow::Result<(Document, TempDir)> {
    use kasane_core::{BindingAxis, Parameter, Part, PartKeyform, SceneBinding, SceneTrack};
    let assets = tempfile::tempdir()?;
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::new(0., 0.), 1.),
    )?;
    let ids = [Uuid::new_v4().to_string(), Uuid::new_v4().to_string()];
    let part = Uuid::new_v4().to_string();
    let keys = vec![0.13, 0.375, 0.91];
    session.edit("Nonuniform parameter keys", None, |edit| {
        for id in &ids {
            edit.create_parameter(Parameter {
                id: id.clone(),
                minimum: 0.13,
                maximum: 0.91,
                default_value: 0.13,
                decimal_places: 2,
                ..Default::default()
            })?;
        }
        edit.create_part(Part {
            id: part.clone(),
            ..Default::default()
        })?;
        edit.create_scene_binding(SceneBinding {
            id: Uuid::new_v4().to_string(),
            axes: ids
                .iter()
                .map(|id| BindingAxis {
                    parameter_id: id.clone(),
                    keys: keys.clone(),
                })
                .collect(),
            track: SceneTrack::Part {
                target_id: part.into(),
                keyforms: keys
                    .iter()
                    .flat_map(|&y| {
                        keys.iter().map(move |&x| PartKeyform {
                            keys: vec![x, y],
                            draw_order: x + y,
                        })
                    })
                    .collect(),
            },
        })?;
        Ok(())
    })?;
    let receipt = session.save_project(&assets.path().join("nonuniform.kasane"), None)?;
    Ok((Document::open(&receipt.manifest)?, assets))
}
