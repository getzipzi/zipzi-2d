//! Measure the synchronous CPU work performed for each parameter slider update.
//! Run with `cargo run --locked --release -p editor-core --example parameter_benchmark -- MODEL 384`.
//! This excludes GPUI layout, Metal rendering and display presentation.
use editor_core::{Editor, EditorCommand, LoadRequest};
use std::{path::PathBuf, time::Instant};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().expect("model3 path")).canonicalize()?;
    let samples: usize = args.next().unwrap_or_else(|| "100".into()).parse()?;
    let mut editor = Editor::empty();
    let task = editor.prepare_load(LoadRequest::Model3(path))?.unwrap();
    editor.finish_load(task.run())?;
    let parameters = editor.document().unwrap().parameters();
    let parameters: Vec<_> = parameters
        .into_iter()
        .filter(|p| p.minimum < p.maximum)
        .collect();
    assert!(!parameters.is_empty(), "model has no adjustable parameter");
    eprintln!("parameters={}, samples={samples}", parameters.len());
    editor.preview()?;
    println!("sample,parameter_id,edit_ms,scene_ms,drawables");
    for sample in 0..samples {
        let parameter = &parameters[sample % parameters.len()];
        let value = parameter.minimum
            + (parameter.maximum - parameter.minimum)
                * if sample / parameters.len() % 2 == 0 {
                    0.4
                } else {
                    0.6
                };
        let start = Instant::now();
        editor.execute(EditorCommand::SetPreviewParameter {
            parameter_id: parameter.id.clone(),
            value,
        })?;
        let edit_ms = start.elapsed().as_secs_f64() * 1000.;
        let start = Instant::now();
        let preview = editor.preview()?;
        let scene_ms = start.elapsed().as_secs_f64() * 1000.;
        println!(
            "{sample},{},{edit_ms:.3},{scene_ms:.3},{}",
            parameter.id,
            preview.scene.viewport_frame.drawables.len()
        );
    }
    Ok(())
}
