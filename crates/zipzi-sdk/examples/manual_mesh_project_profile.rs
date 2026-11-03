//! Allocation and default-budget probe for a local native project; never saves it.
//! cargo run --locked -p zipzi-sdk --example manual_mesh_project_profile -- PROJECT
use std::sync::atomic::Ordering;
use zipzi_core::{ModelSetup, Vec2};
use zipzi_sdk::{AuthoringSession, ManualMeshOperation, ManualVertexTarget};
#[path = "support/allocation.rs"]
mod allocation;
use allocation::{reset_peak, PEAK};

fn insertion(session: &AuthoringSession, id: &str) -> ManualMeshOperation {
    let mesh = session.document().get_mesh(id).unwrap();
    let uv = mesh.triangles[0]
        .map(|id| mesh.uvs[mesh.vertex_ids.iter().position(|v| *v == id).unwrap()]);
    ManualMeshOperation::InsertVertex {
        id: mesh.vertex_ids.iter().max().unwrap() + 1,
        uv: Vec2::new(
            (uv[0].x + uv[1].x + uv[2].x) / 3.,
            (uv[0].y + uv[1].y + uv[2].y) / 3.,
        ),
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("native project path");
    let mut session = AuthoringSession::new(
        "00000000-0000-4000-8000-000000000001",
        ModelSetup::default(),
    )?;
    session.open_project(std::path::Path::new(&path), None)?;
    let mut ids = session.mesh_ids().to_vec();
    ids.sort_by_key(|id| session.document().get_mesh(id).unwrap().vertex_ids.len());
    let most_dependencies = ids
        .iter()
        .max_by_key(|id| session.document().estimated_mesh_dependency_bytes(id));
    let targets: Vec<_> = [
        ids.first(),
        ids.last(),
        session.mesh_ids().first(),
        most_dependencies,
    ]
    .into_iter()
    .flatten()
    .cloned()
    .collect();
    println!(
        "document_bytes={} snapshot_bytes={} limits={:?}",
        session.document().estimated_content_bytes(),
        session.document().estimated_snapshot_bytes(),
        session.authoring_limits()
    );
    for id in &targets {
        let operation = insertion(&session, id);
        let mesh = session.document().get_mesh(id).unwrap();
        let label = format!("{} vertices={}", mesh.name, mesh.vertex_ids.len());
        let mut edit = session.begin_owned_edit("mesh allocation probe", None)?;
        let base = reset_peak();
        let plan = edit.plan_manual_mesh(id, operation)?;
        let cost = plan.report.cost;
        let peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
        assert!(
            peak <= cost.temporary_bytes,
            "plan peak {peak} exceeds {cost:?}"
        );
        edit.apply_manual_mesh_plan(plan)?;
        let base = reset_peak();
        edit.commit_to(&mut session)?;
        let commit_peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
        assert!(
            commit_peak <= cost.temporary_bytes,
            "commit peak {commit_peak} exceeds {cost:?}"
        );
        println!(
            "mesh={label} plan_peak_bytes={peak} commit_peak_bytes={commit_peak} cost={cost:?}"
        );
        session.undo()?;
        session.redo()?;
        session.undo()?;
        session.drain_events();

        let mesh = session.document().get_mesh(id).unwrap();
        let triangle = mesh.triangles[0];
        let uv = triangle
            .map(|vertex| mesh.uvs[mesh.vertex_ids.iter().position(|v| *v == vertex).unwrap()]);
        let center = Vec2::new(
            (uv[0].x + uv[1].x + uv[2].x) / 3.,
            (uv[0].y + uv[1].y + uv[2].y) / 3.,
        );
        let mut drag = session.begin_owned_edit("drag allocation probe", None)?;
        let mut drag_peak = 0;
        for frame in 1..=3 {
            let operation = ManualMeshOperation::MoveVertices {
                targets: vec![ManualVertexTarget {
                    id: triangle[0],
                    uv: Vec2::new(
                        uv[0].x + (center.x - uv[0].x) * frame as f32 * 0.001,
                        uv[0].y + (center.y - uv[0].y) * frame as f32 * 0.001,
                    ),
                }],
                motion_from: None,
            };
            let base = reset_peak();
            let plan = drag.plan_manual_mesh(id, operation)?;
            let peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
            assert!(
                peak <= plan.report.cost.temporary_bytes,
                "drag peak {peak} exceeds {:?}",
                plan.report.cost
            );
            drag_peak = drag_peak.max(peak);
            drag.apply_manual_mesh_plan(plan)?;
        }
        drag.commit_to(&mut session)?;
        session.undo()?;
        session.drain_events();
        println!("mesh={label} drag_peak_bytes={drag_peak}");
    }
    let mut unique = targets;
    unique.sort();
    unique.dedup();
    let operations = unique
        .iter()
        .map(|id| (id.clone(), insertion(&session, id)))
        .collect();
    let mut edit = session.begin_owned_edit("batch allocation probe", None)?;
    let base = reset_peak();
    let reports = edit.edit_manual_mesh_batch(operations)?;
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
    let estimate = reports
        .iter()
        .map(|r| r.cost.temporary_bytes)
        .max()
        .unwrap();
    assert!(peak <= estimate, "batch peak {peak} exceeds {estimate}");
    edit.commit_to(&mut session)?;
    session.undo()?;
    session.redo()?;
    println!(
        "batch_meshes={} plan_peak_bytes={peak} temporary_estimate={estimate}",
        reports.len()
    );
    Ok(())
}
