//! Resolve canvas operations centrally and map them to native cursor resources.
//! Register artwork once per application; viewport requests only copy a style ID.
use gpui_kit::{App, CursorStyle, CustomCursor, Global, Modifiers, point, size};
use zipzi_editor_core::{
    CanvasGestureKind, DeformerHandle, ManualMeshFeedback, ManualTool, OverlaySnapshot, ToolId,
};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolCursor {
    Standard(CursorStyle),
    MeshAdd,
    MeshDelete,
    MeshConnect { new_vertex: bool },
    MeshFixEdge,
    Rotate,
    BoneLength(u8),
}

pub(crate) struct CursorInput<'a> {
    pub(crate) overlay: &'a OverlaySnapshot,
    pub(crate) tool: ToolId,
    pub(crate) manual_tool: Option<ManualTool>,
    pub(crate) modifiers: Modifiers,
    pub(crate) window_active: bool,
    pub(crate) modal: bool,
    pub(crate) panning: bool,
    pub(crate) hand_tool: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct CursorFeedback {
    pub(crate) cursor: Option<ToolCursor>,
    pub(crate) hide_selection_brush: bool,
}

/// One precedence order for navigation, captured operations and idle input routing.
pub(crate) fn resolve(input: CursorInput<'_>) -> CursorFeedback {
    let cursor = resolve_cursor(&input);
    CursorFeedback {
        hide_selection_brush: input.tool == ToolId::BrushSelect
            && matches!(
                cursor,
                Some(ToolCursor::Standard(
                    CursorStyle::OpenHand | CursorStyle::ClosedHand
                ))
            ),
        cursor,
    }
}

fn resolve_cursor(input: &CursorInput<'_>) -> Option<ToolCursor> {
    use CanvasGestureKind as G;
    use CursorStyle as C;
    use ToolCursor as T;
    let overlay = input.overlay;
    let standard = |style| Some(T::Standard(style));
    if input.modal {
        return None;
    }
    if input.panning {
        return standard(C::ClosedHand);
    }
    if let Some(gesture) = overlay.active_gesture {
        let selecting = matches!(
            gesture,
            G::BoxSelect | G::LassoSelect | G::BrushSelect | G::BrushResize
        );
        if !selecting && overlay.point_unavailable.is_some() {
            return standard(C::OperationNotAllowed);
        }
        return match gesture {
            G::MeshAdd => Some(T::MeshAdd),
            G::MeshDelete if overlay.manual_mesh_delete_unavailable => {
                standard(C::OperationNotAllowed)
            }
            G::MeshDelete => Some(T::MeshDelete),
            G::MeshConnect => Some(T::MeshConnect {
                new_vertex: overlay.manual_mesh_feedback
                    == Some(ManualMeshFeedback::ConnectNewVertex),
            }),
            G::MeshFixEdge => Some(T::MeshFixEdge),
            G::Rotate => Some(T::Rotate),
            G::BoneLength => Some(T::BoneLength(handle_direction(overlay))),
            G::Scale => standard(resize_style(handle_direction(overlay))),
            G::Move => standard(C::ClosedHand),
            G::Unavailable => standard(C::OperationNotAllowed),
            G::Cancelled => standard(C::Arrow),
            G::BoxSelect if input.manual_tool.is_none() => None,
            _ => standard(C::Crosshair),
        };
    }
    if input.hand_tool {
        return standard(C::OpenHand);
    }
    // Painting a selection is available even when the hovered point cannot be edited.
    if input.tool == ToolId::BrushSelect && input.manual_tool.is_none() {
        return if input.window_active && input.modifiers.platform {
            standard(if overlay.point_unavailable.is_some() {
                C::OperationNotAllowed
            } else {
                C::OpenHand
            })
        } else {
            standard(C::Crosshair)
        };
    }
    if overlay.point_unavailable.is_some() {
        return standard(C::OperationNotAllowed);
    }
    if let Some(tool) = input.manual_tool {
        let cursor = manual_cursor(tool, overlay.manual_mesh_feedback, input.modifiers);
        return Some(
            if cursor == T::MeshDelete && overlay.manual_mesh_delete_unavailable {
                T::Standard(C::OperationNotAllowed)
            } else {
                cursor
            },
        );
    }
    if let Some(handle) = overlay.active_handle {
        return match handle {
            DeformerHandle::Rotate => Some(T::Rotate),
            DeformerHandle::RotationScale if input.modifiers.alt => {
                Some(T::BoneLength(handle_direction(overlay)))
            }
            DeformerHandle::RotationScale | DeformerHandle::Scale(_) => {
                standard(resize_style(handle_direction(overlay)))
            }
            DeformerHandle::Move => standard(C::OpenHand),
        };
    }
    if overlay.active_component.is_some() {
        return standard(
            if overlay
                .glues
                .iter()
                .flat_map(|g| &g.points)
                .any(|p| p.hovered && p.state == zipzi_editor_core::GluePointState::Editable)
            {
                C::OpenHand
            } else {
                C::Arrow
            },
        );
    }
    (input.tool == ToolId::Lasso).then_some(T::Standard(C::Crosshair))
}

