use super::support;
use std::sync::Arc;
use zipzi_editor_core::*;

fn setup() -> anyhow::Result<(Editor, tempfile::TempDir, ViewportBinding, Vec<ObjectRef>)> {
    let (document, assets) = support::sample_document_with_two_layers()?;
    let mut editor = crate::support::editor_from_document(document);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let binding = editor.primary_viewport_binding().unwrap();
    let preview = editor.preview()?;
    let objects = preview
        .scene
        .viewport_frame
        .drawables
        .iter()
        .map(|mesh| ObjectRef::new(ObjectKind::Mesh, &mesh.id))
        .collect();
    Ok((editor, assets, binding, objects))
}

#[test]
fn isolation_freezes_scope_and_filters_picking_overlays_and_marquee() -> anyhow::Result<()> {
    let (mut editor, _assets, binding, objects) = setup()?;
    editor.execute(EditorCommand::SelectCanvasObjects(vec![objects[0].clone()]))?;
    let original = editor.preview()?;
    let history = editor.document().unwrap().history_lengths();
    let modified = editor.document().unwrap().modified();
    let view = editor.viewport_view(binding);
    editor.toggle_viewport_isolation(binding)?;
    assert!(editor.viewport_isolated(binding));
    assert!(!editor.present_frame(original.canvas.unwrap()));
    let dim = editor.preview()?;
    assert!(Arc::ptr_eq(
        &original.scene.runtime_frame,
        &dim.scene.runtime_frame
    ));
    assert_eq!(
        dim.scene.viewport_frame.drawables[0].opacity,
        original.scene.viewport_frame.drawables[0].opacity
    );
    assert_eq!(
        dim.scene.viewport_frame.drawables[1].opacity,
        original.scene.viewport_frame.drawables[1].opacity
    );
    assert_eq!(
        dim.scene.viewport_frame.offscreens.last().unwrap().opacity,
        0.15
    );
    assert_eq!(
        dim.scene.viewport_frame.drawables,
        original.scene.viewport_frame.drawables
    );
    let frame = dim.canvas.unwrap();
    assert!(frame.geometry.mesh_visible(&objects[0].id));
    assert!(!frame.geometry.mesh_visible(&objects[1].id));
    for point in [
        ModelPoint { x: 128., y: 128. },
        ModelPoint { x: 100., y: 100. },
    ] {
        assert!(
            frame
                .geometry
                .mesh_hits(point)
                .iter()
                .all(|id| *id == objects[0].id)
        );
    }
    assert!(editor.present_frame(frame.clone()));
    editor.execute(EditorCommand::SelectCanvasObjects(vec![objects[1].clone()]))?;
    assert!(editor.viewport_isolated(binding));
    assert!(editor.overlay_for_viewport(binding).vertices.is_empty());
    assert!(editor.overlay_for_viewport(binding).accent_lines.is_empty());
    editor.execute(EditorCommand::SelectCanvasObjects(Vec::new()))?;
    assert!(editor.can_toggle_viewport_isolation(binding));
    let start = ViewportPoint { x: 0., y: 0. };
    let end = ViewportPoint { x: 512., y: 512. };
    crate::support::pointer_input_for(
        &mut editor,
        binding,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
        },
    )?;
    crate::support::pointer_input_for(&mut editor, binding, CanvasInput::Up { point: end })?;
    assert_eq!(
        editor.document().unwrap().context().selections,
        vec![objects[0].clone()]
    );
    assert_eq!(editor.viewport_view(binding), view);
    assert_eq!(editor.document().unwrap().history_lengths(), history);
    assert_eq!(editor.document().unwrap().modified(), modified);
    editor.toggle_viewport_isolation(binding)?;
    let restored = editor.preview()?;
    assert_eq!(restored.scene.viewport_frame, original.scene.viewport_frame);
    assert!(
        restored
            .canvas
            .unwrap()
            .geometry
            .mesh_visible(&objects[1].id)
    );
    Ok(())
}

