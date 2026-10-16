use super::super::support::{self, present_canvas as shown};
use super::common::*;
use editor_core::*;
use kasane_core::{
    Vec2,
    evaluation::{OffscreenFrame, RenderCommand},
};
use std::sync::Arc;

#[test]
fn marquee_selected_vertices_across_meshes_move_in_one_transaction() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let selected = editor
        .layers()
        .iter()
        .map(|m| ObjectRef::new(ObjectKind::Mesh, &m.id))
        .collect();
    editor.execute(EditorCommand::SelectObjects(selected))?;
    editor.canvas_input(CanvasInput::Down {
        point: CanvasPoint { x: 1., y: 1. },
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up {
        point: CanvasPoint { x: 510., y: 510. },
    })?;
    assert_eq!(
        editor
            .overlay()
            .vertices
            .iter()
            .filter(|v| v.selected)
            .count(),
        8
    );
    let original = editor.preview()?.scene;
    let p = editor.overlay().vertices[0].position;
    editor.canvas_input(CanvasInput::Down {
        point: p,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up {
        point: CanvasPoint {
            x: p.x + 20.,
            y: p.y + 10.,
        },
    })?;
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    let moved = editor.preview()?.scene;
    for (before, after) in original
        .viewport_frame
        .drawables
        .iter()
        .zip(&moved.viewport_frame.drawables)
    {
        for (a, b) in before.positions.iter().zip(&after.positions) {
            assert!((b.x - a.x - 40.).abs() < 1e-4);
            assert!((b.y - a.y + 20.).abs() < 1e-4);
        }
    }
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(
        editor.preview()?.scene.viewport_frame.drawables,
        original.viewport_frame.drawables
    );
    Ok(())
}

#[test]
fn hidden_meshes_have_no_invisible_component_hit_targets() -> anyhow::Result<()> {
    let (mut editor, _) = setup()?;
    editor.execute(EditorCommand::SetObjectVisible {
        object: editor
            .document()
            .unwrap()
            .context()
            .selection
            .clone()
            .unwrap(),
        visible: false,
    })?;
    let frame = shown(&mut editor)?;
    assert!(editor.overlay().vertices.is_empty());
    let scene = &frame.geometry;
    assert!(!matches!(
        scene.hit(
            CanvasPoint { x: 0., y: 0. },
            frame.transform,
            &editor.document().unwrap().context().selections,
            ToolId::Select,
            DeformerVisibility::default()
        ),
        Some(HitTarget::Component(_))
    ));
    Ok(())
}

#[test]
fn vertex_marquee_can_start_inside_selected_meshes_without_changing_objects() -> anyhow::Result<()>
{
    let (mut editor, _dir) = setup()?;
    let objects: Vec<_> = editor
        .layers()
        .iter()
        .map(|m| ObjectRef::new(ObjectKind::Mesh, &m.id))
        .collect();
    editor.execute(EditorCommand::SelectObjects(objects.clone()))?;
    let frame = shown(&mut editor)?;
    let start = frame.transform.to_canvas(ModelPoint { x: 120., y: 120. });
    let end = frame.transform.to_canvas(ModelPoint { x: 240., y: 240. });
    editor.canvas_input(CanvasInput::Down {
        point: start,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Move {
        point: end,
        left_down: true,
    })?;
    assert!(
        editor.overlay().marquee.is_some(),
        "a mesh interior must allow component marquee"
    );
    editor.canvas_input(CanvasInput::Up { point: end })?;
    assert_eq!(editor.document().unwrap().context().selections, objects);
    assert_eq!(
        editor
            .overlay()
            .vertices
            .iter()
            .filter(|v| v.selected)
            .count(),
        4
    );
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}

#[test]
fn invisible_offscreen_members_have_no_handles_and_are_excluded_from_marquee() -> anyhow::Result<()>
{
    let (mut editor, _dir) = setup()?;
    let mut frame = editor.preview()?.canvas.unwrap();
    let mut source = frame.geometry.source.clone();
    let evaluated = Arc::make_mut(&mut source.viewport_frame);
    let hidden = evaluated.drawables[0].id.clone();
    let visible = evaluated.drawables[1].id.clone();
    evaluated.offscreens = vec![OffscreenFrame {
        id: "hidden-group".into(),
        enabled: true,
        opacity: 0.,
        ..Default::default()
    }];
    evaluated.render_plan = vec![
        RenderCommand::BeginOffscreen {
            offscreen_id: "hidden-group".into(),
        },
        RenderCommand::DrawMesh {
            mesh_id: hidden.clone(),
        },
        RenderCommand::EndOffscreen {
            offscreen_id: "hidden-group".into(),
        },
        RenderCommand::DrawMesh {
            mesh_id: visible.clone(),
        },
    ];
    frame.geometry = Arc::new(CanvasScene::new(editor.document().unwrap(), source));
    assert!(editor.present_frame(frame));
    editor.execute(EditorCommand::SelectObject(ObjectRef::new(
        ObjectKind::Mesh,
        hidden,
    )))?;
    assert!(
        editor.overlay().vertices.is_empty(),
        "zero-opacity groups must not leave ghost controls"
    );
    editor.execute(EditorCommand::SetTool(ToolId::Select))?;
    editor.canvas_input(CanvasInput::Down {
        point: CanvasPoint { x: 1., y: 1. },
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up {
        point: CanvasPoint { x: 510., y: 510. },
    })?;
    assert_eq!(
        editor.document().unwrap().context().selections,
        [ObjectRef::new(ObjectKind::Mesh, visible)]
    );
    Ok(())
}

#[test]
fn warp_controls_align_with_mesh_after_canvas_y_reversal() -> anyhow::Result<()> {
    for flag in [0, 1] {
        let mut canvas = kasane_core::Canvas::new(320., 280., Vec2::new(110., 150.), 100.);
        canvas.flag = flag;
        let (doc, _dir, warp, _, _) = rich_document_with_canvas(canvas)?;
        let mesh = doc.context().selection.clone().unwrap();
        let mut editor = Editor::from_document(doc);
        editor.set_canvas_metrics(CanvasMetrics {
            width: 700.,
            height: 510.,
            scale_factor: 2.,
        });
        editor.zoom_by(0.5);
        editor.pan_by(CanvasPoint { x: 17., y: -23. });
        let frame = shown(&mut editor)?;
        let control = frame
            .geometry
            .controls
            .iter()
            .find(|c| c.object.id == warp)
            .unwrap();
        assert!((control.points[0].x - 32.).abs() < 1e-3);
        let expected_y = if flag == 0 {
            2. * canvas.origin.y - 32.
        } else {
            32.
        };
        assert!((control.points[0].y - expected_y).abs() < 1e-3);
        // Vertices occupy known local (u,v) positions inside the root warp.
        for ((_, p), (u, v)) in frame.geometry.mesh(&mesh.id).unwrap().vertices.iter().zip([
            (0.1, 0.1),
            (0.9, 0.1),
            (0.9, 0.9),
            (0.1, 0.9),
        ]) {
            let weights = [(1. - u) * (1. - v), u * (1. - v), (1. - u) * v, u * v];
            let expected =
                control
                    .points
                    .iter()
                    .zip(weights)
                    .fold(ModelPoint::default(), |p, (c, w)| ModelPoint {
                        x: p.x + c.x * w,
                        y: p.y + c.y * w,
                    });
            let a = frame.transform.to_canvas(*p);
            let b = frame.transform.to_canvas(expected);
            assert!(
                (a.x - b.x).hypot(a.y - b.y) < 0.01,
                "canvas flag {flag}: {a:?} != {b:?}"
            );
        }
        editor.execute(EditorCommand::SelectObject(control.object.clone()))?;
        editor.execute(EditorCommand::SetTool(ToolId::Select))?;
        let overlay = editor.overlay();
        for (vertex, expected) in overlay.vertices.iter().zip(&control.points) {
            let expected = frame.transform.to_canvas(*expected);
            assert!((vertex.position.x - expected.x).hypot(vertex.position.y - expected.y) < 0.001);
        }
    }
    Ok(())
}

#[test]
fn reversed_canvas_control_drags_follow_the_displayed_pointer() -> anyhow::Result<()> {
    let mut canvas = kasane_core::Canvas::new(320., 280., Vec2::new(110., 150.), 100.);
    canvas.flag = 0;
    let (doc, _dir, warp, rotation, _) = rich_document_with_canvas(canvas)?;
    let mesh = doc.context().selection.clone().unwrap();
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 640.,
        height: 560.,
        scale_factor: 2.,
    });
    for object in [
        mesh,
        ObjectRef::new(ObjectKind::Deformer, &warp),
        ObjectRef::new(ObjectKind::Deformer, &rotation),
    ] {
        editor.execute(EditorCommand::SelectObject(object.clone()))?;
        let position = |f: &CanvasFrame| {
            if object.kind == ObjectKind::Mesh {
                f.transform
                    .to_canvas(f.geometry.mesh(&object.id).unwrap().vertices[0].1)
            } else {
                let c = f
                    .geometry
                    .controls
                    .iter()
                    .find(|c| c.object == object)
                    .unwrap();
                f.geometry.control_points(c, f.transform)[0]
            }
        };
        let frame = shown(&mut editor)?;
        let start = position(&frame);
        let end = CanvasPoint {
            x: start.x + 8.,
            y: start.y + 12.,
        };
        editor.canvas_input(CanvasInput::Down {
            point: start,
            shift: false,
        })?;
        editor.canvas_input(CanvasInput::Up { point: end })?;
        let after = position(&shown(&mut editor)?);
        assert!(
            (after.x - end.x).hypot(after.y - end.y) < 0.01,
            "{}: {after:?} != {end:?}",
            object.id
        );
        assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
        editor.execute(EditorCommand::Undo)?;
        let restored = position(&shown(&mut editor)?);
        assert!((restored.x - start.x).hypot(restored.y - start.y) < 0.01);
    }
    // The visible rotation axis must follow the cursor in the same final space.
    let frame = shown(&mut editor)?;
    let c = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object.id == rotation)
        .unwrap();
    let points = frame.geometry.control_points(c, frame.transform);
    let end = CanvasPoint {
        x: points[0].x,
        y: points[0].y + (points[1].x - points[0].x).hypot(points[1].y - points[0].y),
    };
    editor.canvas_input(CanvasInput::Down {
        point: points[1],
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up { point: end })?;
    let frame = shown(&mut editor)?;
    let c = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object.id == rotation)
        .unwrap();
    let after = frame.geometry.control_points(c, frame.transform)[1];
    assert!(
        (after.x - end.x).hypot(after.y - end.y) < 0.01,
        "{after:?} != {end:?}"
    );
    Ok(())
}

