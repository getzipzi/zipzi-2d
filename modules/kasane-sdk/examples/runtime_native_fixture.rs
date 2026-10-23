//! License-free native control baseline, with independent nonsingular parents.
//! cargo run --locked --release -p kasane-sdk --example runtime_native_fixture -- OUTPUT
use kasane_core::{
    Canvas, PreciseVec2, RotationPose, RotationTransform, Transform, TransformData, Vec2,
    WarpTransform,
};
use kasane_sdk::{rectangle_grid_geometry, rectangle_mesh, AuthoringSession, MeshGeometry};
use std::path::PathBuf;

fn id(n: usize) -> String {
    format!("71000000-0000-4000-8000-{n:012x}")
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("fresh absolute output path")?,
    );
    if !output.is_absolute() || output.exists() {
        return Err("output must be absolute and must not exist".into());
    }
    // Match the editor's 540-model-unit rotation axis so its tip remains in view.
    let mut sdk = AuthoringSession::new(&id(1), Canvas::new(4000., 3000., Vec2::default(), 1.))?;
    let asset = sdk.prepare_rgba_asset(
        &id(2),
        "asymmetric control texture",
        2,
        2,
        vec![
            255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255,
        ]
        .into(),
    )?;
    sdk.edit("Native control fixture", None, |edit| {
        edit.create_asset(asset)?;
        edit.create_transform(Transform {
            id: id(3),
            name: "Root Warp".into(),
            data: TransformData::Warp(WarpTransform {
                bezier: None,
                rows: 2,
                columns: 2,
                quad: true,
                points: (0..3)
                    .flat_map(|y| {
                        (0..3).map(move |x| {
                            Vec2::new(-2000. + x as f32 * 2000., -2000. + y as f32 * 2000.)
                        })
                    })
                    .collect(),
            }),
            ..Default::default()
        })?;
        edit.create_transform(Transform {
            id: id(4),
            name: "Root Rotation".into(),
            data: TransformData::Rotation(RotationTransform {
                pose: RotationPose {
                    origin: PreciseVec2::new(1000., 0.),
                    angle: 180.,
                    ..Default::default()
                },
                ..Default::default()
            }),
            ..Default::default()
        })?;
        for (mesh_id, parent, minimum, maximum) in [
            (id(5), id(3), Vec2::new(0.1, 0.1), Vec2::new(0.9, 0.9)),
            (id(6), id(4), Vec2::new(-500., -500.), Vec2::new(500., 500.)),
        ] {
            let mut mesh = rectangle_mesh(&mesh_id, &mesh_id, &id(2), minimum, maximum)?;
            let grid = rectangle_grid_geometry(
                &MeshGeometry {
                    vertex_ids: mesh.vertex_ids.clone(),
                    positions: mesh.base_positions.clone(),
                    uvs: mesh.uvs.clone(),
                    triangles: mesh.triangles.clone(),
                },
                12,
                12,
            )?;
            mesh.vertex_ids = grid.vertex_ids;
            mesh.base_positions = grid.positions;
            mesh.uvs = grid.uvs;
            mesh.triangles = grid.triangles;
            mesh.deformer_id = parent;
            edit.create_mesh(mesh)?;
        }
        // Keep the authored Warp's outer anchors inside the native benchmark's
        // 40-pixel inset, while retaining a reachable 540-unit Rotation axis.
        let mut bounds = rectangle_mesh(
            &id(7),
            "Fit bounds",
            &id(2),
            Vec2::new(-2250., -2250.),
            Vec2::new(2250., 2250.),
        )?;
        bounds.appearance.opacity = 0.03;
        edit.create_mesh(bounds)?;
        Ok(())
    })?;
    assert!(sdk.validate_structure().is_empty());
    let before = sdk.evaluate(&Default::default())?;
    assert_eq!(before.drawables.len(), 3);
    assert_eq!(before.drawables[0].positions[0], Vec2::new(-1600., -1600.));
    let rotated = before.drawables[1].positions[0];
    assert!((rotated.x - 1500.).abs() < 0.001 && (rotated.y - 500.).abs() < 0.001);
    let saved = sdk.save_project(&output, None)?;
    let mut reopened = AuthoringSession::new(&id(99), kasane_core::ModelSetup::default())?;
    reopened.open_project(&saved.manifest, None)?;
    assert_eq!(reopened.evaluate(&Default::default())?, before);
    println!("{}", saved.manifest.display());
    Ok(())
}
