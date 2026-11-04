//! Default-budget/allocation probe for a local native project; never saves it.
//! cargo run --locked -p zipzi-sdk --example deletion_project_profile -- PROJECT
use std::{path::Path, sync::atomic::Ordering};
use zipzi_core::{document::*, ModelSetup};
use zipzi_sdk::{AuthoringSession, DeletionIntent, DeletionOptions};
#[path = "support/allocation.rs"]
mod allocation;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("native project path");
    let mut session = AuthoringSession::new(
        "00000000-0000-4000-8000-000000000001",
        ModelSetup::default(),
    )?;
    session.open_project(Path::new(&path), None)?;
    println!(
        "document_bytes={} snapshot_bytes={} limits={:?}",
        session.document().estimated_content_bytes(),
        session.document().estimated_snapshot_bytes(),
        session.authoring_limits()
    );
    let mut meshes = session.mesh_ids().to_vec();
    meshes.sort_by_key(|id| session.document().get_mesh(id).unwrap().vertex_ids.len());
    let mut targets = vec![
        DeletionTarget::Mesh(meshes.first().unwrap().clone()),
        DeletionTarget::Mesh(meshes.last().unwrap().clone()),
    ];
    targets.extend(
        session
            .document()
            .part_order()
            .first()
            .cloned()
            .map(DeletionTarget::Part),
    );
    targets.extend(
        session
            .document()
            .transform_order()
            .first()
            .cloned()
            .map(DeletionTarget::Deformer),
    );
    for target in targets {
        let request = DeletionRequest {
            roots: vec![target.clone()],
            ..Default::default()
        };
        let snapshot = session.read_snapshot();
        let base = allocation::reset_peak();
        let review = snapshot.prepare_deletion_review(
            DeletionIntent {
                request: request.clone(),
                ..Default::default()
            },
            session.authoring_limits(),
            &ConversionControl::default(),
        )?;
        let peak = allocation::PEAK
            .load(Ordering::Relaxed)
            .saturating_sub(base);
        println!(
            "target={target:?} review_peak_bytes={peak} cost={:?} summary={:?}",
            review.cost(),
            review.summary()
        );
        assert!(peak <= review.cost().temporary_bytes);
        let analysis = session.analyze_deletion(&request)?;
        let plan = session.plan_deletion(
            &request,
            &DeletionOptions {
                accepted_export_failures: analysis.new_export_failures,
            },
        )?;
        let base = allocation::reset_peak();
        session.execute_deletion_plan(&plan)?;
        let peak = allocation::PEAK
            .load(Ordering::Relaxed)
            .saturating_sub(base);
        println!("target={target:?} commit_peak_bytes={peak}");
        assert!(peak <= review.cost().temporary_bytes);
        session.undo()?;
        assert!(session.document().same_content(snapshot.document()));
        session.redo()?;
        session.undo()?;
        session.drain_events();
    }
    Ok(())
}
