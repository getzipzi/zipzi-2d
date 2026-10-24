use super::super::harness::{Harness, update};
use super::common::{drain, select};
use anyhow::{Context as _, ensure};
use zipzi_editor_core::{AnimationEvaluator, AnimationPreviewMode, Document};

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
            zipzi_sdk::MotionPreview::new(request.source.materialized_snapshot()?.document());
        sdk.set_physics_enabled(!request.physics.bypass);
        for (id, value) in &request.physics.baseline {
            sdk.set_base_parameter(id, *value)?;
        }
        sdk.set_physics_stabilized_start(request.physics.stabilized);
        let motion = request.motion.as_ref().unwrap();
        if let Some(id) = &motion.entry {
            sdk.schedule_motion_registration(id, 0.)?;
        } else {
            sdk.schedule_motion(&motion.clip_id, 0.)?;
        }
        sdk.replace_expression_schedule_with_transitions(
            request
                .expressions
                .iter()
                .map(|e| (e.expression_id.as_str(), e.time, e.transition)),
        )?;
        for input in &request.physics.inputs {
            sdk.schedule_parameter_input(&input.parameter, input.time, input.value)?;
        }
        sdk.seek(request.identity.time)?;
        // Seek snapshots omit optional inspector observations. The editor
        // explicitly collects them, so compare the same complete snapshot.
        let mut expected = sdk.snapshot().clone();
        expected.physics = sdk.physics_snapshot();
        let mut evaluator = AnimationEvaluator::default();
        let evaluated = evaluator.evaluate(&request, |_, _| true)?;
        ensure!(
            *evaluated.snapshot == expected,
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
                == Some(&expected)),
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
                    .as_ref()
                    .clone()
            })
            .collect::<Vec<_>>()
    });
    let temp = tempfile::tempdir()?;
    let path = temp.path().join("timeline.zipzi");
    let save = update(h, |v, _, _| {
        v.editor
            .prepare_save(v.editor.active_id().unwrap(), &path, false)
    })??;
    let mut completion = save.run();
    update(h, |v, _, _| v.editor.finish_save(&mut completion))??;
    let reopened = Document::open(&path.join("project.zipzi.json"))?;
    for c in original {
        ensure!(
            reopened.motion_definition(&c.id).as_deref() == Some(&c),
            "save/reopen changed imported keys, UUIDs, tails or extensions"
        );
    }
    Ok(())
}
