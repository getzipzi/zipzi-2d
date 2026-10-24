//! Release-only query/structure probe; does not save or change source assets.
//! cargo run --release -p zipzi-sdk --example c6_authoring_benchmark -- MODEL3
use std::{hint::black_box, sync::atomic::Ordering, time::Instant};
use zipzi_core::{document::*, Canvas, Vec2};
use zipzi_sdk::{AuthoringSession, ObjectKind};
#[path = "support/allocation.rs"]
mod allocation;
use allocation::{reset_peak, PEAK};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[derive(Default)]
struct Samples(Vec<(f64, usize)>);
impl Samples {
    fn measure<T>(&mut self, iteration: usize, operation: impl FnOnce() -> Result<T>) -> Result<T> {
        let base = reset_peak();
        let start = Instant::now();
        let value = operation()?;
        let ms = start.elapsed().as_secs_f64() * 1000.;
        let peak = PEAK.load(Ordering::Relaxed).saturating_sub(base);
        if iteration >= 4 {
            self.0.push((ms, peak));
        }
        Ok(value)
    }
    fn report(mut self, case: &str) {
        self.0.sort_by(|a, b| a.0.total_cmp(&b.0));
        println!(
            "{}",
            serde_json::json!({
                "case": case, "samples": self.0.len(), "p50_ms": self.0[self.0.len()/2].0,
                "p95_ms": self.0[(self.0.len()*95/100).min(self.0.len()-1)].0,
                "max_ms": self.0.last().unwrap().0,
                "peak_live_bytes": self.0.iter().map(|s| s.1).max().unwrap(),
            })
        );
    }
}
fn id(n: u32) -> String {
    format!("60000000-0000-4000-8000-{n:012x}")
}

fn main() -> Result<()> {
    let path = std::path::PathBuf::from(std::env::args().nth(1).expect("absolute model3 path"));
    let mut sdk = AuthoringSession::new(&id(1), Canvas::new(100., 100., Vec2::default(), 1.))?;
    sdk.import_model3(&path, None)?;
    sdk.evaluate(&Default::default())?;
    sdk.edit("Probe setup", None, |edit| {
        edit.edit_motion(MotionEdit::CreateGroup {
            id: id(2),
            name: "Probe group".into(),
        })?;
        edit.edit_motion(MotionEdit::Create {
            id: id(3),
            name: "Probe clip".into(),
            registration_id: id(4),
            destination: MotionContainer::Root,
        })
    })?;
    sdk.drain_events();
    let asset = sdk
        .asset_ids()
        .first()
        .expect("model needs an image")
        .clone();
    let mesh = sdk.mesh_ids().first().expect("model needs a mesh").clone();
    let handle = sdk.handle(ObjectKind::Mesh, &mesh)?;
    let group = CdiParameterGroup {
        id: id(5),
        runtime_id: "C6Probe".into(),
        name: "Probe".into(),
        parent_id: None,
        extensions: Default::default(),
    };
    println!(
        "{}",
        serde_json::json!({"model": path, "meshes": sdk.mesh_ids().len(),
        "bindings": sdk.binding_ids().len(), "document_estimated_bytes": sdk.estimated_content_bytes(),
        "authoring_limits": sdk.authoring_limits(), "warmup": 4})
    );
    let version = sdk.version();
    let history = sdk.history_lengths();
    let evaluation = sdk.evaluation_revision();
    let mut capabilities = Samples::default();
    let mut hover = Samples::default();
    let mut blocked = Samples::default();
    for i in 0..132 {
        capabilities.measure(i, || Ok(black_box(sdk.scene_capabilities(&handle)?)))?;
        hover.measure(i, || {
            Ok(black_box(sdk.plan_motion_edit(
                MotionEdit::MoveRegistrations {
                    ids: vec![id(4)],
                    destination: MotionContainer::Group(id(2)),
                    before: None,
                },
            )?))
        })?;
        blocked.measure(i, || {
            let error = sdk
                .plan_resource_edit(ResourceEdit::Delete {
                    id: asset.clone(),
                    replacement: None,
                })
                .unwrap_err();
            assert!(!error.referrers.is_empty());
            Ok(black_box(error))
        })?;
    }
    assert_eq!(sdk.version(), version);
    assert_eq!(sdk.history_lengths(), history);
    assert_eq!(sdk.evaluation_revision(), evaluation);
    assert!(sdk.drain_events().is_empty());
    capabilities.report("mesh_capabilities");
    hover.report("registration_hover_plan");
    blocked.report("used_texture_delete_plan");

    macro_rules! structure {
        ($name:literal, $intent:expr, $plan:ident, $execute:ident) => {{
            let mut planning = Samples::default();
            let mut committing = Samples::default();
            let mut undoing = Samples::default();
            for i in 0..36 {
                let history = sdk.history_lengths().0;
                let plan = planning.measure(i, || Ok(sdk.$plan($intent)?))?;
                let receipt = committing.measure(i, || Ok(sdk.$execute(&plan)?))?;
                assert!(receipt.changed);
                assert_eq!(sdk.history_lengths().0, history + 1);
                undoing.measure(i, || Ok(sdk.undo()?))?;
                assert_eq!(sdk.history_lengths().0, history);
                assert_eq!(sdk.drain_events().len(), 2);
            }
            planning.report(concat!($name, "_plan"));
            committing.report(concat!($name, "_commit"));
            undoing.report(concat!($name, "_undo"));
        }};
    }
    structure!(
        "registration_move",
        MotionEdit::MoveRegistrations {
            ids: vec![id(4)],
            destination: MotionContainer::Group(id(2)),
            before: None
        },
        plan_motion_edit,
        execute_motion_plan
    );
    structure!(
        "empty_parameter_group",
        OrganizationEdit::CreateGroup {
            group: group.clone(),
            before: None
        },
        plan_organization_edit,
        execute_organization_plan
    );
    structure!(
        "texture_record_copy",
        ResourceEdit::Duplicate {
            source: asset.clone(),
            id: id(6),
            name: "Probe copy".into(),
            before: None
        },
        plan_resource_edit,
        execute_resource_plan
    );
    Ok(())
}
