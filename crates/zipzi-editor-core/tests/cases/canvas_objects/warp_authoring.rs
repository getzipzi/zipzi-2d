use super::common::*;
use crate::support::{pointer_input, present_canvas as shown};
use zipzi_core::{Vec2, WarpBezier};
use zipzi_editor_core::*;

fn stored(editor: &Editor, object: &ObjectRef) -> WarpBezier {
    serde_json::from_value(
        editor
            .document()
            .unwrap()
            .object_index()
            .get(object)
            .unwrap()
            .properties["warp"]["bezier"]
            .clone(),
    )
    .unwrap()
}

#[test]
fn pointer_smoothing_modes_move_neighboring_anchors_with_the_documented_influence()
-> anyhow::Result<()> {
    for (mode, weight) in [
        ("local", 0.),
        ("smooth_local", 0.5),
        ("smooth_global", (-0.5f32).exp()),
    ] {
        let (mut editor, dir, object) = deformer_editor(false)?;
        explicit_warp_grid(&mut editor, &dir, &object)?;
        editor.execute(EditorCommand::SetObjectProperty {
            object: object.clone(),
            field: "warp_authoring.edit_mode".into(),
            value: serde_json::json!(mode),
        })?;
        let frame = shown(&mut editor)?;
        let start = editor.overlay().vertices[4].position;
        pointer_input(
            &mut editor,
            CanvasInput::Down {
                point: start,
                shift: false,
                alt: false,
                primary: false,
            },
        )?;
        let end = ViewportPoint {
            x: start.x + 12.,
            y: start.y,
        };
        pointer_input(
            &mut editor,
            CanvasInput::Move {
                point: end,
                left_down: true,
            },
        )?;
        assert_eq!(editor.overlay().warp_bezier_mode.is_some(), mode != "local");
        pointer_input(&mut editor, CanvasInput::Up { point: end })?;
        let controller = stored(&editor, &object);
        let center = frame.transform.to_model(start);
        assert!((controller.nodes[4].position.x - center.x - 12.).abs() < 0.001);
        assert!((controller.nodes[5].position.x - center.x - 96. - 12. * weight).abs() < 0.001);
        assert_eq!(controller.nodes[4].position.y, center.y);
        let after = controller.clone();
        editor.execute(EditorCommand::Undo)?;
        assert!(
            editor
                .document()
                .unwrap()
                .object_index()
                .get(&object)
                .unwrap()
                .properties["warp"]["bezier"]
                .is_null()
        );
        editor.execute(EditorCommand::Redo)?;
        assert_eq!(stored(&editor, &object), after);
    }
    Ok(())
}

#[test]
fn pointer_structure_mode_couples_only_the_opposite_tangent() -> anyhow::Result<()> {
    use zipzi_core::WarpBezierEvaluation;
    let (mut editor, dir, object) = deformer_editor(false)?;
    explicit_warp_grid(&mut editor, &dir, &object)?;
    editor.execute(EditorCommand::SetObjectProperty {
        object: object.clone(),
        field: "warp_authoring.edit_mode".into(),
        value: serde_json::json!("preserve_structure"),
    })?;
    let frame = shown(&mut editor)?;
    let points = control_points(&mut editor, &object)?
        .iter()
        .map(|p| Vec2::new(p.x, p.y))
        .collect::<Vec<_>>();
    let before = WarpBezier::from_lattice_grid(
        4,
        4,
        true,
        &points,
        zipzi_core::WarpBezierGrid {
            rows: 2,
            columns: 2,
        },
    );
    let p = before.nodes[4].handles[1];
    let start = frame.transform.to_viewport(ModelPoint { x: p.x, y: p.y });
    drag(
        &mut editor,
        start,
        ViewportPoint {
            x: start.x + 24.,
            y: start.y + 16.,
        },
        false,
    )?;
    let after = stored(&editor, &object);
    for (i, (a, b)) in before.nodes.iter().zip(&after.nodes).enumerate() {
        assert_eq!(a.position, b.position);
        if i != 4 {
            assert_eq!(a, b);
        }
    }
    let node = &after.nodes[4];
    let left = Vec2::new(
        node.handles[0].x - node.position.x,
        node.handles[0].y - node.position.y,
    );
    let right = Vec2::new(
        node.handles[1].x - node.position.x,
        node.handles[1].y - node.position.y,
    );
    assert!((left.x * right.y - left.y * right.x).abs() < 0.001);
    assert!((left.x.hypot(left.y) - right.x.hypot(right.y)).abs() < 0.001);
    assert_ne!(before.nodes[4].handles[0], after.nodes[4].handles[0]);
    Ok(())
}
