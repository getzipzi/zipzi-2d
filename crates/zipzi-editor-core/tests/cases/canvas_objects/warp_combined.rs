use super::common::{click, control_points, drag};
use crate::support::{pointer_input, present_canvas as shown};
use zipzi_editor_core::*;

fn fixture() -> anyhow::Result<(Editor, ObjectRef)> {
    let mut document = Document::new_blank()?;
    let object = ObjectRef::new(ObjectKind::Deformer, uuid::Uuid::new_v4().to_string());
    let plan = document.plan_scene_authoring(zipzi_sdk::SceneEdit::Create {
        id: object.id.clone(),
        name: "Warp".into(),
        object: zipzi_sdk::SceneCreate::Warp {
            bounds: Some([32., 32., 224., 224.]),
        },
        part_id: None,
        parent_id: None,
        before: None,
        deformation_before: None,
    })?;
    document.execute_scene_authoring(&plan)?;
    let mut editor = crate::support::editor_from_document(document);
    editor.set_canvas_metrics(ViewportMetrics {
        width: 512.,
        height: 512.,
        scale_factor: 1.,
    });
    editor.reset_view();
    editor.execute(EditorCommand::SelectObject(object.clone()))?;
    shown(&mut editor)?;
    Ok((editor, object))
}

#[test]
fn combined_controls_hit_bezier_at_overlaps_and_edit_grid_without_changing_the_controller()
-> anyhow::Result<()> {
    let (mut editor, object) = fixture()?;
    let overlay = editor.overlay();
    assert_eq!(
        overlay
            .vertices
            .iter()
            .filter(|v| v.kind == OverlayVertexKind::WarpGrid)
            .count(),
        36
    );
    assert_eq!(
        overlay
            .vertices
            .iter()
            .filter(|v| v.kind == OverlayVertexKind::Warp)
            .count(),
        9
    );
    let corner = overlay
        .vertices
        .iter()
        .find(|v| v.kind == OverlayVertexKind::Warp)
        .unwrap()
        .position;
    click(&mut editor, corner, false)?;
    let overlay = editor.overlay();
    assert!(
        overlay
            .vertices
            .iter()
            .filter(|v| v.selected)
            .all(|v| v.kind == OverlayVertexKind::Warp)
    );
    let tangent = overlay.handles.iter().find(|h| h.tangent).unwrap().position;
    drag(
        &mut editor,
        tangent,
        ViewportPoint {
            x: tangent.x,
            y: tangent.y - 15.,
        },
        false,
    )?;
    let before = control_points(&mut editor, &object)?;
    let index = editor.document().unwrap().object_index();
    let controller = index.get(&object).unwrap().properties["warp"]["bezier"].clone();
    assert!(controller.is_object());
    let grid = editor
        .overlay()
        .vertices
        .into_iter()
        .find(|v| v.kind == OverlayVertexKind::WarpGrid && v.id == 14)
        .unwrap()
        .position;
    drag(
        &mut editor,
        grid,
        ViewportPoint {
            x: grid.x + 7.,
            y: grid.y - 5.,
        },
        false,
    )?;
    let after = control_points(&mut editor, &object)?;
    assert!((after[14].x - before[14].x - 7.).abs() < 0.001);
    assert!((after[14].y - before[14].y - 5.).abs() < 0.001);
    for i in 0..before.len() {
        if i != 14 {
            assert_eq!(after[i], before[i]);
        }
    }
    assert_eq!(
        editor
            .document()
            .unwrap()
            .object_index()
            .get(&object)
            .unwrap()
            .properties["warp"]["bezier"],
        controller
    );
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(control_points(&mut editor, &object)?, before);
    Ok(())
}

