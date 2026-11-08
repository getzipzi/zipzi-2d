use super::*;
use CanvasGestureKind as G;
use CursorStyle as C;
use ManualMeshFeedback as F;
use ManualTool as M;
use ToolCursor as T;

fn input(overlay: &OverlaySnapshot) -> CursorInput<'_> {
    CursorInput {
        overlay,
        tool: ToolId::Select,
        manual_tool: None,
        modifiers: Modifiers::default(),
        window_active: true,
        modal: false,
        panning: false,
        hand_tool: false,
    }
}

#[test]
fn tool_cursor_manual_idle_matches_click_and_shortcut_routes() {
    let primary = Modifiers {
        platform: true,
        ..Default::default()
    };
    let delete = Modifiers {
        platform: true,
        shift: true,
        ..Default::default()
    };
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    let alt = Modifiers {
        alt: true,
        ..Default::default()
    };
    for (tool, feedback, modifiers, expected) in [
        (M::Select, F::Empty, Modifiers::default(), T::MeshAdd),
        (M::Lasso, F::AutomaticEdge, Modifiers::default(), T::MeshAdd),
        (
            M::Select,
            F::Vertex,
            Modifiers::default(),
            T::Standard(C::OpenHand),
        ),
        (M::Select, F::FixedEdge, shift, T::Standard(C::Crosshair)),
        (M::Lasso, F::Empty, alt, T::Standard(C::Crosshair)),
        (M::Create, F::FixedEdge, Modifiers::default(), T::MeshAdd),
        (
            M::Create,
            F::Vertex,
            Modifiers::default(),
            T::MeshConnect { new_vertex: false },
        ),
        (M::Delete, F::Vertex, Modifiers::default(), T::MeshDelete),
        (
            M::Delete,
            F::Empty,
            Modifiers::default(),
            T::Standard(C::OperationNotAllowed),
        ),
        (
            M::Select,
            F::Vertex,
            primary,
            T::MeshConnect { new_vertex: false },
        ),
        (M::Delete, F::AutomaticEdge, primary, T::MeshFixEdge),
        (
            M::Create,
            F::FixedEdge,
            primary,
            T::Standard(C::OperationNotAllowed),
        ),
        (
            M::Lasso,
            F::Empty,
            primary,
            T::Standard(C::OperationNotAllowed),
        ),
        (M::Create, F::Vertex, delete, T::MeshDelete),
        (M::Select, F::AutomaticEdge, delete, T::MeshDelete),
        (
            M::Select,
            F::Empty,
            delete,
            T::Standard(C::OperationNotAllowed),
        ),
    ] {
        let overlay = OverlaySnapshot {
            manual_mesh_feedback: Some(feedback),
            ..Default::default()
        };
        let mut input = input(&overlay);
        input.manual_tool = Some(tool);
        input.modifiers = modifiers;
        assert_eq!(
            resolve(input).cursor,
            Some(expected),
            "{tool:?}, {feedback:?}, {modifiers:?}"
        );
    }
}

#[test]
fn tool_cursor_captured_operations_ignore_toolbar_and_released_modifiers() {
    for (gesture, feedback, expected) in [
        (G::MeshAdd, F::Empty, T::MeshAdd),
        (G::MeshDelete, F::Vertex, T::MeshDelete),
        (
            G::MeshConnect,
            F::ConnectVertex,
            T::MeshConnect { new_vertex: false },
        ),
        (
            G::MeshConnect,
            F::ConnectNewVertex,
            T::MeshConnect { new_vertex: true },
        ),
        (G::MeshFixEdge, F::FixEdge, T::MeshFixEdge),
        (G::Rotate, F::Empty, T::Rotate),
        (G::Scale, F::Empty, T::Standard(C::ResizeLeftRight)),
        (G::BoneLength, F::Empty, T::BoneLength(0)),
        (G::Move, F::Vertex, T::Standard(C::ClosedHand)),
        (G::Cancelled, F::Cancelled, T::Standard(C::Arrow)),
        (
            G::Unavailable,
            F::Empty,
            T::Standard(C::OperationNotAllowed),
        ),
    ] {
        for modifiers in [
            Modifiers::default(),
            Modifiers {
                platform: true,
                shift: true,
                alt: true,
                ..Default::default()
            },
        ] {
            let overlay = OverlaySnapshot {
                active_gesture: Some(gesture),
                manual_mesh_feedback: Some(feedback),
                ..Default::default()
            };
            let mut input = input(&overlay);
            input.tool = ToolId::Hand;
            input.hand_tool = true;
            input.manual_tool = Some(M::Lasso);
            input.modifiers = modifiers;
            assert_eq!(
                resolve(input).cursor,
                Some(expected),
                "{gesture:?}, {modifiers:?}"
            );
        }
    }
}

