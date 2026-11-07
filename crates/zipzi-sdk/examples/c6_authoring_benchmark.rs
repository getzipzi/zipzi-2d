//! Release-only query/structure probe; does not save or change source assets.
//! cargo run --release -p zipzi-sdk --example c6_authoring_benchmark -- MODEL3
use std::{hint::black_box, sync::atomic::Ordering, time::Instant};
use zipzi_contract::authoring::ObjectKind;
use zipzi_core::document::*;
use zipzi_sdk::{AuthoringSession, Snapshot};
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
    let (result, imported, _) = zipzi_project::ProjectImporter::default().import_model3(&path);
    let imported = imported.ok_or(result.status)?;
    let mut sdk = AuthoringSession::new(Snapshot::new(imported.document, imported.resources)?);
    let mut draft = sdk.begin_draft();
    draft.batch(draft.source(), |edit| {
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
    sdk.publish(draft.prepare("Probe setup")?)?;
    sdk.take_events();
    let asset = sdk
        .snapshot()
        .document()
        .asset_order()
        .first()
        .expect("model needs an image")
        .clone();
    let mesh = sdk
        .snapshot()
        .document()
        .mesh_order()
        .first()
        .expect("model needs a mesh")
        .clone();
    let group = CdiParameterGroup {
        id: id(5),
        runtime_id: "C6Probe".into(),
        name: "Probe".into(),
        parent_id: None,
        extensions: Default::default(),
    };
    println!(
        "{}",
        serde_json::json!({"model": path, "meshes": sdk.snapshot().document().mesh_order().len(),
        "bindings": sdk.snapshot().document().binding_order().len(),
        "warmup": 4})
    );
    let version = sdk.version();
    let history = sdk.history_state();
    let evaluation = sdk.snapshot().content_id();
    let mut capabilities = Samples::default();
    let mut hover = Samples::default();
    let mut blocked = Samples::default();
    for i in 0..132 {
        capabilities.measure(i, || {
            Ok(black_box(
                sdk.snapshot()
                    .document()
                    .scene_capabilities(ObjectKind::Mesh, &mesh)?,
            ))
        })?;
        hover.measure(i, || {
            let mut draft = sdk.begin_draft();
            draft.batch(draft.source(), |c| {
                c.edit_motion(MotionEdit::MoveRegistrations {
                    ids: vec![id(4)],
                    destination: MotionContainer::Group(id(2)),
                    before: None,
                })
            })?;
            Ok(black_box(draft.prepare("Move registration")?))
        })?;
        blocked.measure(i, || {
            let mut draft = sdk.begin_draft();
            let error = draft
                .batch(draft.source(), |c| {
                    c.edit_resource(ResourceEdit::Delete {
                        id: asset.clone(),
                        replacement: None,
                    })
                })
                .unwrap_err();
            Ok(black_box(error))
        })?;
    }
    assert_eq!(sdk.version(), version);
    assert_eq!(sdk.history_state(), history);
    assert_eq!(sdk.snapshot().content_id(), evaluation);
    assert!(sdk.take_events().is_empty());
    capabilities.report("mesh_capabilities");
    hover.report("registration_hover_plan");
    blocked.report("used_texture_delete_plan");

    macro_rules! structure {
        ($name:literal, $intent:expr, $command:ident) => {{
            let mut planning = Samples::default();
            let mut committing = Samples::default();
            let mut undoing = Samples::default();
            for i in 0..36 {
                let history = sdk.history_state().undo_steps;
                let plan = planning.measure(i, || {
                    let mut draft = sdk.begin_draft();
                    draft.batch(draft.source(), |c| c.$command($intent))?;
                    Ok(draft.prepare($name)?)
                })?;
                let receipt = committing.measure(i, || Ok(sdk.publish(plan)?))?;
                assert!(!receipt.writes.is_empty());
                assert_eq!(sdk.history_state().undo_steps, history + 1);
                undoing.measure(i, || Ok(sdk.undo(sdk.version())?))?;
                assert_eq!(sdk.history_state().undo_steps, history);
                assert_eq!(sdk.take_events().len(), 2);
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
        edit_motion
    );
    structure!(
        "empty_parameter_group",
        OrganizationEdit::CreateGroup {
            group: group.clone(),
            before: None
        },
        edit_organization
    );
    structure!(
        "texture_record_copy",
        ResourceEdit::Duplicate {
            source: asset.clone(),
            id: id(6),
            name: "Probe copy".into(),
            before: None
        },
        edit_resource
    );
    Ok(())
}
