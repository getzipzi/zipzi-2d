//! Default-budget/allocation probe for a local native project; never saves it.
//! cargo run --locked -p zipzi-sdk --example deletion_project_profile -- PROJECT
use std::{path::Path, sync::atomic::Ordering};
use zipzi_core::document::*;
use zipzi_sdk::{AuthoringSession, Snapshot};
#[path = "support/allocation.rs"]
mod allocation;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).expect("native project path");
    let path = Path::new(&path);
    let io =
        zipzi_project::ProjectIo::new(path.parent().unwrap_or(Path::new(".")), Default::default());
    let loaded = io.load(path, || true)?;
    let mut session = AuthoringSession::new(Snapshot::new(loaded.document, loaded.resources)?);
    let mut meshes = session.snapshot().document().mesh_order().to_vec();
    meshes.sort_by_key(|id| {
        session
            .snapshot()
            .document()
            .mesh(id)
            .unwrap()
            .vertex_ids
            .len()
    });
    let mut targets = vec![
        DeletionTarget::Mesh(meshes.first().unwrap().clone()),
        DeletionTarget::Mesh(meshes.last().unwrap().clone()),
    ];
    targets.extend(
        session
            .snapshot()
            .document()
            .part_order()
            .first()
            .cloned()
            .map(DeletionTarget::Part),
    );
    targets.extend(
        session
            .snapshot()
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
        let mut draft = session.begin_draft();
        let review = draft.batch(draft.source(), |c| {
            c.review_deletion(&request, Default::default(), &Default::default())
        })?;
        let peak = allocation::PEAK
            .load(Ordering::Relaxed)
            .saturating_sub(base);
        println!(
            "target={target:?} review_peak_bytes={peak} work={:?}",
            review.work
        );
        if !review.report.blockers.is_empty()
            || !review.report.structural_blockers.is_empty()
            || !review.report.decisions.is_empty()
        {
            println!("blocked: {:?}", review.report);
            continue;
        }
        let prepared = draft.prepare("Profile deletion")?;
        let base = allocation::reset_peak();
        session.publish(prepared)?;
        let peak = allocation::PEAK
            .load(Ordering::Relaxed)
            .saturating_sub(base);
        println!("target={target:?} commit_peak_bytes={peak}");
        session.undo(session.version())?;
        assert_eq!(
            session.snapshot().content_id(),
            snapshot.snapshot.content_id()
        );
        session.redo(session.version())?;
        session.undo(session.version())?;
        session.take_events();
    }
    Ok(())
}