#[test]
fn single_grid_anchor_or_tangent_keeps_whole_frame_and_remains_directly_draggable()
-> anyhow::Result<()> {
    for kind in 0..3 {
        let (mut editor, object) = fixture()?;
        let before = control_points(&mut editor, &object)?;
        let history = editor.document().unwrap().history_lengths();
        let overlay = editor.overlay();
        assert!(!overlay.warp_frame_lines.is_empty());
        let whole_handles: Vec<_> = overlay
            .handles
            .iter()
            .filter(|h| !h.tangent)
            .map(|h| h.position)
            .collect();
        let point = match kind {
            0 => {
                overlay
                    .vertices
                    .iter()
                    .find(|v| v.kind == OverlayVertexKind::WarpGrid && v.id == 14)
                    .unwrap()
                    .position
            }
            1 => {
                overlay
                    .vertices
                    .iter()
                    .find(|v| v.kind == OverlayVertexKind::Warp)
                    .unwrap()
                    .position
            }
            _ => overlay.handles.iter().find(|h| h.tangent).unwrap().position,
        };
        click(&mut editor, point, false)?;
        assert!(
            !editor.overlay().warp_frame_lines.is_empty(),
            "single control kind {kind}"
        );
        assert_eq!(
            editor
                .overlay()
                .handles
                .iter()
                .filter(|h| !h.tangent)
                .map(|h| h.position)
                .collect::<Vec<_>>(),
            whole_handles
        );
        editor.hover_canvas(
            editor.primary_viewport_binding().unwrap(),
            whole_handles.last().copied(),
        )?;
        assert_eq!(
            editor.overlay().active_handle,
            Some(DeformerHandle::Rotate),
            "single control kind {kind} keeps whole-frame hit targets"
        );
        let end = ViewportPoint {
            x: point.x + 10.,
            y: point.y - 7.,
        };
        drag(&mut editor, point, end, false)?;
        shown(&mut editor)?;
        let overlay = editor.overlay();
        assert!(!overlay.warp_frame_lines.is_empty());
        assert_eq!(overlay.handles.iter().filter(|h| !h.tangent).count(), 9);
        let selected = if kind == 2 {
            overlay
                .handles
                .iter()
                .find(|h| h.tangent && h.selected)
                .unwrap()
                .position
        } else {
            overlay
                .vertices
                .iter()
                .find(|v| v.selected)
                .unwrap()
                .position
        };
        assert!(
            (selected.x - end.x).abs() < 0.001 && (selected.y - end.y).abs() < 0.001,
            "kind {kind}: {selected:?} != {end:?}"
        );
        assert_eq!(
            editor.document().unwrap().history_lengths(),
            (history.0 + 1, 0)
        );
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(control_points(&mut editor, &object)?, before);
        // Undo clears component selection; select the first control again.
        click(&mut editor, point, false)?;
        let overlay = editor.overlay();
        let second = match kind {
            0 => {
                overlay
                    .vertices
                    .iter()
                    .find(|v| v.kind == OverlayVertexKind::WarpGrid && v.id == 15)
                    .unwrap()
                    .position
            }
            1 => {
                overlay
                    .vertices
                    .iter()
                    .filter(|v| v.kind == OverlayVertexKind::Warp)
                    .nth(1)
                    .unwrap()
                    .position
            }
            _ => {
                overlay
                    .handles
                    .iter()
                    .filter(|h| h.tangent)
                    .nth(1)
                    .unwrap()
                    .position
            }
        };
        click(&mut editor, second, true)?;
        assert_eq!(
            editor
                .warp_transform_target(&object)
                .unwrap()
                .components
                .len(),
            2
        );
        assert!(!editor.overlay().warp_frame_lines.is_empty());
        assert_eq!(
            editor
                .overlay()
                .handles
                .iter()
                .filter(|h| !h.tangent)
                .count(),
            9
        );
    }
    Ok(())
}

#[test]
fn mixed_grid_and_bezier_selection_moves_each_lattice_point_once_and_cancels_atomically()
-> anyhow::Result<()> {
    let (mut editor, object) = fixture()?;
    let before = control_points(&mut editor, &object)?;
    let history = editor.document().unwrap().history_lengths();
    let overlay = editor.overlay();
    let anchor = overlay
        .vertices
        .iter()
        .find(|v| v.kind == OverlayVertexKind::Warp)
        .unwrap()
        .position;
    let grid = overlay
        .vertices
        .iter()
        .find(|v| v.kind == OverlayVertexKind::WarpGrid && v.id == 14)
        .unwrap()
        .position;
    click(&mut editor, anchor, false)?;
    click(&mut editor, grid, true)?;
    assert_eq!(
        editor
            .warp_transform_target(&object)
            .unwrap()
            .components
            .len(),
        2
    );
    editor.nudge_selection(
        editor.primary_viewport_binding().unwrap(),
        ModelVector { x: 7., y: 5. },
    )?;
    let after = control_points(&mut editor, &object)?;
    for i in [0, 14] {
        assert!((after[i].x - before[i].x - 7.).abs() < 0.001);
        assert!((after[i].y - before[i].y - 5.).abs() < 0.001);
    }
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(control_points(&mut editor, &object)?, before);
    shown(&mut editor)?;
    editor.dispatch(editor.command(CommandId::SelectAll))?;
    assert_eq!(
        editor
            .overlay()
            .vertices
            .iter()
            .filter(|v| v.selected)
            .count(),
        45
    );
    let start = editor
        .overlay()
        .vertices
        .iter()
        .find(|v| v.kind == OverlayVertexKind::Warp)
        .unwrap()
        .position;
    pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: ViewportPoint {
                x: start.x + 10.,
                y: start.y - 5.,
            },
            left_down: true,
        },
    )?;
    pointer_input(&mut editor, CanvasInput::Cancel)?;
    assert_eq!(control_points(&mut editor, &object)?, before);
    assert_eq!(editor.document().unwrap().history_lengths(), (history.0, 1));
    shown(&mut editor)?;
    pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
            primary: false,
        },
    )?;
    pointer_input(
        &mut editor,
        CanvasInput::Move {
            point: ViewportPoint {
                x: start.x + 10.,
                y: start.y - 5.,
            },
            left_down: true,
        },
    )?;
    pointer_input(&mut editor, CanvasInput::Up { point: start })?;
    assert_eq!(control_points(&mut editor, &object)?, before);
    assert_eq!(
        editor.document().unwrap().history_lengths(),
        (history.0, 1),
        "returning a mixed selection to the press point must preserve redo and absent controller metadata"
    );
    shown(&mut editor)?;
    editor.nudge_selection(
        editor.primary_viewport_binding().unwrap(),
        ModelVector { x: 3., y: -2. },
    )?;
    for (a, b) in before.iter().zip(control_points(&mut editor, &object)?) {
        assert!((b.x - a.x - 3.).abs() < 0.001);
        assert!((b.y - a.y + 2.).abs() < 0.001);
    }
    assert_eq!(
        editor.document().unwrap().history_lengths(),
        (history.0 + 1, 0)
    );
    Ok(())
}
