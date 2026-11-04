use std::time::Instant;
use zipzi_editor_core::*;

#[test]
fn local_project_tree_state_timing() -> anyhow::Result<()> {
    let Some(path) = std::env::var_os("ZIPZI_TREE_STATE_PROJECT") else {
        eprintln!("SKIP: set ZIPZI_TREE_STATE_PROJECT to inspect a local project");
        return Ok(());
    };
    let document = Document::open(std::path::Path::new(&path))?;
    let mut editor = super::support::editor_from_document(document);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 1200.,
        height: 900.,
        scale_factor: 2.,
    });
    super::support::present_canvas(&mut editor)?;
    let objects: Vec<_> = editor
        .document()
        .unwrap()
        .object_index()
        .objects
        .iter()
        .filter(|o| {
            matches!(
                o.reference.kind,
                ObjectKind::Mesh | ObjectKind::Part | ObjectKind::Deformer
            )
        })
        .map(|o| o.reference.clone())
        .collect();
    for kind in [ObjectKind::Mesh, ObjectKind::Part, ObjectKind::Deformer] {
        let object = objects.iter().find(|o| o.kind == kind).unwrap().clone();
        let original = editor.document().unwrap().object_state(&object).explicit;
        for lock in [true, false] {
            for active in [true, false] {
                #[cfg(feature = "bench-instrumentation")]
                let capture = zipzi_core::benchmark::Capture::start();
                let start = Instant::now();
                editor.execute(if lock {
                    EditorCommand::SetObjectsLocked {
                        objects: vec![object.clone()],
                        locked: if active {
                            !original.locked
                        } else {
                            original.locked
                        },
                    }
                } else {
                    EditorCommand::SetObjectVisible {
                        object: object.clone(),
                        visible: if active {
                            original.hidden_in_editor
                        } else {
                            !original.hidden_in_editor
                        },
                    }
                })?;
                let command = start.elapsed();
                #[cfg(feature = "bench-instrumentation")]
                for (stage, m) in capture.finish() {
                    eprintln!(
                        "{stage}: {:.3}ms self {:.3}ms",
                        m.inclusive_ns as f64 / 1e6,
                        m.self_ns as f64 / 1e6
                    );
                }
                let start = Instant::now();
                editor.document().unwrap().object_index();
                editor.document().unwrap().object_state_index();
                editor.part_tree();
                editor.deformer_tree();
                let trees = start.elapsed();
                let start = Instant::now();
                if let Some(preview) = editor.preview_if_needed()? {
                    if let Some(frame) = preview.canvas {
                        editor.present_frame(frame);
                    }
                }
                eprintln!(
                    "{kind:?} lock={lock} active={active}: command={command:?} trees={trees:?} preview={:?}",
                    start.elapsed()
                );
            }
        }
    }
    Ok(())
}
