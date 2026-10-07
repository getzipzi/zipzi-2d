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

pub fn sample_document_with_parts() -> anyhow::Result<(Document, TempDir)> {
    let (mut document, assets) = sample_document_with_two_layers()?;
    let meshes: Vec<_> = document.mesh_tree()[0]
        .children
        .iter()
        .map(|node| node.id.clone())
        .collect();
    let saved = document.save(&assets.path().join("parts.kasane"))?;
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::default(), 1.),
    )?;
    session.open_project(&saved.manifest, None)?;
    let id = |value| Uuid::from_u128(value).to_string();
    session.edit("Parts fixture", None, |edit| {
        for (value, name, parent) in [
            (101, "Face", None),
            (102, "Body", None),
            (103, "Eyes", Some(101)),
        ] {
            edit.create_part(kasane_core::Part {
                id: id(value),
                name: name.into(),
                parent_id: parent.map(id).unwrap_or_default(),
                ..Default::default()
            })?;
        }
        edit.set_mesh_part(&meshes[0], &id(101))?;
        edit.set_mesh_part(&meshes[1], &id(103))
    })?;
    let saved = session.save_project(&saved.manifest, None)?;
    Ok((Document::open(&saved.manifest)?, assets))
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
pub fn sample_document_with_parameter_groups() -> anyhow::Result<(Document, TempDir)> {
    use kasane_core::{Parameter, document::CdiParameterGroup};

    let directory = tempfile::tempdir()?;
    let mut session = AuthoringSession::new(
        &Uuid::new_v4().to_string(),
        Canvas::new(256., 256., Vec2::default(), 1.),
    )?;
    session.edit("Grouped parameters", None, |edit| {
        for (index, name, parent) in [
            (101, "Face", None),
            (102, "Eyes", Some(101)),
            (103, "Body", None),
            (104, "Expressions", None),
        ] {
            edit.create_parameter_group(CdiParameterGroup {
                id: Uuid::from_u128(index).to_string(),
                runtime_id: name.into(),
                name: name.into(),
                parent_id: parent.map(|id| Uuid::from_u128(id).to_string()),
                extensions: Default::default(),
            })?;
        }
        // Deliberately interleave memberships to exercise presentation ordering.
        for (index, name, group) in [
            (1, "Angle X", Some(101)),
            (2, "Body Angle", Some(103)),
            (3, "Eye Open L", Some(102)),
            (4, "Eye Open R", Some(102)),
            (5, "Breath", None),
        ] {
            let id = Uuid::from_u128(index).to_string();
            edit.create_parameter(Parameter {
                id: id.clone(),
                runtime_id: format!("Param{index}"),
                name: name.into(),
                minimum: 0.,
                maximum: 1.,
                default_value: 0.,
                ..Default::default()
            })?;
            let group = group.map(|id| Uuid::from_u128(id).to_string());
            edit.set_parameter_group(&id, group.as_deref())?;
        }
        Ok(())
    })?;
    let saved = session.save_project(&directory.path().join("grouped.kasane"), None)?;
    Ok((Document::open(&saved.manifest)?, directory))
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
