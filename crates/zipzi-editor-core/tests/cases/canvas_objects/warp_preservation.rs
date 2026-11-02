use super::common::*;
use crate::support::{pointer_input, present_canvas as shown};
use zipzi_editor_core::*;

fn mesh_points(editor: &mut Editor) -> anyhow::Result<Vec<ModelPoint>> {
    Ok(shown(editor)?
        .geometry
        .meshes
        .iter()
        .flat_map(|m| m.vertices.iter().map(|(_, p)| *p))
        .collect())
}
fn same(a: &[ModelPoint], b: &[ModelPoint]) {
    assert_eq!(a.len(), b.len());
    for (a, b) in a.iter().zip(b) {
        assert!((a.x - b.x).hypot(a.y - b.y) < 0.001, "{a:?} != {b:?}");
    }
}
fn edge(editor: &mut Editor, object: &ObjectRef) -> anyhow::Result<ViewportPoint> {
    let frame = shown(editor)?;
    let c = frame
        .geometry
        .controls
        .iter()
        .find(|c| c.object == *object)
        .unwrap();
    let g = frame.geometry.deformer_gizmo(c, frame.transform).unwrap();
    let (a, b) = g.outline[0];
    Ok(ViewportPoint {
        x: (a.x + b.x) * 0.5,
        y: (a.y + b.y) * 0.5,
    })
}

#[test]
fn command_drag_freezes_policy_preserves_children_and_returns_exactly_to_the_press_snapshot()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let initial = control_points(&mut editor, &object)?;
    let children = mesh_points(&mut editor)?;
    for finish in [0, 1, 2] {
        let start = edge(&mut editor, &object)?;
        pointer_input(
            &mut editor,
            CanvasInput::Down {
                point: start,
                shift: false,
                alt: false,
                primary: true,
            },
        )?;
        let end = ViewportPoint {
            x: start.x + 14.,
            y: start.y + 9.,
        };
        pointer_input(
            &mut editor,
            CanvasInput::Move {
                point: end,
                left_down: true,
            },
        )?;
        assert!(editor.overlay().warp_keep_children);
        same(&children, &mesh_points(&mut editor)?);
        assert_ne!(initial, control_points(&mut editor, &object)?);
        pointer_input(
            &mut editor,
            match finish {
                0 => CanvasInput::Cancel,
                1 => CanvasInput::Up { point: start },
                _ => CanvasInput::Up { point: end },
            },
        )?;
        if finish < 2 {
            assert_eq!(control_points(&mut editor, &object)?, initial);
            assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
        }
    }
    assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
    same(&children, &mesh_points(&mut editor)?);
    editor.execute(EditorCommand::Undo)?;
    assert_eq!(control_points(&mut editor, &object)?, initial);
    editor.execute(EditorCommand::Redo)?;
    same(&children, &mesh_points(&mut editor)?);
    assert!(!editor.overlay().warp_keep_children);
    let start = edge(&mut editor, &object)?;
    drag(
        &mut editor,
        start,
        ViewportPoint {
            x: start.x + 12.,
            y: start.y,
        },
        false,
    )?;
    assert_ne!(
        children,
        mesh_points(&mut editor)?,
        "the next ordinary drag must move the children"
    );
    Ok(())
}

#[test]
fn command_rotation_and_scaling_preserve_children_without_a_persistent_mode() -> anyhow::Result<()>
{
    for rotate in [true, false] {
        let (mut editor, _dir, object) = deformer_editor(false)?;
        let initial = control_points(&mut editor, &object)?;
        let children = mesh_points(&mut editor)?;
        let frame = shown(&mut editor)?;
        let control = frame
            .geometry
            .controls
            .iter()
            .find(|c| c.object == object)
            .unwrap();
        let gizmo = frame
            .geometry
            .deformer_gizmo(control, frame.transform)
            .unwrap();
        let handle = if rotate {
            DeformerHandle::Rotate
        } else {
            DeformerHandle::Scale(4)
        };
        let start = gizmo.handles.iter().find(|(h, _)| *h == handle).unwrap().1;
        let (x, y) = (start.x - gizmo.center.x, start.y - gizmo.center.y);
        let end = if rotate {
            let (s, c) = 23f32.to_radians().sin_cos();
            ViewportPoint {
                x: gizmo.center.x + x * c - y * s,
                y: gizmo.center.y + x * s + y * c,
            }
        } else {
            ViewportPoint {
                x: gizmo.center.x + x * 0.8,
                y: gizmo.center.y + y * 0.8,
            }
        };
        pointer_input(
            &mut editor,
            CanvasInput::Down {
                point: start,
                shift: false,
                alt: false,
                primary: true,
            },
        )?;
        pointer_input(
            &mut editor,
            CanvasInput::Move {
                point: end,
                left_down: true,
            },
        )?;
        assert!(editor.overlay().warp_keep_children);
        same(&children, &mesh_points(&mut editor)?);
        pointer_input(&mut editor, CanvasInput::Up { point: end })?;
        assert_ne!(initial, control_points(&mut editor, &object)?);
        assert!(!editor.overlay().warp_keep_children);
        same(&children, &mesh_points(&mut editor)?);
        assert_eq!(editor.document().unwrap().history_lengths(), (1, 0));
        editor.execute(EditorCommand::Undo)?;
        assert_eq!(initial, control_points(&mut editor, &object)?);
        editor.execute(EditorCommand::Redo)?;
        same(&children, &mesh_points(&mut editor)?);
    }
    Ok(())
}

#[test]
fn nonlinear_keep_children_pointer_edit_is_refused_without_falling_back_to_an_ordinary_drag()
-> anyhow::Result<()> {
    let (mut editor, _dir, object) = deformer_editor(false)?;
    let initial = control_points(&mut editor, &object)?;
    let frame = shown(&mut editor)?;
    let start = frame.transform.to_viewport(initial[0]);
    pointer_input(
        &mut editor,
        CanvasInput::Down {
            point: start,
            shift: false,
            alt: false,
            primary: true,
        },
    )?;
    let result = pointer_input(
        &mut editor,
        CanvasInput::Up {
            point: ViewportPoint {
                x: start.x + 13.,
                y: start.y,
            },
        },
    );
    assert!(result.is_err());
    assert!(!editor.gesture_active());
    assert!(!editor.document().unwrap().has_pending_edit());
    assert_eq!(initial, control_points(&mut editor, &object)?);
    assert_eq!(editor.document().unwrap().history_lengths(), (0, 0));
    Ok(())
}
