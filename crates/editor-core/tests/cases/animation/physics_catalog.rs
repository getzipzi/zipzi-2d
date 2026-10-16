use super::super::support;
use super::common::*;
use editor_core::*;

#[test]
fn combined_physics_pose_expression_replay_matches_continuous_and_cancelled_seek()
-> anyhow::Result<()> {
    let (document, _assets) = support::combined_animation_document()?;
    let physics = document
        .parameters()
        .iter()
        .find(|p| p.name == "Physics output")
        .unwrap()
        .id
        .clone();
    let mut editor = crate::support::editor_from_document(document);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    editor.set_canvas_metrics(CanvasMetrics {
        width: 256.,
        height: 256.,
        scale_factor: 1.,
    });
    let id = editor.active_id().unwrap();
    select(&mut editor, 0)?;
    expression(&mut editor, 0)?;
    let mut evaluator = AnimationEvaluator::default();
    editor.toggle_animation()?;
    for time in [0.25, 0.5, 0.75, 1.] {
        editor.advance_animation_clock(time);
        if time == 0.5 {
            expression(&mut editor, 2)?;
        }
        let request = editor.animation_request().unwrap();
        let continuous = evaluator.evaluate(&request, |_, _| true)?;
        let cold = AnimationEvaluator::default().evaluate(&request, |_, _| true)?;
        assert_eq!(continuous.snapshot, cold.snapshot);
        assert_eq!(continuous.scene.runtime_frame, cold.scene.runtime_frame);
        assert!(!continuous.snapshot.part_opacities.is_empty());
        if time == 1. {
            assert_ne!(continuous.snapshot.parameters[&physics], 0.);
        }
    }
    for time in [0.1, 0.75, 0.503, 1.5] {
        editor.seek_animation(id, time)?;
        let request = editor.animation_request().unwrap();
        let cached = evaluator.evaluate(&request, |_, _| true)?;
        let cold = AnimationEvaluator::default().evaluate(&request, |_, _| true)?;
        assert_eq!(cached.snapshot, cold.snapshot);
        assert_eq!(cached.scene.runtime_frame, cold.scene.runtime_frame);
    }
    editor.seek_animation(id, 0.2)?;
    let mut cancelled = editor.animation_request().unwrap();
    cancelled.identity.time = 100.;
    assert!(evaluator.evaluate(&cancelled, |step, _| step < 20).is_err());
    editor.seek_animation(id, 0.3)?;
    let request = editor.animation_request().unwrap();
    assert_eq!(
        evaluator.evaluate(&request, |_, _| true)?.snapshot,
        AnimationEvaluator::default()
            .evaluate(&request, |_, _| true)?
            .snapshot
    );
    editor.clear_expression()?;
    let no_expression = evaluate(&mut editor, &mut evaluator)?;
    assert!(no_expression.snapshot.active_expressions.is_empty());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    assert!(!editor.document().unwrap().modified());
    Ok(())
}

