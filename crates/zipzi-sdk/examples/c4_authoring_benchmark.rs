//! Release-only, local-asset C4 capacity/latency/allocation probe.
//! cargo run --release -p zipzi-sdk --example c4_authoring_benchmark -- MODEL3
use std::{sync::atomic::Ordering, time::Instant};
use zipzi_core::document::{BindingGridEdit, GridOperation};
use zipzi_sdk::{AuthoringSession, ConversionControl, ConversionLimits, Snapshot};
#[path = "support/allocation.rs"]
mod allocation;
use allocation::{reset_peak, LIVE, PEAK};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("absolute model3 path"));
    let (result, imported, _) = zipzi_project::ProjectImporter::default().import_model3(&path);
    let imported = imported.ok_or(result.status)?;
    let mut s = AuthoringSession::new(Snapshot::new(imported.document, imported.resources)?);
    let limits = ConversionLimits::default();
    let doc = s.snapshot().document();
    println!(
        "model={} meshes={} bindings={} limits={limits:?}",
        path.display(),
        doc.mesh_order().len(),
        doc.binding_order().len()
    );
    let mut candidates: Vec<_> = doc
        .binding_order()
        .iter()
        .filter_map(|id| {
            let binding = doc.binding(id)?;
            let axis = binding.axes.iter().find(|axis| axis.keys.len() > 1)?;
            let value = (f64::from(axis.keys[0]) * 0.5 + f64::from(axis.keys[1]) * 0.5) as f32;
            if value == axis.keys[0] || value == axis.keys[1] {
                return None;
            }
            Some((
                binding.keyforms.len() * doc.mesh(&binding.mesh_id)?.base_positions.len(),
                id.clone(),
                axis.id.clone(),
                value,
            ))
        })
        .collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    assert!(
        !candidates.is_empty(),
        "Model must contain ordinary Mesh grids"
    );
    for (_, binding_id, axis_id, value) in candidates.into_iter().take(3) {
        let intent = BindingGridEdit::Edit {
            owner: binding_id.clone(),
            operation: GridOperation::InsertSample {
                axis_id,
                id: uuid::Uuid::new_v4().to_string(),
                value,
            },
        };
        let mut planning = Vec::new();
        let mut committing = Vec::new();
        let mut plan_peak = 0;
        let mut commit_peak = 0;
        let mut final_cost = None;
        for _ in 0..8 {
            let base = reset_peak();
            let start = Instant::now();
            let mut draft = s.begin_draft();
            let outcome = draft.batch(draft.source(), |c| {
                c.edit_binding_grid(intent.clone(), limits, &Default::default())
            })?;
            let p = draft.prepare("Insert sample")?;
            planning.push(start.elapsed().as_secs_f64() * 1000.);
            plan_peak = plan_peak.max(PEAK.load(Ordering::Relaxed).saturating_sub(base));
            final_cost = Some(outcome.work);
            let base = reset_peak();
            let start = Instant::now();
            let receipt = s.publish(p)?;
            committing.push(start.elapsed().as_secs_f64() * 1000.);
            assert!(!receipt.writes.is_empty());
            commit_peak = commit_peak.max(PEAK.load(Ordering::Relaxed).saturating_sub(base));
            s.undo(s.version())?;
            s.take_events();
        }
        planning.sort_by(f64::total_cmp);
        committing.sort_by(f64::total_cmp);
        let cost = final_cost.unwrap();
        let version = s.version();
        let history = s.history_state();
        let base = reset_peak();
        let control = ConversionControl::with_progress(|n| n < 8);
        let mut draft = s.begin_draft();
        let e = draft
            .batch(draft.source(), |c| {
                c.edit_binding_grid(intent, limits, &control)
            })
            .unwrap_err();
        assert_eq!(e.code.as_str(), "AUTHORING_CANCELLED");
        assert_eq!(s.version(), version);
        assert_eq!(s.history_state(), history);
        drop(e);
        drop(control);
        let retained_after_cancel = LIVE.load(Ordering::Relaxed).saturating_sub(base);
        println!("binding={binding_id} plan_p50_ms={:.3} plan_max_ms={:.3} commit_p50_ms={:.3} commit_max_ms={:.3} plan_peak_bytes={plan_peak} commit_peak_bytes={commit_peak} cancel_retained_bytes={retained_after_cancel} cost={cost:?}",planning[4],planning[7],committing[4],committing[7]);
    }
    Ok(())
}
