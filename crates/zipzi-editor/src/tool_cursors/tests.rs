use super::*;
use CanvasGestureKind as G;
use CanvasSelectionOperation as O;
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

fn primary_modifiers() -> Modifiers {
    if cfg!(target_os = "macos") {
        Modifiers {
            platform: true,
            ..Default::default()
        }
    } else {
        Modifiers {
            control: true,
            ..Default::default()
        }
    }
}

#[test]
fn tool_cursor_manual_idle_matches_click_and_shortcut_routes() {
    let primary = primary_modifiers();
    let delete = Modifiers {
        shift: true,
        ..primary
    };
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    for (tool, feedback, modifiers, expected) in [
        (M::Select, F::Empty, Modifiers::default(), T::MeshAdd),
        (M::Lasso, F::AutomaticEdge, Modifiers::default(), T::MeshAdd),
        (M::Select, F::Vertex, Modifiers::default(), T::Move),
        (M::Select, F::FixedEdge, shift, T::BoxSelect(O::Add)),
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
            selection_operation: modifiers.shift.then_some(O::Add),
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
        (G::Rotate, F::Empty, T::Rotate(0)),
        (G::Scale, F::Empty, T::Standard(C::ResizeLeftRight)),
        (G::BoneLength, F::Empty, T::BoneLength(0)),
        (G::Move, F::Vertex, T::Move),
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
    assert_eq!(resolve(moving).cursor, Some(T::Move));
    let mut connecting = input(&overlay);
    connecting.manual_tool = Some(M::Select);
    connecting.modifiers = primary_modifiers();
    assert_eq!(
        resolve(connecting).cursor,
        Some(T::MeshConnect { new_vertex: false })
    );
    let mut deleting = input(&overlay);
    deleting.manual_tool = Some(M::Select);
    deleting.modifiers = Modifiers {
        shift: true,
        ..primary_modifiers()
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
        (Some(G::BrushSelect), T::Brush(O::Replace), false),
        (Some(G::BrushResize), T::Standard(C::Crosshair), false),
        (Some(G::Move), T::Move, true),
        (None, T::Move, true),
    ] {
        let overlay = OverlaySnapshot {
            active_gesture: gesture,
            ..Default::default()
        };
        let mut input = input(&overlay);
        input.tool = ToolId::BrushSelect;
        input.modifiers = primary_modifiers();
        assert_eq!(
            resolve(input),
            CursorFeedback {
                cursor: Some(expected),
                hide_selection_brush: hidden
            }
        );
    }
    let overlay = OverlaySnapshot {
        active_gesture: Some(G::BrushSelect),
        point_unavailable: Some("locked".into()),
        ..Default::default()
    };
    assert_eq!(resolve(input(&overlay)).cursor, Some(T::Brush(O::Replace)));
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
            active_control_kind: Some(ControlKind::Rotation),
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

#[test]
fn tool_cursor_rotation_follows_displayed_orientation_before_and_during_drag() {
    for (radians, frame_direction, bone_direction) in [
        (0., 0, 4),
        (std::f32::consts::FRAC_PI_4, 2, 6),
        (std::f32::consts::FRAC_PI_2, 4, 8),
        (std::f32::consts::PI, 8, 12),
        (-std::f32::consts::FRAC_PI_4, 14, 2),
        (std::f32::consts::TAU + std::f32::consts::FRAC_PI_4, 2, 6),
        (f32::NAN, 0, 4),
        (f32::INFINITY, 0, 4),
    ] {
        for (kind, expected) in [
            (None, frame_direction),
            (Some(ControlKind::Warp), frame_direction),
            (Some(ControlKind::Rotation), bone_direction),
        ] {
            for gesture in [None, Some(G::Rotate)] {
                let overlay = OverlaySnapshot {
                    active_handle: Some(DeformerHandle::Rotate),
                    active_control_kind: kind,
                    active_handle_radians: radians,
                    active_gesture: gesture,
                    ..Default::default()
                };
                assert_eq!(
                    resolve(input(&overlay)).cursor,
                    Some(T::Rotate(expected)),
                    "{kind:?}, {radians}, {gesture:?}"
                );
            }
        }
    }
}

#[test]
fn tool_cursor_rotated_artwork_keeps_its_center_and_fits_the_bitmap() {
    for direction in 0..ROTATION_DIRECTIONS {
        let cursor = rotation_artwork(direction).unwrap();
        assert_eq!(cursor.hotspot(), point(12, 12));
        for image in cursor.images() {
            let width = image.size().width as usize;
            let height = image.size().height as usize;
            for (index, pixel) in image.rgba().chunks_exact(4).enumerate() {
                let x = index % width;
                let y = index / width;
                if x == 0 || y == 0 || x + 1 == width || y + 1 == height {
                    assert_eq!(pixel[3], 0, "direction {direction}, pixel {x},{y}");
                }
            }
            assert!(image.rgba().chunks_exact(4).any(|pixel| pixel[3] > 0));
        }
    }
}

#[test]
fn tool_cursor_selection_badges_use_core_operations_instead_of_live_keys() {
    for operation in [O::Replace, O::Add, O::Remove, O::Toggle] {
        let overlay = OverlaySnapshot {
            active_gesture: Some(G::LassoSelect),
            selection_operation: Some(operation),
            ..Default::default()
        };
        let mut captured = input(&overlay);
        captured.modifiers = Modifiers {
            shift: true,
            alt: true,
            platform: true,
            ..Default::default()
        };
        assert_eq!(resolve(captured).cursor, Some(T::Lasso(operation)));
        let idle_overlay = OverlaySnapshot {
            active_gesture: None,
            ..overlay
        };
        let mut idle = input(&idle_overlay);
        idle.tool = ToolId::Lasso;
        assert_eq!(resolve(idle).cursor, Some(T::Lasso(operation)));
        if operation != O::Toggle {
            let overlay = OverlaySnapshot {
                active_gesture: Some(G::BrushSelect),
                ..idle_overlay
            };
            assert_eq!(resolve(input(&overlay)).cursor, Some(T::Brush(operation)));
            let idle_overlay = OverlaySnapshot {
                active_gesture: None,
                ..overlay
            };
            let mut brush = input(&idle_overlay);
            brush.tool = ToolId::BrushSelect;
            brush.modifiers.alt = operation != O::Remove;
            assert_eq!(resolve(brush).cursor, Some(T::Brush(operation)));
        }
    }
    let overlay = OverlaySnapshot {
        manual_mesh_feedback: Some(F::Empty),
        selection_operation: Some(O::Remove),
        ..Default::default()
    };
    let mut manual = input(&overlay);
    manual.manual_tool = Some(M::Lasso);
    manual.modifiers.shift = true;
    assert_eq!(resolve(manual).cursor, Some(T::Lasso(O::Remove)));
}

#[test]
fn tool_cursor_control_points_distinguish_motion_from_selection_only_clicks() {
    let component = zipzi_editor_core::ComponentRef {
        object: zipzi_editor_core::ObjectRef::new(zipzi_editor_core::ObjectKind::Mesh, "mesh"),
        id: 0,
    };
    for operation in [None, Some(O::Add), Some(O::Remove), Some(O::Toggle)] {
        let overlay = OverlaySnapshot {
            active_component: Some(component.clone()),
            selection_operation: operation,
            ..Default::default()
        };
        let expected = if matches!(operation, Some(O::Remove | O::Toggle)) {
            T::Standard(C::Arrow)
        } else {
            T::Move
        };
        assert_eq!(resolve(input(&overlay)).cursor, Some(expected));
        let captured = OverlaySnapshot {
            active_gesture: Some(G::Move),
            ..overlay
        };
        assert_eq!(resolve(input(&captured)).cursor, Some(expected));
        let restricted = OverlaySnapshot {
            active_gesture: None,
            point_unavailable: Some("locked".into()),
            ..captured
        };
        let expected = if expected == T::Move {
            T::Standard(C::OperationNotAllowed)
        } else {
            expected
        };
        assert_eq!(resolve(input(&restricted)).cursor, Some(expected));
    }
}

#[test]
fn tool_cursor_view_panning_keeps_hands_separate_from_point_motion() {
    let overlay = OverlaySnapshot {
        active_gesture: Some(G::Move),
        ..Default::default()
    };
    assert_eq!(resolve(input(&overlay)).cursor, Some(T::Move));
    let mut pan = input(&overlay);
    pan.panning = true;
    assert_eq!(resolve(pan).cursor, Some(T::Standard(C::ClosedHand)));
    let idle = OverlaySnapshot::default();
    let mut hand = input(&idle);
    hand.hand_tool = true;
    assert_eq!(resolve(hand).cursor, Some(T::Standard(C::OpenHand)));
    let box_selection = OverlaySnapshot {
        active_gesture: Some(G::BoxSelect),
        ..Default::default()
    };
    assert_eq!(
        resolve(input(&box_selection)).cursor,
        Some(T::Standard(C::Arrow))
    );
}

#[test]
fn tool_cursor_mesh_box_badges_follow_idle_and_captured_operations() {
    for operation in [O::Replace, O::Add, O::Remove, O::Toggle] {
        for gesture in [None, Some(G::BoxSelect)] {
            let overlay = OverlaySnapshot {
                active_gesture: gesture,
                manual_mesh_feedback: Some(F::Empty),
                selection_operation: Some(operation),
                ..Default::default()
            };
            let mut request = input(&overlay);
            request.manual_tool = Some(M::Select);
            // Idle modifiers enter selection; captured gestures retain the core operation
            // even after all keys are released.
            request.modifiers.shift = gesture.is_none();
            assert_eq!(resolve(request).cursor, Some(T::BoxSelect(operation)));
        }
    }
}
