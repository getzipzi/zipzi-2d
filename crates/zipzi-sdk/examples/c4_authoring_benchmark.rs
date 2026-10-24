//! Release-only, local-asset C4 capacity/latency/allocation probe.
//! cargo run --release -p zipzi-sdk --example c4_authoring_benchmark -- MODEL3
use std::{sync::atomic::Ordering, time::Instant};
use zipzi_core::{Canvas, Vec2};
use zipzi_sdk::{AuthoringSession, BindingAuthoringEdit, ConversionControl, MeshGridEdit};
#[path = "support/allocation.rs"]
mod allocation;
use allocation::{reset_peak, LIVE, PEAK};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("absolute model3 path"));
    let mut s = AuthoringSession::new(
        "00000000-0000-4000-8000-000000009400",
        Canvas::new(100., 100., Vec2::default(), 1.),
    )?;
    s.import_model3(&path, None)?;
    s.evaluate(&Default::default())?;
    s.drain_events();
    let limits = s.authoring_limits();
    println!(
        "model={} meshes={} bindings={} document_estimated={} limits={limits:?}",
        path.display(),
        s.mesh_ids().len(),
        s.binding_ids().len(),
        s.estimated_content_bytes()
    );
    let mut candidates: Vec<_> = s
        .binding_ids()
        .iter()
        .filter_map(|id| {
            let b = s.document().get_binding(id)?;
            let a = b.axes.iter().find(|a| a.keys.len() > 1)?;
            let value = (f64::from(a.keys[0]) * 0.5 + f64::from(a.keys[1]) * 0.5) as f32;
            if value == a.keys[0] || value == a.keys[1] {
                return None;
            }
            Some((
                b.keyforms.len() * s.document().get_mesh(&b.mesh_id)?.base_positions.len(),
                id.clone(),
                a.parameter_id.clone(),
                value,
            ))
        })
        .collect();
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    assert!(
        !candidates.is_empty(),
        "Model must contain ordinary Mesh grids"
    );
    for (_, binding_id, parameter_id, value) in candidates.into_iter().take(3) {
        let intent = BindingAuthoringEdit::MeshGrid(MeshGridEdit::InsertSample {
            binding_id: binding_id.clone(),
            parameter_id,
            value,
        });
        let mut planning = Vec::new();
        let mut committing = Vec::new();
        let mut plan_peak = 0;
        let mut commit_peak = 0;
        let mut final_cost = None;
        for _ in 0..8 {
            let base = reset_peak();
            let start = Instant::now();
            let p = s.plan_binding_authoring(intent.clone())?;
            planning.push(start.elapsed().as_secs_f64() * 1000.);
            plan_peak = plan_peak.max(PEAK.load(Ordering::Relaxed).saturating_sub(base));
            final_cost = Some(p.cost());
            let base = reset_peak();
            let start = Instant::now();
            let receipt = s.execute_binding_plan(&p)?;
            committing.push(start.elapsed().as_secs_f64() * 1000.);
            assert!(receipt.changed);
            commit_peak = commit_peak.max(PEAK.load(Ordering::Relaxed).saturating_sub(base));
            drop(p);
            s.undo()?;
            s.drain_events();
        }
        planning.sort_by(f64::total_cmp);
        committing.sort_by(f64::total_cmp);
        let cost = final_cost.unwrap();
        assert!(
            plan_peak <= cost.temporary_bytes,
            "planning peak exceeded estimate"
        );
        assert!(
            commit_peak <= cost.temporary_bytes,
            "commit peak exceeded estimate"
        );
        let version = s.version();
        let history = s.history_state();
        let base = reset_peak();
        let control = ConversionControl::with_progress(|n| n < 8);
        let e = s
            .plan_binding_authoring_with_control(intent, &control)
            .unwrap_err();
        assert_eq!(e.code.as_ref(), "AUTHORING_CANCELLED");
        assert_eq!(s.version(), version);
        assert_eq!(s.history_state(), history);
        drop(e);
        drop(control);
        let retained_after_cancel = LIVE.load(Ordering::Relaxed).saturating_sub(base);
        println!("binding={binding_id} plan_p50_ms={:.3} plan_max_ms={:.3} commit_p50_ms={:.3} commit_max_ms={:.3} plan_peak_bytes={plan_peak} commit_peak_bytes={commit_peak} cancel_retained_bytes={retained_after_cancel} cost={cost:?}",planning[4],planning[7],committing[4],committing[7]);
    }
    Ok(())
}
