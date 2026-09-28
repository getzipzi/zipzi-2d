//! CPU-only baseline; native display latency and GPU memory require manual measurement.
use editor_core::{Editor, LoadRequest};
use std::{path::PathBuf, time::Instant};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let first = PathBuf::from(args.next().expect("first model3 path")).canonicalize()?;
    let second = PathBuf::from(args.next().expect("second model3 path")).canonicalize()?;
    let samples: usize = args.next().unwrap_or_else(|| "5".into()).parse()?;
    println!("sample,stage,milliseconds,drawables,decoded_texture_bytes");
    for sample in 0..samples {
        let mut editor = Editor::empty();
        for (stage, path) in [("import_first", &first), ("import_second", &second)] {
            let start = Instant::now();
            let task = editor
                .prepare_load(LoadRequest::Model3(path.clone()))?
                .unwrap();
            editor.finish_load(task.run())?;
            println!(
                "{sample},{stage},{:.6},,",
                start.elapsed().as_secs_f64() * 1000.
            );
            let start = Instant::now();
            let preview = editor.preview()?;
            println!(
                "{sample},{stage}_snapshot,{:.6},{},{}",
                start.elapsed().as_secs_f64() * 1000.,
                preview.scene.frame.drawables.len(),
                preview
                    .scene
                    .textures
                    .iter()
                    .map(|t| t.rgba.len())
                    .sum::<usize>()
            );
        }
        for index in 0..10 {
            let start = Instant::now();
            editor.activate(editor.adjacent(true).unwrap())?;
            let preview = editor.preview()?;
            std::hint::black_box(preview);
            println!(
                "{sample},switch_and_snapshot_{index},{:.6},,",
                start.elapsed().as_secs_f64() * 1000.
            );
        }
    }
    Ok(())
}