#[test]
fn local_model3_live_expression_switch_reuses_physics_history() -> anyhow::Result<()> {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../models/local/mao/runtime/mao_pro.model3.json");
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
    editor.set_canvas_metrics(CanvasMetrics {
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
        .join("../../models/local/mao/runtime/mao_pro.model3.json");
    if !path.exists() {
        return Ok(());
    }
    let (document, _) = Document::import_model3(&path)?;
    let modified = document.modified();
    let history = document.history_lengths();
    let mut editor = crate::support::editor_from_document(document);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    editor.set_canvas_metrics(CanvasMetrics {
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

#[test]
fn catalog_preserves_registration_fades_and_reuses_only_the_current_model_revision()
-> anyhow::Result<()> {
    use std::sync::Arc;
    let (mut editor, _assets) = setup()?;
    let first = editor.document().unwrap().animation_catalog();
    assert_eq!(first.motions.len(), 3);
    assert_eq!(
        first.motions[0].selection.clip_id,
        first.motions[1].selection.clip_id
    );
    assert_ne!(first.motions[0].selection, first.motions[1].selection);
    assert_eq!(first.motions[0].fade_in, 0.);
    assert_eq!(first.motions[1].fade_in, 1.);
    assert_eq!(
        (
            first.motions[0].fps,
            first.motions[0].tracks,
            first.motions[0].events
        ),
        (60., 1, 0)
    );
    assert_eq!(
        (first.expressions[0].fade_in, first.expressions[0].fade_out),
        (0.2, 0.3)
    );
    assert_eq!(first.expressions[0].parameters, 1);
    assert_eq!(
        first.expressions[0].blends,
        vec![kasane_core::document::ExpressionBlend::Add]
    );
    assert_eq!(
        first.parameters.len(),
        editor.document().unwrap().parameters().len()
    );
    expression(&mut editor, 0)?;
    editor.toggle_animation()?;
    editor.advance_animation_clock(0.75);
    assert!(Arc::ptr_eq(
        &first,
        &editor.document().unwrap().animation_catalog()
    ));
    editor.set_workspace(WorkspaceKind::Modeling)?;
    editor.execute(EditorCommand::SetObjectProperty {
        object: ObjectRef::new(ObjectKind::Expression, first.expressions[0].id.clone()),
        field: "fade_in".into(),
        value: serde_json::json!(0.7),
    })?;
    let changed = editor.document().unwrap().animation_catalog();
    assert!(!Arc::ptr_eq(&first, &changed));
    assert_eq!(changed.expressions[0].fade_in, 0.7);
    editor.dispatch(editor.command(CommandId::Undo))?;
    let undone = editor.document().unwrap().animation_catalog();
    assert!(!Arc::ptr_eq(&changed, &undone));
    assert_eq!(undone.expressions[0].fade_in, 0.2);
    editor.dispatch(editor.command(CommandId::Redo))?;
    assert_eq!(
        editor.document().unwrap().animation_catalog().expressions[0].fade_in,
        0.7
    );
    Ok(())
}

#[test]
fn diagnostics_are_available_before_play_and_do_not_duplicate_sdk_coverage() -> anyhow::Result<()> {
    let (document, _assets) = support::diagnostic_animation_document()?;
    let mut editor = crate::support::editor_from_document(document);
    editor.set_workspace(WorkspaceKind::Animation)?;
    editor.set_animation_preview_mode(editor.active_id().unwrap(), AnimationPreviewMode::Result)?;
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let catalog = editor.document().unwrap().animation_catalog();
    assert_eq!(catalog.diagnostics.len(), 5); // 2 distinct registrations of Sound + 3 target issues.
    assert!(catalog.diagnostics.iter().any(
        |d| matches!(d, AnimationDiagnostic::SoundUnsupported { file, .. } if file == "test.wav")
    ));
    let invalid = &catalog.expressions[3];
    assert_eq!((invalid.fade_in, invalid.fade_out), (1., 1.));
    assert_eq!(
        invalid.blends,
        vec![kasane_core::document::ExpressionBlend::Add]
    );
    assert!(
        editor
            .select_expression(editor.active_id().unwrap(), invalid.id.clone())
            .is_err()
    );
    assert!(editor.animation().unwrap().expression.is_none());
    editor.select_motion(
        editor.active_id().unwrap(),
        catalog.motions[3].selection.clone(),
    )?;
    let mut evaluator = AnimationEvaluator::default();
    let evaluated = evaluate(&mut editor, &mut evaluator)?;
    assert_eq!(evaluated.snapshot.coverage.len(), 2);
    assert_eq!(
        catalog.diagnostics_for(Some(&evaluated.snapshot)),
        catalog.diagnostics
    );
    let mut snapshot = (*evaluated.snapshot).clone();
    snapshot.coverage.extend([
        "Physics: step limit reached".into(),
        "Future coverage".into(),
        "Future coverage".into(),
    ]);
    let diagnostics = catalog.diagnostics_for(Some(&snapshot));
    assert_eq!(diagnostics.len(), 7);
    assert!(diagnostics.contains(&AnimationDiagnostic::PhysicsStepLimit));
    assert!(diagnostics.contains(&AnimationDiagnostic::RuntimeCoverage {
        detail: "Future coverage".into()
    }));
    let index = editor.document().unwrap().object_index();
    for object in &index.objects {
        if object.reference.kind == ObjectKind::Motion {
            assert!(object.properties.get("state").is_none());
        }
    }
    for relation in &index.relations {
        assert!(!relation.detail.contains("not playing"));
        assert!(!relation.detail.contains("not applied"));
    }
    assert!(!editor.document().unwrap().modified());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn runtime_failures_remain_typed_for_localized_display() {
    let error: anyhow::Error = kasane_sdk::AnimationError::SeekLimit.into();
    assert_eq!(
        AnimationFailure::from_error(&error),
        AnimationFailure::Runtime(kasane_sdk::AnimationError::SeekLimit)
    );
    assert_eq!(
        AnimationFailure::from_error(&anyhow::anyhow!("GPU unavailable")),
        AnimationFailure::Other("GPU unavailable".into())
    );
}