#[test]
fn pose_updates_share_topology_but_publish_independent_positions() -> anyhow::Result<()> {
    let (doc, _dir) = support::sample_document_with_binding(true)?;
    let parameter_id = doc.parameters()[0].id.clone();
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let original = shown(&mut editor)?;
    let positions = original.geometry.meshes[0].vertices.clone();
    let point = editor.overlay().vertices[0].position;
    editor.canvas_input(CanvasInput::Down {
        point,
        shift: false,
    })?;
    editor.canvas_input(CanvasInput::Up {
        point: CanvasPoint {
            x: point.x + 20.,
            y: point.y,
        },
    })?;
    let edited = shown(&mut editor)?;
    assert_ne!(edited.geometry.meshes[0].vertices, positions);
    assert_eq!(
        original.geometry.meshes[0].vertices, positions,
        "old displayed frames are immutable"
    );
    editor.execute(EditorCommand::SetPreviewParameter {
        parameter_id,
        value: 1.,
    })?;
    let other_key = shown(&mut editor)?;
    assert_eq!(other_key.geometry.meshes[0].vertices, positions);
    for frame in [&edited, &other_key] {
        assert!(Arc::ptr_eq(&original.geometry.index, &frame.geometry.index));
        assert!(Arc::ptr_eq(
            &original.geometry.meshes[0].triangles,
            &frame.geometry.meshes[0].triangles
        ));
        assert!(Arc::ptr_eq(
            &original.geometry.meshes[0].edges,
            &frame.geometry.meshes[0].edges
        ));
        assert!(!Arc::ptr_eq(
            &original.geometry.meshes,
            &frame.geometry.meshes
        ));
    }
    Ok(())
}