fn manual_cursor(
    tool: ManualTool,
    feedback: Option<ManualMeshFeedback>,
    modifiers: Modifiers,
) -> ToolCursor {
    use CursorStyle as C;
    use ManualMeshFeedback as F;
    use ToolCursor as T;
    let Some(feedback) = feedback else {
        return T::Standard(C::Arrow);
    };
    if modifiers.platform && !modifiers.shift {
        return match feedback {
            F::Vertex => T::MeshConnect { new_vertex: false },
            F::AutomaticEdge => T::MeshFixEdge,
            _ => T::Standard(C::OperationNotAllowed),
        };
    }
    let tool = if modifiers.platform && modifiers.shift {
        ManualTool::Delete
    } else {
        tool
    };
    match tool {
        ManualTool::Delete if feedback == F::Empty => T::Standard(C::OperationNotAllowed),
        ManualTool::Delete => T::MeshDelete,
        ManualTool::Create if feedback == F::Vertex => T::MeshConnect { new_vertex: false },
        ManualTool::Create => T::MeshAdd,
        _ if feedback == F::Vertex => T::Standard(C::OpenHand),
        _ if !modifiers.shift && !modifiers.alt => T::MeshAdd,
        _ => T::Standard(C::Crosshair),
    }
}

fn handle_direction(overlay: &OverlaySnapshot) -> u8 {
    let base = match overlay.active_handle {
        Some(DeformerHandle::Scale(index)) => [1., 2., 3., 0.][index as usize % 4],
        _ => 0.,
    };
    (base + overlay.active_handle_radians / std::f32::consts::FRAC_PI_4)
        .round()
        .rem_euclid(4.) as u8
}

fn resize_style(direction: u8) -> CursorStyle {
    match direction % 4 {
        0 => CursorStyle::ResizeLeftRight,
        1 => CursorStyle::ResizeUpLeftDownRight,
        2 => CursorStyle::ResizeUpDown,
        _ => CursorStyle::ResizeUpRightDownLeft,
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ToolCursors {
    pub(crate) mesh_add: CursorStyle,
    mesh_delete: CursorStyle,
    mesh_connect: CursorStyle,
    mesh_connect_new: CursorStyle,
    mesh_fix: CursorStyle,
    rotate: CursorStyle,
    bone_length: [CursorStyle; 4],
}
impl Global for ToolCursors {}

impl ToolCursors {
    pub(crate) fn style(&self, cursor: ToolCursor) -> CursorStyle {
        match cursor {
            ToolCursor::Standard(style) => style,
            ToolCursor::MeshAdd => self.mesh_add,
            ToolCursor::MeshDelete => self.mesh_delete,
            ToolCursor::MeshConnect { new_vertex: false } => self.mesh_connect,
            ToolCursor::MeshConnect { new_vertex: true } => self.mesh_connect_new,
            ToolCursor::MeshFixEdge => self.mesh_fix,
            ToolCursor::Rotate => self.rotate,
            ToolCursor::BoneLength(direction) => self.bone_length[direction as usize % 4],
        }
    }

    pub(crate) fn get_or_init(cx: &mut App) -> Self {
        if let Some(cursors) = cx.try_global::<Self>() {
            return *cursors;
        }
        let register = |svg: &[u8], fallback| {
            CustomCursor::from_svg(svg, size(24, 24), point(7, 7), fallback)
                .map(|cursor| cx.register_custom_cursor(cursor))
                .unwrap_or(fallback)
        };
        let length_images = [
            include_bytes!("../assets/cursors/bone-length-0.svg").as_slice(),
            include_bytes!("../assets/cursors/bone-length-45.svg").as_slice(),
            include_bytes!("../assets/cursors/bone-length-90.svg").as_slice(),
            include_bytes!("../assets/cursors/bone-length-135.svg").as_slice(),
        ];
        let cursors = Self {
            mesh_add: register(
                include_bytes!("../assets/cursors/mesh-add.svg"),
                CursorStyle::Crosshair,
            ),
            mesh_delete: register(
                include_bytes!("../assets/cursors/mesh-delete.svg"),
                CursorStyle::Arrow,
            ),
            mesh_connect: register(
                include_bytes!("../assets/cursors/mesh-connect.svg"),
                CursorStyle::PointingHand,
            ),
            mesh_connect_new: register(
                include_bytes!("../assets/cursors/mesh-connect-new.svg"),
                CursorStyle::PointingHand,
            ),
            mesh_fix: register(
                include_bytes!("../assets/cursors/mesh-fix.svg"),
                CursorStyle::PointingHand,
            ),
            rotate: register(
                include_bytes!("../assets/cursors/rotate.svg"),
                CursorStyle::OpenHand,
            ),
            bone_length: std::array::from_fn(|index| {
                register(length_images[index], resize_style(index as u8))
            }),
        };
        cx.set_global(cursors);
        cursors
    }
}
