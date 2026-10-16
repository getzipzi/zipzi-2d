use super::super::harness::{Harness, update};
use super::common::{drain, select};
use anyhow::{Context as _, ensure};
use editor_core::{AnimationEvaluator, AnimationPreviewMode, Document};

pub(crate) fn result_parity(h: &mut Harness) -> anyhow::Result<()> {
    select(h, "TimelineNatural")?;
    update(h, |view, _, cx| -> anyhow::Result<()> {
        let doc = view.editor.active_id().unwrap();
        view.editor
            .set_animation_preview_mode(doc, AnimationPreviewMode::Result)?;
        let expression = view
            .editor
            .document()
            .unwrap()
            .animation_catalog()
            .expressions
            .first()
            .map(|e| e.id.clone());
        if let Some(expression) = expression {
            view.editor.select_expression(doc, expression)?;
        }
        view.redraw(cx);
        Ok(())
    })??;
    for time in [0.1137, 1.125, 9.23, 0.25] {
        let request = update(h, |view, _, cx| -> anyhow::Result<_> {
            view.editor
                .seek_timeline(view.editor.active_id().unwrap(), time)?;
            let request = view
                .editor
                .animation_request()
                .context("missing parity request")?;
            // The observer owns this request. Submit a fresh identity to the real worker too.
            view.editor.invalidate_animation();
            view.redraw(cx);
            Ok(request)
        })??;
        let mut sdk =
            kasane_sdk::MotionPreview::new(request.source.materialized_snapshot()?.document());
        let motion = request.motion.as_ref().unwrap();
        if let Some((group, index)) = &motion.entry {
            sdk.schedule_motion_entry(group, *index, 0.)?;
        } else {
            sdk.schedule_motion(&motion.clip_id, 0.)?;
        }
        sdk.replace_expression_schedule_with_transitions(
            request
                .expressions
                .iter()
                .map(|e| (e.expression_id.as_str(), e.time, e.transition)),
        )?;
        sdk.seek(request.identity.time)?;
        let mut evaluator = AnimationEvaluator::default();
        let evaluated = evaluator.evaluate(&request, |_, _| true)?;
        ensure!(
            *evaluated.snapshot == *sdk.snapshot(),
            "real result snapshot diverged from SDK"
        );
        ensure!(
            *evaluated.scene.viewport_frame == sdk.evaluate_drawables()?,
            "real result geometry diverged from SDK"
        );
        drain(h)?;
        ensure!(
            h.view.read_with(&h.cx, |v, _| v
                .editor
                .animation()
                .unwrap()
                .presented
                .as_deref()
                == Some(sdk.snapshot())),
            "presented result diverged from SDK"
        );
    }
    Ok(())
}

pub(crate) fn persistence(h: &mut Harness) -> anyhow::Result<()> {
    let original = h.view.read_with(&h.cx, |v, _| {
        v.editor
            .document()
            .unwrap()
            .animation_catalog()
            .motions
            .iter()
            .map(|m| {
                v.editor
                    .document()
                    .unwrap()
                    .motion_definition(&m.selection.clip_id)
                    .unwrap()
                    .clone()
            })
            .collect::<Vec<_>>()
    });
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("timeline.kasane");
    let save = update(h, |v, _, _| {
        v.editor
            .prepare_save(v.editor.active_id().unwrap(), &path, false)
    })??;
    let mut completion = save.run();
    update(h, |v, _, _| v.editor.finish_save(&mut completion))??;
    let reopened = Document::open(&path.join("project.kasane.json"))?;
    for c in original {
        ensure!(
            reopened.motion_definition(&c.id) == Some(&c),
            "save/reopen changed imported keys, UUIDs, tails or extensions"
        );
    }
    Ok(())
}