#[test]
fn tool_cursor_navigation_and_modal_precedence_preserve_available_actions() {
    let overlay = OverlaySnapshot {
        point_unavailable: Some("locked".into()),
        active_gesture: Some(G::Rotate),
        ..Default::default()
    };
    let mut pan = input(&overlay);
    pan.panning = true;
    assert_eq!(resolve(pan).cursor, Some(T::Standard(C::ClosedHand)));
    let mut modal = input(&overlay);
    modal.modal = true;
    modal.panning = true;
    assert_eq!(resolve(modal).cursor, None);
    assert_eq!(
        resolve(input(&overlay)).cursor,
        Some(T::Standard(C::OperationNotAllowed))
    );
    let overlay = OverlaySnapshot {
        active_gesture: None,
        ..overlay
    };
    let mut hand = input(&overlay);
    hand.hand_tool = true;
    assert_eq!(resolve(hand).cursor, Some(T::Standard(C::OpenHand)));
}

#[test]
fn tool_cursor_glue_references_block_deletion_without_blocking_move_or_connect() {
    let overlay = OverlaySnapshot {
        manual_mesh_feedback: Some(F::Vertex),
        manual_mesh_delete_unavailable: true,
        ..Default::default()
    };
    let mut moving = input(&overlay);
    moving.manual_tool = Some(M::Select);
    assert_eq!(resolve(moving).cursor, Some(T::Standard(C::OpenHand)));
    let mut connecting = input(&overlay);
    connecting.manual_tool = Some(M::Select);
    connecting.modifiers.platform = true;
    assert_eq!(
        resolve(connecting).cursor,
        Some(T::MeshConnect { new_vertex: false })
    );
    let mut deleting = input(&overlay);
    deleting.manual_tool = Some(M::Select);
    deleting.modifiers = Modifiers {
        platform: true,
        shift: true,
        ..Default::default()
    };
    assert_eq!(
        resolve(deleting).cursor,
        Some(T::Standard(C::OperationNotAllowed))
    );
    let overlay = OverlaySnapshot {
        active_gesture: Some(G::MeshDelete),
        ..overlay
    };
    assert_eq!(
        resolve(input(&overlay)).cursor,
        Some(T::Standard(C::OperationNotAllowed))
    );
}

#[test]
fn tool_cursor_brush_capture_uses_actual_operation_and_controls_preview() {
    for (gesture, expected, hidden) in [
        (Some(G::BrushSelect), C::Crosshair, false),
        (Some(G::BrushResize), C::Crosshair, false),
        (Some(G::Move), C::ClosedHand, true),
        (None, C::OpenHand, true),
    ] {
        let overlay = OverlaySnapshot {
            active_gesture: gesture,
            ..Default::default()
        };
        let mut input = input(&overlay);
        input.tool = ToolId::BrushSelect;
        input.modifiers.platform = true;
        assert_eq!(
            resolve(input),
            CursorFeedback {
                cursor: Some(T::Standard(expected)),
                hide_selection_brush: hidden
            }
        );
    }
    let overlay = OverlaySnapshot {
        active_gesture: Some(G::BrushSelect),
        point_unavailable: Some("locked".into()),
        ..Default::default()
    };
    assert_eq!(
        resolve(input(&overlay)).cursor,
        Some(T::Standard(C::Crosshair))
    );
}

#[test]
fn tool_cursor_bone_length_is_distinct_from_scaling_at_each_axis() {
    for (radians, direction, expected) in [
        (0., 0, C::ResizeLeftRight),
        (std::f32::consts::FRAC_PI_4, 1, C::ResizeUpLeftDownRight),
        (std::f32::consts::FRAC_PI_2, 2, C::ResizeUpDown),
        (-std::f32::consts::FRAC_PI_4, 3, C::ResizeUpRightDownLeft),
    ] {
        let overlay = OverlaySnapshot {
            active_handle: Some(DeformerHandle::RotationScale),
            active_handle_radians: radians,
            ..Default::default()
        };
        assert_eq!(resolve(input(&overlay)).cursor, Some(T::Standard(expected)));
        let mut length = input(&overlay);
        length.modifiers.alt = true;
        assert_eq!(resolve(length).cursor, Some(T::BoneLength(direction)));
        let captured = OverlaySnapshot {
            active_gesture: Some(G::BoneLength),
            ..overlay
        };
        assert_eq!(
            resolve(input(&captured)).cursor,
            Some(T::BoneLength(direction))
        );
    }
}