#[test]
fn mesh_drag_reuses_unchanged_meshes_without_changing_old_frames() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let original = shown(&mut editor)?;
    let selected = editor
        .document()
        .unwrap()
        .context()
        .selection
        .clone()
        .unwrap();
    let unchanged = original
        .geometry
        .meshes
        .iter()
        .find(|mesh| mesh.id != selected.id)
        .unwrap();
    let positions = original
        .geometry
        .mesh(&selected.id)
        .unwrap()
        .vertices
        .clone();
    let point = editor.overlay().vertices[0].position;
    editor.canvas_input(CanvasInput::Down {
        point,
        shift: false,
    })?;
    for dx in [20., 40.] {
        editor.canvas_input(CanvasInput::Move {
            point: CanvasPoint {
                x: point.x + dx,
                y: point.y,
            },
            left_down: true,
        })?;
        let edited = shown(&mut editor)?;
        let same = edited
            .geometry
            .meshes
            .iter()
            .find(|mesh| mesh.id == unchanged.id)
            .unwrap();
        assert!(Arc::ptr_eq(unchanged, same));
        assert_ne!(
            edited.geometry.mesh(&selected.id).unwrap().vertices,
            positions
        );
        assert_eq!(
            original.geometry.mesh(&selected.id).unwrap().vertices,
            positions
        );
        let current = &edited.geometry.mesh(&selected.id).unwrap().vertices[0];
        assert_eq!(
            edited.geometry.hit(
                edited.transform.to_canvas(current.1),
                edited.transform,
                std::slice::from_ref(&selected),
                ToolId::Select,
                DeformerVisibility::default(),
            ),
            Some(HitTarget::Component(ComponentRef {
                object: selected.clone(),
                id: current.0
            })),
        );
    }
    editor.canvas_input(CanvasInput::Cancel)?;
    assert_eq!(
        shown(&mut editor)?
            .geometry
            .mesh(&selected.id)
            .unwrap()
            .vertices,
        positions
    );
    Ok(())
}

