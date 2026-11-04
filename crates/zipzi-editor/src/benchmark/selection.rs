//! Measure the normal selection command and subsequent native window presentation.
//! Command injection excludes OS delivery and tree hit testing.
use super::*;
use zipzi_editor_core::{ObjectKind, ObjectRef};

pub(super) async fn run(
    view: &WeakEntity<EditorView>,
    cx: &mut AsyncWindowContext,
    config: &Config,
) -> anyhow::Result<()> {
    let (objects, history) = view.update_in(cx, |v, _, _| {
        let document = v.editor.document().unwrap();
        let mut objects = document
            .object_index()
            .objects
            .iter()
            .filter(|o| o.reference.kind == ObjectKind::Deformer)
            .map(|o| o.reference.clone())
            .collect::<Vec<ObjectRef>>();
        if let Ok(ids) = std::env::var("ZIPZI_BENCH_SELECTION_OBJECTS") {
            objects.retain(|object| ids.split(',').any(|id| id == object.id));
        }
        objects.truncate(2);
        (objects, document.history_lengths())
    })?;
    ensure!(
        objects.len() == 2,
        "selection benchmark needs two deformers"
    );
    eprintln!("Selection benchmark objects: {objects:?}");
    for sample in 0..config.samples + config.warmup {
        cx.background_executor()
            .timer(Duration::from_millis(150))
            .await;
        let object = &objects[sample % objects.len()];
        let input = frame_trace::InputScope::begin(310);
        frame_trace::emit(
            input.id(),
            "selection_tick",
            &[
                ("sample", sample as u64),
                ("warmup", u64::from(sample < config.warmup)),
            ],
        );
        let start = Instant::now();
        view.update_in(cx, |v, w, cx| -> anyhow::Result<()> {
            ensure!(
                w.is_visible() && w.is_window_active(),
                "selection window lost focus"
            );
            let result = v
                .editor
                .execute(EditorCommand::SelectObject(object.clone()));
            v.complete_edit(result, cx);
            Ok(())
        })??;
        frame_trace::emit(
            input.id(),
            "selection_dispatch_end",
            &[("duration_ns", start.elapsed().as_nanos() as u64)],
        );
        drop(input);
        cx.background_executor()
            .timer(Duration::from_millis(150))
            .await;
        view.update_in(cx, |v, _, cx| -> anyhow::Result<()> {
            ensure!(
                v.edit_error.is_none(),
                "selection error: {:?}",
                v.edit_error
            );
            let document = v.editor.document().unwrap();
            ensure!(
                document.context().selection.as_ref() == Some(object),
                "selection did not change"
            );
            ensure!(
                document.history_lengths() == history,
                "selection changed history"
            );
            ensure!(
                v.dock
                    .as_ref()
                    .unwrap()
                    .properties
                    .read(cx)
                    .current_target()
                    .is_some_and(|t| &t.object == object),
                "inspector did not follow selection"
            );
            Ok(())
        })??;
    }
    cx.background_executor()
        .timer(Duration::from_millis(300))
        .await;
    Ok(())
}