#[test]
fn isolation_display_is_per_viewport_and_cached_without_re_evaluation() -> anyhow::Result<()> {
    let (mut editor, _assets, binding, objects) = setup()?;
    let other = ViewportId::new();
    editor.register_viewport(other, ViewportKind::Modeling)?;
    editor.set_viewport_metrics(other, editor.canvas_metrics());
    let other = editor.viewport_binding(other).unwrap();
    editor.execute(EditorCommand::SelectCanvasObjects(vec![objects[0].clone()]))?;
    editor.toggle_viewport_isolation(binding)?;
    let dim = editor.preview_for(binding)?;
    let evaluations = editor.evaluation_count();
    editor.pan_viewport(binding, ViewportPoint { x: 10., y: 0. })?;
    let pan = editor.preview_for(binding)?;
    assert!(Arc::ptr_eq(
        &dim.scene.viewport_frame,
        &pan.scene.viewport_frame
    ));
    assert!(Arc::ptr_eq(
        &dim.canvas.unwrap().geometry,
        &pan.canvas.unwrap().geometry
    ));
    editor.set_viewport_isolation_display(binding, IsolationDisplay::Hide)?;
    let hidden = editor.preview_for_if_needed(binding)?.unwrap();
    assert!(!hidden.scene.viewport_frame.drawables[1].visible);
    assert_eq!(hidden.scene.viewport_frame.drawables[1].opacity, 1.);
    let full = editor.preview_for(other)?;
    assert!(!editor.viewport_isolated(other));
    assert!(full.scene.viewport_frame.drawables[1].visible);
    assert_eq!(full.scene.viewport_frame.drawables[1].opacity, 1.);
    assert_eq!(editor.evaluation_count(), evaluations);
    editor.set_viewport_visible(binding.viewport, false);
    editor.set_viewport_visible(binding.viewport, true);
    let rebound = editor.viewport_binding(binding.viewport).unwrap();
    assert!(editor.viewport_isolated(rebound));
    assert!(editor.toggle_viewport_isolation(binding).is_err());
    editor.toggle_viewport_isolation(rebound)?;
    editor.toggle_viewport_isolation(rebound)?;
    assert_eq!(
        editor.viewport_isolation_display(rebound),
        IsolationDisplay::Hide
    );
    editor.new_document()?;
    assert!(!editor.viewport_isolated(editor.viewport_binding(binding.viewport).unwrap()));
    Ok(())
}

#[test]
fn isolation_expands_parts_and_deformers_but_preserves_document_visibility() -> anyhow::Result<()> {
    let (document, _assets) = support::sample_document_with_scene_objects()?;
    let mut editor = crate::support::editor_from_document(document);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let binding = editor.primary_viewport_binding().unwrap();
    for target in [
        support::object(ObjectKind::Part, 101),
        support::object(ObjectKind::Deformer, 501),
        support::object(ObjectKind::Offscreen, 601),
    ] {
        editor.execute(EditorCommand::SelectCanvasObjects(vec![target.clone()]))?;
        editor.toggle_viewport_isolation(binding)?;
        let frame = editor.preview()?.canvas.unwrap();
        assert!(!frame.geometry.member_meshes(&target).is_empty());
        assert!(
            frame
                .geometry
                .controls
                .iter()
                .filter(|c| c.visible)
                .all(|c| c.object == target
                    || frame.geometry.ancestors(&c.object).contains(&target)
                    || target.kind == ObjectKind::Offscreen)
        );
        editor.toggle_viewport_isolation(binding)?;
    }
    let mesh = ObjectRef::new(ObjectKind::Mesh, editor.layers()[0].id.clone());
    editor.execute(EditorCommand::SetObjectVisible {
        object: mesh.clone(),
        visible: false,
    })?;
    editor.execute(EditorCommand::SelectCanvasObjects(vec![mesh]))?;
    editor.toggle_viewport_isolation(binding)?;
    let preview = editor.preview()?;
    assert!(!preview.scene.viewport_frame.drawables[0].visible);
    Ok(())
}

#[test]
fn isolation_allows_navigation_but_blocks_modeling_interactions() -> anyhow::Result<()> {
    for kind in [
        ViewportInteractionKind::Navigation,
        ViewportInteractionKind::Modeling,
    ] {
        let (mut editor, _assets, binding, objects) = setup()?;
        editor.execute(EditorCommand::SelectCanvasObjects(vec![objects[0].clone()]))?;
        let navigation = kind == ViewportInteractionKind::Navigation;
        let token = if navigation {
            editor.begin_viewport_navigation(binding)?
        } else {
            let frame = editor.preview_for(binding)?.canvas.unwrap();
            assert!(editor.present_frame(frame));
            editor
                .begin_canvas_gesture(binding, ViewportPoint { x: 5., y: 5. }, false, false)?
                .capture
                .unwrap()
        };
        assert_eq!(editor.can_toggle_viewport_isolation(binding), navigation);
        assert_eq!(
            editor.toggle_viewport_isolation(binding).is_ok(),
            navigation
        );
        assert_eq!(
            editor
                .set_viewport_isolation_display(binding, IsolationDisplay::Hide)
                .is_ok(),
            navigation
        );
        assert!(editor.viewport_interaction_valid(token));
        if navigation {
            let before = editor.viewport_view(binding).unwrap();
            editor.pan_viewport(binding, ViewportPoint { x: 12., y: -7. })?;
            let view = editor.viewport_view(binding).unwrap();
            assert_eq!(view.pan.x, before.pan.x + 12.);
            assert_eq!(view.pan.y, before.pan.y - 7.);
            editor.toggle_viewport_isolation(binding)?;
            assert!(!editor.viewport_isolated(binding));
            assert_eq!(editor.viewport_view(binding), Some(view));
            assert!(editor.viewport_interaction_valid(token));
        }
        editor.cancel_viewport_interaction(token);
        assert!(editor.can_toggle_viewport_isolation(binding));
        assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    }
    Ok(())
}