#[test]
fn opacity_reuses_positions_with_fresh_picking_visibility_and_history() -> anyhow::Result<()> {
    let (mut editor, _dir) = setup()?;
    let top = editor
        .layers()
        .into_iter()
        .find(|l| l.name == "Second")
        .unwrap()
        .id;
    editor.execute(EditorCommand::Select(top.clone()))?;
    let original = shown(&mut editor)?;
    let point = ModelPoint { x: 128., y: 128. };
    assert_eq!(original.geometry.mesh_hits(point)[0], top);
    for opacity in [0., 0.5, 1., 0.] {
        editor.execute(EditorCommand::PreviewOpacity(opacity))?;
        let frame = shown(&mut editor)?;
        assert!(Arc::ptr_eq(
            &original.geometry.meshes,
            &frame.geometry.meshes
        ));
        assert!(Arc::ptr_eq(&original.geometry.index, &frame.geometry.index));
        assert_eq!(frame.geometry.mesh_visible(&top), opacity > 0.);
        assert_eq!(frame.geometry.mesh_hits(point).contains(&top), opacity > 0.);
        assert_eq!(editor.overlay().vertices.is_empty(), opacity == 0.);
        assert!(original.geometry.mesh_visible(&top));
        assert_eq!(original.geometry.mesh_hits(point)[0], top);
    }
    editor.execute(EditorCommand::FinishEdit)?;
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(shown(&mut editor)?.geometry.mesh_hits(point)[0], top);
    editor.execute(EditorCommand::Redo)?;
    assert!(!shown(&mut editor)?.geometry.mesh_hits(point).contains(&top));
    editor.execute(EditorCommand::PreviewOpacity(1.))?;
    shown(&mut editor)?;
    editor.execute(EditorCommand::CancelEdit)?;
    assert!(!shown(&mut editor)?.geometry.mesh_visible(&top));
    Ok(())
}

#[test]
fn canvas_metadata_refreshes_on_rename_cancel_property_edit_and_history() -> anyhow::Result<()> {
    let (doc, _dir, warp, _, _) = rich_document()?;
    let mesh = doc.context().selection.clone().unwrap();
    let mut editor = Editor::from_document(doc);
    editor.set_canvas_metrics(CanvasMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    let original = shown(&mut editor)?;
    editor.execute(EditorCommand::PreviewName("Renamed".into()))?;
    let renamed = shown(&mut editor)?;
    assert_eq!(renamed.geometry.index.name(&mesh), "Renamed");
    assert_eq!(original.geometry.index.name(&mesh), "Mesh");
    editor.execute(EditorCommand::CancelEdit)?;
    assert_eq!(shown(&mut editor)?.geometry.index.name(&mesh), "Mesh");
    let warp = ObjectRef::new(ObjectKind::Deformer, warp);
    editor.execute(EditorCommand::SetObjectProperty {
        object: warp.clone(),
        field: "name".into(),
        value: "Updated Warp".into(),
    })?;
    assert_eq!(
        shown(&mut editor)?.geometry.index.name(&warp),
        "Updated Warp"
    );
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(shown(&mut editor)?.geometry.index.name(&warp), "Warp");
    editor.execute(EditorCommand::Redo)?;
    assert_eq!(
        shown(&mut editor)?.geometry.index.name(&warp),
        "Updated Warp"
    );
    assert_eq!(renamed.geometry.index.name(&mesh), "Renamed");
    Ok(())
}
