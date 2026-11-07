use zipzi_animation::PhysicsPreview;
use zipzi_core::{state::DocumentState, Canvas, Parameter, Vec2};
use zipzi_project::import_physics3;

const DOC: &str = "00000000-0000-4000-8000-000000000f21";
const X: &str = "00000000-0000-4000-8000-000000000f22";
const Y: &str = "00000000-0000-4000-8000-000000000f23";
const PHYSICS: &str = "00000000-0000-4000-8000-000000000f24";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("physics3 path required")?;
    let stabilize = std::env::args().nth(2).as_deref() == Some("--stabilize");
    let state = DocumentState::new(DOC, Canvas::new(100.0, 100.0, Vec2::default(), 10.0).into())?;
    let mut builder = state.builder();
    for (id, runtime) in [(X, "ParamX"), (Y, "ParamY")] {
        builder.create_parameter(Parameter {
            id: id.into(),
            runtime_id: runtime.into(),
            name: runtime.into(),
            minimum: -1.0,
            maximum: 1.0,
            default_value: 0.0,
            ..Default::default()
        })?;
    }
    let state = builder.freeze()?.state;
    let doc = import_physics3(&state, PHYSICS, &std::fs::read_to_string(path)?)?.candidate;
    let mut preview = PhysicsPreview::new(&doc);
    if stabilize {
        preview.set_parameter(X, 0.7)?;
        let settled = preview.stabilize()[Y];
        let mut frames = Vec::new();
        for index in 0..20 {
            let dt = if index % 2 == 0 {
                1.0_f32 / 60.0
            } else {
                1.0_f32 / 30.0
            };
            preview.set_parameter(X, if index % 3 == 0 { -0.4 } else { 0.7 })?;
            frames.push(preview.advance(dt)?[Y]);
        }
        println!(
            "{}",
            serde_json::json!({"stabilized": settled, "frames": frames})
        );
        return Ok(());
    }
    let mut frames = Vec::new();
    for index in 0..120 {
        let dt = if index % 11 == 0 {
            0.1_f32
        } else if index % 3 == 0 {
            1.0_f32 / 30.0
        } else {
            1.0_f32 / 60.0
        };
        let input = ((index as f32) * 0.17).sin();
        preview.set_parameter(X, input)?;
        let values = preview.advance(dt)?;
        frames.push(serde_json::json!({"dt": dt, "input": values[X], "output": values[Y]}));
    }
    println!("{}", serde_json::json!({"frames": frames}));
    Ok(())
}
