//! Allocation and default-budget probe for a local native project; never saves it.
//! cargo run --locked -p zipzi-sdk --example manual_mesh_project_profile -- PROJECT
use std::sync::atomic::Ordering;
use zipzi_core::Vec2;
use zipzi_sdk::{AuthoringSession, ManualMeshOperation, ManualVertexTarget, Snapshot};
#[path = "support/allocation.rs"]
mod allocation;
use allocation::{reset_peak, PEAK};

fn insertion(session: &AuthoringSession, id: &str) -> ManualMeshOperation {
    let mesh = session.snapshot().document().mesh(id).unwrap();
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

fn main() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("native project path");
    let path = std::path::Path::new(&path);
    let io = zipzi_project::ProjectIo::new(
        path.parent().unwrap_or(std::path::Path::new(".")),
        Default::default(),
    );
    let loaded = io.load(path, || true)?;
    let mut session = AuthoringSession::new(Snapshot::new(loaded.document, loaded.resources)?);
    let mut ids = session.snapshot().document().mesh_order().to_vec();
    ids.sort_by_key(|id| {
        session
            .snapshot()
            .document()
            .mesh(id)
            .unwrap()
            .vertex_ids
            .len()
    });
    let most_dependencies = ids
        .iter()
        .max_by_key(|id| session.snapshot().document().mesh_dependency_bytes(id));
    let targets: Vec<_> = [
        ids.first(),
        ids.last(),
        session.snapshot().document().mesh_order().first(),
        most_dependencies,
    ]
    .into_iter()
    .flatten()
    .cloned()
    .collect();
    for id in &targets {
        let operation = insertion(&session, id);
        let mesh = session.snapshot().document().mesh(id).unwrap();
        let label = format!("{} vertices={}", mesh.name, mesh.vertex_ids.len());
        let mut draft = session.begin_draft();
        let base = reset_peak();
        let outcome = draft.batch(draft.source(), |c| {
            c.edit_manual_mesh(id, operation, Default::default(), &Default::default())
        })?;
        let prepared = draft.prepare("Mesh allocation probe")?;
        let cost = outcome.report.cost;
        let peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
        let base = reset_peak();
        session.publish(prepared)?;
        let commit_peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
        println!(
            "mesh={label} plan_peak_bytes={peak} commit_peak_bytes={commit_peak} cost={cost:?}"
        );
        session.undo(session.version())?;
        session.redo(session.version())?;
        session.undo(session.version())?;
        session.take_events();

        let mesh = session.snapshot().document().mesh(id).unwrap();
        let triangle = mesh.triangles[0];
        let uv = triangle
            .map(|vertex| mesh.uvs[mesh.vertex_ids.iter().position(|v| *v == vertex).unwrap()]);
        let center = Vec2::new(
            (uv[0].x + uv[1].x + uv[2].x) / 3.,
            (uv[0].y + uv[1].y + uv[2].y) / 3.,
        );
        let drag = session.begin_draft();
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
            let _preview = drag.prepare_preview(drag.source(), |c| {
                c.edit_manual_mesh(id, operation, Default::default(), &Default::default())
            })?;
            let peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
            drag_peak = drag_peak.max(peak);
        }
        println!("mesh={label} drag_peak_bytes={drag_peak}");
    }
    let mut unique = targets;
    unique.sort();
    unique.dedup();
    let operations = unique
        .iter()
        .map(|id| (id.clone(), insertion(&session, id)))
        .collect::<Vec<_>>();
    let mut draft = session.begin_draft();
    let base = reset_peak();
    let reports = draft.batch(draft.source(), |c| {
        operations
            .into_iter()
            .map(|(id, op)| c.edit_manual_mesh(&id, op, Default::default(), &Default::default()))
            .collect::<std::result::Result<Vec<_>, _>>()
    })?;
    let prepared = draft.prepare("Batch allocation probe")?;
    let peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
    let estimate = reports
        .iter()
        .map(|r| r.report.cost.workspace_bytes)
        .max()
        .unwrap();
    session.publish(prepared)?;
    session.undo(session.version())?;
    session.redo(session.version())?;
    println!(
        "batch_meshes={} plan_peak_bytes={peak} temporary_estimate={estimate}",
        reports.len()
    );
    Ok(())
}
