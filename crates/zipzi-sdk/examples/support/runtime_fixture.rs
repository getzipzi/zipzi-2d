//! Deterministic, license-free model with geometry, a driven binding, animation
//! and an unsaved RGBA resource. Shared by the R0 probe and later host fixtures.
use std::sync::Arc;
use zipzi_core::{BindingAxis, Canvas, MeshBinding, MeshKeyform, Parameter, Vec2};
use zipzi_sdk::{
    AuthoringSession, MeshGeometry, SdkError, rectangle_grid_geometry, rectangle_mesh,
};

pub fn id(n: usize) -> String {
    format!("70000000-0000-4000-8000-{n:012x}")
}

pub fn build(
    meshes: usize,
    divisions: usize,
    image_side: u32,
) -> Result<AuthoringSession, SdkError> {
    let mut sdk = AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 1.))?;
    let rgba: Arc<[u8]> = (0..image_side as usize * image_side as usize * 4)
        .map(|i| ((i * 37 + i / 11) % 256) as u8)
        .collect();
    let image = sdk.prepare_rgba_image(image_side, image_side, rgba)?;
    sdk.edit("R0 synthetic fixture", None, |edit| {
        edit.create_prepared_image(id(2), "R0 memory texture".into(), &image)?;
        edit.create_parameter(Parameter {
            id: id(3),
            runtime_id: "ParamR0".into(),
            name: "R0 driver".into(),
            ..Default::default()
        })?;
        for index in 0..meshes {
            let mut mesh = rectangle_mesh(
                &id(100 + index),
                &format!("R0 mesh {index}"),
                &id(2),
                Vec2::new(0., 0.),
                Vec2::new(10., 10.),
            )?;
            let grid = rectangle_grid_geometry(
                &MeshGeometry {
                    vertex_ids: mesh.vertex_ids.clone(),
                    positions: mesh.base_positions.clone(),
                    uvs: mesh.uvs.clone(),
                    triangles: mesh.triangles.clone(),
                },
                divisions,
                divisions,
            )?;
            mesh.vertex_ids = grid.vertex_ids;
            mesh.base_positions = grid.positions;
            mesh.uvs = grid.uvs;
            mesh.triangles = grid.triangles;
            let binding = (index == 0).then(|| MeshBinding {
                id: id(4),
                mesh_id: mesh.id.clone(),
                axes: vec![BindingAxis {
                    parameter_id: id(3),
                    keys: vec![-1., 1.],
                    ..Default::default()
                }],
                keyforms: [-1., 1.]
                    .into_iter()
                    .map(|key| MeshKeyform {
                        keys: vec![key],
                        positions: mesh
                            .base_positions
                            .iter()
                            .map(|p| Vec2::new(p.x + key, p.y))
                            .collect(),
                        ..Default::default()
                    })
                    .collect(),
            });
            edit.create_mesh(mesh)?;
            if let Some(binding) = binding {
                edit.create_binding(binding)?;
            }
        }
        edit.import_motion3(&id(10), "R0 motion", r#"{"Version":3,"Meta":{"Duration":1,"Fps":30,"Loop":false,"AreBeziersRestricted":true,"CurveCount":1,"TotalSegmentCount":1,"TotalPointCount":2,"UserDataCount":0,"TotalUserDataSize":0},"Curves":[{"Target":"Parameter","Id":"ParamR0","Segments":[0,-1,0,1,1]}],"UserData":[]}"#)?;
        Ok(())
    })?;
    sdk.drain_events();
    Ok(sdk)
}
