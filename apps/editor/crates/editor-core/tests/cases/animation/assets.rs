use super::common::*;
use editor_core::*;

#[test]
fn local_model3_live_expression_switch_reuses_physics_history() -> anyhow::Result<()> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../models/local/mao/runtime/mao_pro.model3.json");
    if !path.exists() {
        eprintln!("Skipping live expression timing: local Mao model is unavailable");
        return Ok(());
    }
    let (document, _) = Document::import_model3(&path)?;
    let catalog = document.animation_catalog();
    let motion = catalog
        .motions
        .iter()
        .find(|motion| motion.looping && motion.playable())
        .unwrap();
    let mut editor = crate::support::editor_from_document(document);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    editor.set_canvas_metrics(ViewportMetrics {
        width: 640.,
        height: 480.,
        scale_factor: 1.,
    });
    editor.select_motion(editor.active_id().unwrap(), motion.selection.clone())?;
    editor.toggle_animation()?;
    let mut evaluator = AnimationEvaluator::default();
    editor.advance_animation_clock(1.);
    evaluate(&mut editor, &mut evaluator)?;
    editor.advance_animation_clock(60.5);
    evaluate(&mut editor, &mut evaluator)?;
    for expression in catalog
        .expressions
        .iter()
        .filter(|expression| expression.unresolved_targets.is_empty())
        .take(2)
    {
        editor.select_expression(editor.active_id().unwrap(), expression.id.clone())?;
        let request = editor.animation_request().unwrap();
        let mut steps = 0;
        let start = std::time::Instant::now();
        let frame = evaluator.evaluate(&request, |_, total| {
            steps = total;
            true
        })?;
        let incremental = start.elapsed();
        let start = std::time::Instant::now();
        let cold = AnimationEvaluator::default().evaluate(&request, |_, _| true)?;
        eprintln!(
            "Mao expression switch at {}s: {steps} steps, {incremental:?}; cold replay {:?}",
            request.identity.time,
            start.elapsed()
        );
        assert!(steps <= 60);
        assert_eq!(frame.snapshot, cold.snapshot);
        assert_eq!(frame.scene.runtime_frame, cold.scene.runtime_frame);
        editor.advance_animation_clock(editor.animation().unwrap().time + 0.5);
        evaluate(&mut editor, &mut evaluator)?;
    }
    Ok(())
}

#[test]
fn local_model3_motion_uses_imported_physics_and_default_runtime_baseline() -> anyhow::Result<()> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../models/local/mao/runtime/mao_pro.model3.json");
    if !path.exists() {
        return Ok(());
    }
    let (document, _) = Document::import_model3(&path)?;
    let modified = document.modified();
    let history = document.history_lengths();
    let mut editor = crate::support::editor_from_document(document);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    editor.set_canvas_metrics(ViewportMetrics {
        width: 640.,
        height: 480.,
        scale_factor: 1.,
    });
    let resources = editor.document().unwrap().motion_resources();
    assert!(!resources.is_empty());
    select(&mut editor, 0)?;
    editor.toggle_animation()?;
    let mut evaluator = AnimationEvaluator::default();
    editor.advance_animation_clock(0.5);
    let first = evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)?;
    editor.advance_animation_clock(1.);
    let next = editor.animation_request().unwrap();
    let continuous = evaluator.evaluate(&next, |_, _| true)?;
    let cold = AnimationEvaluator::default().evaluate(&next, |_, _| true)?;
    assert_eq!(continuous.snapshot, cold.snapshot);
    assert_eq!(continuous.scene.viewport_frame, cold.scene.viewport_frame);
    assert!(
        continuous
            .scene
            .viewport_frame
            .drawables
            .iter()
            .any(|d| d.visible)
    );
    assert_ne!(first.snapshot.parameters, continuous.snapshot.parameters);
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    assert_eq!(editor.document().unwrap().modified(), modified);
    Ok(())
}
