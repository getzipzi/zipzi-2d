//! Measure dirty-state queries only; import, save, edits and IO are untimed.
use std::{hint::black_box, path::PathBuf, time::Instant};
use zipzi_editor_core::{CommandId, Document, Editor};

fn measure(label: &str, samples: usize, expected: bool, mut query: impl FnMut() -> bool) {
    for _ in 0..100 {
        assert_eq!(black_box(query()), expected);
    }
    // Printing is outside the measured loop to avoid perturbing each sample.
    let mut times = Vec::with_capacity(samples);
    for _ in 0..samples {
        let start = Instant::now();
        let result = black_box(query());
        let elapsed = start.elapsed().as_nanos();
        assert_eq!(result, expected);
        times.push(elapsed);
    }
    for (sample, nanoseconds) in times.into_iter().enumerate() {
        println!("{label},{sample},{nanoseconds},{expected}");
    }
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path = PathBuf::from(args.next().expect("model3 path")).canonicalize()?;
    let samples: usize = args.next().unwrap_or_else(|| "2000".into()).parse()?;
    anyhow::ensure!(samples > 0, "samples must be positive");
    let (mut document, _) = Document::import_model3(&path)?;
    let layers = document.layers();
    eprintln!(
        "debug_assertions={}, meshes={}, vertices={}, triangles={}, samples={}",
        cfg!(debug_assertions),
        layers.len(),
        layers.iter().map(|l| l.vertices).sum::<usize>(),
        layers.iter().map(|l| l.triangles).sum::<usize>(),
        samples
    );
    println!("case,sample,nanoseconds,modified");
    measure("imported_unsaved", samples, true, || {
        black_box(&document).modified()
    });

    // Use a real saved project so the expensive saved_content branch runs.
    // The original model/resources are never modified.
    let temporary = tempfile::tempdir()?;
    let receipt = document.save(&temporary.path().join("model"))?;
    eprintln!(
        "saved_manifest_bytes={}",
        std::fs::metadata(&receipt.manifest)?.len()
    );
    measure("saved_clean", samples, false, || {
        black_box(&document).modified()
    });
    let mut document = Document::open(&receipt.manifest)?;
    measure("reopened_clean", samples, false, || {
        black_box(&document).modified()
    });

    let original_name = document.selected_layer().unwrap().name;
    document.preview_name(&original_name)?;
    measure("pending_unchanged", samples, false, || {
        black_box(&document).modified()
    });
    document.preview_name(&format!("{original_name} benchmark"))?;
    measure("pending_changed", samples, true, || {
        black_box(&document).modified()
    });
    document.finish_edit()?;
    measure("committed_changed", samples, true, || {
        black_box(&document).modified()
    });
    document.undo()?;
    measure("undo_to_saved", samples, false, || {
        black_box(&document).modified()
    });

    // Root rendering asks for tab summaries and both navigation command states.
    // Keep this combination to measure redundant summary construction too.
    let editor = Editor::from_document(document);
    measure("tab_and_navigation_queries", samples, false, || {
        let editor = black_box(&editor);
        let documents = black_box(editor.documents());
        black_box(editor.command_available(editor.command(CommandId::Previous)));
        black_box(editor.command_available(editor.command(CommandId::Next)));
        documents[0].modified
    });
    Ok(())
}
