//! Native window playback: the application's display callbacks, GPU workers and compositor.
use super::*;

pub(super) async fn run(
    view: &WeakEntity<EditorView>,
    cx: &mut AsyncWindowContext,
    config: &Config,
) -> anyhow::Result<()> {
    let baseline = view.update_in(cx, |view, window, cx| -> anyhow::Result<_> {
        view.run_command(editor_core::CommandId::AnimationWorkspace, window, cx);
        let resource = view
            .editor
            .document()
            .unwrap()
            .animation_catalog()
            .motions
            .iter()
            .find(|m| m.playable() && m.looping)
            .cloned()
            .context("native benchmark needs a looping motion")?;
        view.editor
            .select_motion(view.editor.active_id().unwrap(), resource.selection)?;
        view.editor.set_animation_preview_mode(
            view.editor.active_id().unwrap(),
            editor_core::AnimationPreviewMode::Result,
        )?;
        view.redraw(cx);
        Ok((
            view.editor.document().unwrap().history_lengths(),
            view.editor.document().unwrap().modified(),
        ))
    })??;
    let executor = cx.background_executor().clone();
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        executor.timer(Duration::from_millis(10)).await;
        if view.update_in(cx, |v, _, _| {
            v.editor.animation().unwrap().presented.is_some()
        })? {
            break;
        }
        ensure!(
            Instant::now() < deadline,
            "animation baseline not presented"
        );
    }
    view.update_in(cx, |view, window, cx| {
        view.run_command(editor_core::CommandId::PlayPause, window, cx)
    })?;
    executor.timer(Duration::from_millis(500)).await;
    let marker = frame_trace::next_id();
    frame_trace::emit(marker, "animation_workload", &[("model", 0), ("phase", 1)]);
    let deadline =
        Instant::now() + Duration::from_secs_f64(config.samples as f64 / config.hz as f64);
    while Instant::now() < deadline {
        executor.timer(Duration::from_millis(100)).await;
        view.update_in(cx, |view, window, _| -> anyhow::Result<()> {
            ensure!(
                window.is_visible() && window.is_window_active(),
                "animation benchmark window lost visibility or focus"
            );
            let a = view.editor.animation().unwrap();
            ensure!(
                a.playing && a.error.is_none(),
                "animation stopped: {:?}",
                a.error
            );
            Ok(())
        })??;
    }
    frame_trace::emit(
        frame_trace::next_id(),
        "animation_workload",
        &[("model", 0), ("phase", 0)],
    );
    view.update_in(cx, |view, window, cx| -> anyhow::Result<()> {
        view.run_command(editor_core::CommandId::PlayPause, window, cx);
        ensure!(
            (
                view.editor.document().unwrap().history_lengths(),
                view.editor.document().unwrap().modified()
            ) == baseline,
            "playback changed authoring state"
        );
        Ok(())
    })??;
    executor.timer(Duration::from_millis(200)).await;
    Ok(())
}
