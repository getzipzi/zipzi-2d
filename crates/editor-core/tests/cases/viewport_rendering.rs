use super::support;
use editor_core::*;
use std::sync::Arc;

fn pair(editor: &mut Editor, kind: ViewportKind) -> [ViewportBinding; 2] {
    let ids = [ViewportId::new(), ViewportId::new()];
    for (index, id) in ids.into_iter().enumerate() {
        editor.register_viewport(id, kind).unwrap();
        editor.set_viewport_metrics(
            id,
            ViewportMetrics {
                width: 512. + index as f32 * 100.,
                height: 512.,
                scale_factor: 2.,
            },
        );
    }
    ids.map(|id| editor.viewport_binding(id).unwrap())
}

#[test]
fn cancel_command_only_releases_its_target_viewport() -> anyhow::Result<()> {
    let (document, _assets) = support::sample_document()?;
    let mut editor = Editor::from_document(document);
    let [a, b] = pair(&mut editor, ViewportKind::Modeling);
    let frame = editor.preview_for(a)?.canvas.unwrap();
    assert!(editor.present_frame(frame));
    let original = editor.document().unwrap().selected_vertex_positions();
    let point = editor.overlay_for_viewport(a).vertices[0].position;
    let token = editor.begin_viewport_interaction(a, ViewportInteractionKind::Modeling)?;
    editor.canvas_interaction_input(
        token,
        CanvasInput::Down {
            point,
            shift: false,
        },
    )?;
    editor.canvas_interaction_input(
        token,
        CanvasInput::Move {
            point: ViewportPoint {
                x: point.x + 20.,
                y: point.y,
            },
            left_down: true,
        },
    )?;
    let candidate = editor.document().unwrap().selected_vertex_positions();
    assert_ne!(candidate, original);
    editor.dispatch(editor.command_for_viewport(
        CommandId::CancelGesture,
        b,
        CommandSource::Shortcut,
    ))?;
    assert!(editor.viewport_interaction_valid(token));
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        candidate
    );
    editor.dispatch(editor.command_for_viewport(
        CommandId::CancelGesture,
        a,
        CommandSource::Shortcut,
    ))?;
    assert!(!editor.viewport_interaction_active());
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        original
    );

    // Core dispatch must also release navigation without depending on UI cleanup.
    let pan = editor.begin_viewport_interaction(a, ViewportInteractionKind::Navigation)?;
    editor.dispatch(editor.command_for_viewport(
        CommandId::CancelGesture,
        b,
        CommandSource::Direct,
    ))?;
    assert!(editor.viewport_interaction_valid(pan));
    editor.dispatch(editor.command_for_viewport(
        CommandId::CancelGesture,
        a,
        CommandSource::Direct,
    ))?;
    assert!(!editor.viewport_interaction_active());
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn modeling_instances_share_content_but_keep_requests_images_and_hit_geometry_independent()
-> anyhow::Result<()> {
    let (document, _assets) = support::sample_document()?;
    let mut editor = Editor::from_document(document);
    let [a, b] = pair(&mut editor, ViewportKind::Modeling);
    let first = editor.preview_for(a)?.canvas.unwrap();
    let second = editor.preview_for(b)?.canvas.unwrap();
    assert_eq!(first.version.request, second.version.request);
    assert!(Arc::ptr_eq(&first.geometry, &second.geometry));
    assert_ne!(first.transform, second.transform);
    assert_eq!(editor.evaluation_count(), 1);
    assert!(editor.present_frame(second.clone()));
    assert!(editor.present_frame(first.clone()));
    editor.pan_viewport(a, ViewportPoint { x: 20., y: 10. })?;
    let moved = editor.preview_for_if_needed(a)?.unwrap().canvas.unwrap();
    assert!(editor.preview_for_if_needed(b)?.is_none());
    assert!(Arc::ptr_eq(&first.geometry, &moved.geometry));
    assert!(editor.present_frame(moved.clone()));
    assert!(editor.has_presented_frame(Some(second.version)));
    assert!(!editor.has_presented_frame(Some(first.version)));
    assert!(editor.has_presented_frame(Some(moved.version)));
    let original = editor.document().unwrap().selected_vertex_positions();
    let point = editor.overlay_for_viewport(b).vertices[0].position;
    let token = editor.begin_viewport_interaction(b, ViewportInteractionKind::Modeling)?;
    editor.canvas_interaction_input(
        token,
        CanvasInput::Down {
            point,
            shift: false,
        },
    )?;
    editor.canvas_interaction_input(
        token,
        CanvasInput::Move {
            point: ViewportPoint {
                x: point.x + 20.,
                y: point.y,
            },
            left_down: true,
        },
    )?;
    assert_ne!(
        editor.document().unwrap().selected_vertex_positions(),
        original
    );
    assert!(!editor.canvas_input_for(
        a,
        CanvasInput::Move {
            point,
            left_down: false
        }
    )?);
    assert!(editor.viewport_interaction_valid(token));
    let candidate_a = editor.preview_for(a)?.canvas.unwrap();
    let candidate_b = editor.preview_for(b)?.canvas.unwrap();
    assert!(editor.present_frame(candidate_a.clone()));
    assert!(editor.present_frame(candidate_b.clone()));
    editor.set_viewport_visible(a.viewport, false);
    assert!(editor.viewport_interaction_valid(token));
    assert!(editor.has_presented_frame(Some(candidate_b.version)));
    assert!(!editor.present_frame(candidate_a));
    editor.cancel_viewport_interaction(token);
    assert_eq!(
        editor.document().unwrap().selected_vertex_positions(),
        original
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn animation_instances_reuse_one_content_evaluation_and_accept_independent_intermediate_views()
-> anyhow::Result<()> {
    let (document, _assets) = support::animation_document()?;
    let mut editor = Editor::from_document(document);
    editor.set_workspace(WorkspaceKind::Animation)?;
    let [a, b] = pair(&mut editor, ViewportKind::Animation);
    let motion = editor.document().unwrap().motion_resources()[0]
        .selection
        .clone();
    editor.select_motion(a.document, motion)?;
    let mut evaluator = AnimationEvaluator::default();
    let content = evaluator.evaluate(&editor.animation_request().unwrap(), |_, _| true)?;
    let canvas = content.scene.viewport_frame.model_space;
    let (first, _) = editor
        .animation_viewport_request(a, content.identity, canvas)
        .unwrap();
    let (second, _) = editor
        .animation_viewport_request(b, content.identity, canvas)
        .unwrap();
    assert_eq!(first.content, second.content);
    assert_ne!(first.metrics, second.metrics);
    editor.pan_viewport(a, ViewportPoint { x: 10., y: -5. })?;
    assert!(editor.animation_request().is_none());
    let (newer, _) = editor
        .animation_viewport_request(a, content.identity, canvas)
        .unwrap();
    assert!(
        editor
            .animation_viewport_request(b, content.identity, canvas)
            .is_none()
    );
    assert!(editor.present_animation_viewport(first, content.snapshot.clone()));
    assert!(editor.present_animation_viewport(newer, content.snapshot.clone()));
    // Another viewport may complete this same content after the first has advanced.
    assert!(editor.present_animation_viewport(second, content.snapshot.clone()));
    assert!(editor.can_display_animation_viewport(second));
    editor.set_viewport_metrics(
        a.viewport,
        ViewportMetrics {
            width: 900.,
            height: 400.,
            scale_factor: 1.,
        },
    );
    assert!(editor.can_display_animation_viewport(second));
    assert!(editor.animation_request().is_none());
    let (resized, _) = editor
        .animation_viewport_request(a, content.identity, canvas)
        .unwrap();
    editor.set_viewport_visible(a.viewport, false);
    editor.set_viewport_visible(a.viewport, true);
    assert!(!editor.present_animation_viewport(resized, content.snapshot.clone()));
    assert!(editor.can_display_animation_viewport(second));
    let rebound = editor.viewport_binding(a.viewport).unwrap();
    let (replacement, _) = editor
        .animation_viewport_request(rebound, content.identity, canvas)
        .unwrap();
    assert!(editor.present_animation_viewport(replacement, content.snapshot));
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn hover_and_marquee_overlays_belong_to_their_instance_and_binding() -> anyhow::Result<()> {
    let (document, _assets) = support::sample_document()?;
    let mut editor = Editor::from_document(document);
    let [a, b] = pair(&mut editor, ViewportKind::Modeling);
    for binding in [a, b] {
        let frame = editor.preview_for(binding)?.canvas.unwrap();
        assert!(editor.present_frame(frame));
    }
    let point = editor.overlay_for_viewport(b).vertices[0].position;
    editor.canvas_input_for(
        b,
        CanvasInput::Move {
            point,
            left_down: false,
        },
    )?;
    assert!(
        editor
            .overlay_for_viewport(b)
            .vertices
            .iter()
            .any(|vertex| vertex.hovered)
    );
    assert!(
        !editor
            .overlay_for_viewport(a)
            .vertices
            .iter()
            .any(|vertex| vertex.hovered)
    );
    editor.canvas_input_for(a, CanvasInput::Leave)?;
    assert!(
        editor
            .overlay_for_viewport(b)
            .vertices
            .iter()
            .any(|vertex| vertex.hovered)
    );
    editor.set_viewport_visible(b.viewport, false);
    editor.set_viewport_visible(b.viewport, true);
    let rebound = editor.viewport_binding(b.viewport).unwrap();
    let frame = editor.preview_for(rebound)?.canvas.unwrap();
    assert!(editor.present_frame(frame));
    assert!(
        !editor
            .overlay_for_viewport(rebound)
            .vertices
            .iter()
            .any(|vertex| vertex.hovered)
    );
    let token = editor.begin_viewport_interaction(a, ViewportInteractionKind::Modeling)?;
    editor.canvas_interaction_input(
        token,
        CanvasInput::Down {
            point: ViewportPoint { x: 5., y: 5. },
            shift: false,
        },
    )?;
    editor.canvas_interaction_input(
        token,
        CanvasInput::Move {
            point: ViewportPoint { x: 60., y: 50. },
            left_down: true,
        },
    )?;
    assert!(editor.overlay_for_viewport(a).marquee.is_some());
    assert!(editor.overlay_for_viewport(rebound).marquee.is_none());
    editor.cancel_viewport_interaction(token);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn hiding_or_destroying_a_domain_input_owner_cancels_its_candidate_without_a_ui_token()
-> anyhow::Result<()> {
    for destroy in [false, true] {
        let (document, _assets) = support::sample_document()?;
        let mut editor = Editor::from_document(document);
        let [a, b] = pair(&mut editor, ViewportKind::Modeling);
        let frame = editor.preview_for(a)?.canvas.unwrap();
        assert!(editor.present_frame(frame));
        let original = editor.document().unwrap().selected_vertex_positions();
        let point = editor.overlay_for_viewport(a).vertices[0].position;
        editor.canvas_input_for(
            a,
            CanvasInput::Down {
                point,
                shift: false,
            },
        )?;
        editor.canvas_input_for(
            a,
            CanvasInput::Move {
                point: ViewportPoint {
                    x: point.x + 30.,
                    y: point.y,
                },
                left_down: true,
            },
        )?;
        assert!(editor.gesture_active());
        if destroy {
            editor.unregister_viewport(a.viewport);
        } else {
            editor.set_viewport_visible(a.viewport, false);
        }
        assert!(!editor.gesture_active());
        assert_eq!(
            editor.document().unwrap().selected_vertex_positions(),
            original
        );
        assert!(editor.viewport_binding_valid(b));
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    }
    Ok(())
}
