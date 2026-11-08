//! Resolve canvas operations centrally and map them to native cursor resources.
//! Register artwork once per application; viewport requests only copy a style ID.
use gpui_kit::{App, CursorStyle, CustomCursor, Global, Modifiers, point, size};
use zipzi_editor_core::{
    CanvasGestureKind, CanvasSelectionOperation, ControlKind, DeformerHandle, ManualMeshFeedback,
    ManualTool, OverlaySnapshot, ToolId,
};

#[cfg(test)]
mod tests;

const ROTATION_DIRECTIONS: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ToolCursor {
    Standard(CursorStyle),
    MeshAdd,
    MeshDelete,
    MeshConnect { new_vertex: bool },
    MeshFixEdge,
    Rotate(u8),
    BoneLength(u8),
    Move,
    Lasso(CanvasSelectionOperation),
    Brush(CanvasSelectionOperation),
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
                Some(
                    ToolCursor::Move
                        | ToolCursor::Standard(CursorStyle::OpenHand | CursorStyle::ClosedHand)
                )
            ),
        cursor,
    }
}

fn resolve_cursor(input: &CursorInput<'_>) -> Option<ToolCursor> {
    use CanvasGestureKind as G;
    use CursorStyle as C;
    use ToolCursor as T;
    let overlay = input.overlay;
    let operation = overlay
        .selection_operation
        .unwrap_or(CanvasSelectionOperation::Replace);
    let selection_only = matches!(
        overlay.selection_operation,
        Some(CanvasSelectionOperation::Remove | CanvasSelectionOperation::Toggle)
    );
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
        if !selecting
            && !(gesture == G::Move && selection_only)
            && overlay.point_unavailable.is_some()
        {
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
            G::Rotate => Some(T::Rotate(rotation_direction(overlay))),
            G::BoneLength => Some(T::BoneLength(handle_direction(overlay))),
            G::Scale => standard(resize_style(handle_direction(overlay))),
            G::Move if selection_only => standard(C::Arrow),
            G::Move => Some(T::Move),
            G::Unavailable => standard(C::OperationNotAllowed),
            G::Cancelled => standard(C::Arrow),
            G::BoxSelect if input.manual_tool.is_none() => standard(C::Arrow),
            G::LassoSelect => Some(T::Lasso(operation)),
            G::BrushSelect => Some(T::Brush(operation)),
            _ => standard(C::Crosshair),
        };
    }
    if input.hand_tool {
        return standard(C::OpenHand);
    }
    // Painting a selection is available even when the hovered point cannot be edited.
    if input.tool == ToolId::BrushSelect && input.manual_tool.is_none() {
        return if input.window_active && input.modifiers.platform {
            if overlay.point_unavailable.is_some() {
                standard(C::OperationNotAllowed)
            } else {
                Some(T::Move)
            }
        } else {
            Some(T::Brush(operation))
        };
    }
    if overlay.point_unavailable.is_some()
        && !(overlay.active_component.is_some() && selection_only)
    {
        return standard(C::OperationNotAllowed);
    }
    if let Some(tool) = input.manual_tool {
        let cursor = manual_cursor(
            tool,
            overlay.manual_mesh_feedback,
            overlay.selection_operation,
            input.modifiers,
        );
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
            DeformerHandle::Rotate => Some(T::Rotate(rotation_direction(overlay))),
            DeformerHandle::RotationScale if input.modifiers.alt => {
                Some(T::BoneLength(handle_direction(overlay)))
            }
            DeformerHandle::RotationScale | DeformerHandle::Scale(_) => {
                standard(resize_style(handle_direction(overlay)))
            }
            DeformerHandle::Move => Some(T::Move),
        };
    }
    if overlay.active_component.is_some() {
        return if selection_only {
            standard(C::Arrow)
        } else {
            Some(T::Move)
        };
    }
    (input.tool == ToolId::Lasso).then_some(T::Lasso(operation))
}

fn manual_cursor(
    tool: ManualTool,
    feedback: Option<ManualMeshFeedback>,
    operation: Option<CanvasSelectionOperation>,
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
        _ if feedback == F::Vertex
            && matches!(
                operation,
                Some(CanvasSelectionOperation::Remove | CanvasSelectionOperation::Toggle)
            ) =>
        {
            T::Standard(C::Arrow)
        }
        _ if feedback == F::Vertex => T::Move,
        _ if !modifiers.shift && !modifiers.alt => T::MeshAdd,
        ManualTool::Lasso => T::Lasso(operation.unwrap_or(CanvasSelectionOperation::Replace)),
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

fn rotation_direction(overlay: &OverlaySnapshot) -> u8 {
    let mut radians = if overlay.active_handle_radians.is_finite() {
        overlay.active_handle_radians
    } else {
        0.
    };
    // The artwork's outward normal is -Y, matching a frame's top rotation handle.
    // A Rotation gizmo reports its +X bone axis, so align the normal clockwise.
    if overlay.active_control_kind == Some(ControlKind::Rotation) {
        radians += std::f32::consts::FRAC_PI_2;
    }
    (radians.rem_euclid(std::f32::consts::TAU) * ROTATION_DIRECTIONS as f32 / std::f32::consts::TAU)
        .round()
        .rem_euclid(ROTATION_DIRECTIONS as f32) as u8
}

/// Build the finite direction set once; rotating a gesture only selects its ID.
fn rotation_artwork(direction: usize) -> anyhow::Result<CustomCursor> {
    let degrees = (direction % ROTATION_DIRECTIONS) as f32 * 360. / ROTATION_DIRECTIONS as f32;
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24"><g transform="rotate({degrees} 12 12)">{}</g></svg>"#,
        include_str!("../assets/cursors/rotate.svg")
    );
    CustomCursor::from_svg(
        svg.as_bytes(),
        size(24, 24),
        point(12, 12),
        CursorStyle::OpenHand,
    )
}

fn resize_style(direction: u8) -> CursorStyle {
    match direction % 4 {
        0 => CursorStyle::ResizeLeftRight,
        1 => CursorStyle::ResizeUpLeftDownRight,
        2 => CursorStyle::ResizeUpDown,
        _ => CursorStyle::ResizeUpRightDownLeft,
    }
}

fn selection_index(operation: CanvasSelectionOperation) -> usize {
    match operation {
        CanvasSelectionOperation::Replace => 0,
        CanvasSelectionOperation::Add => 1,
        CanvasSelectionOperation::Remove => 2,
        CanvasSelectionOperation::Toggle => 3,
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ToolCursors {
    pub(crate) mesh_add: CursorStyle,
    mesh_delete: CursorStyle,
    mesh_connect: CursorStyle,
    mesh_connect_new: CursorStyle,
    mesh_fix: CursorStyle,
    rotate: [CursorStyle; ROTATION_DIRECTIONS],
    bone_length: [CursorStyle; 4],
    lasso: [CursorStyle; 4],
    brush: [CursorStyle; 3],
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
            ToolCursor::Rotate(direction) => self.rotate[direction as usize % ROTATION_DIRECTIONS],
            ToolCursor::BoneLength(direction) => self.bone_length[direction as usize % 4],
            ToolCursor::Move => CursorStyle::Arrow,
            ToolCursor::Lasso(operation) => self.lasso[selection_index(operation)],
            ToolCursor::Brush(operation) => self
                .brush
                .get(selection_index(operation))
                .copied()
                .unwrap_or(CursorStyle::Crosshair),
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
        let lasso_images = [
            include_bytes!("../assets/cursors/lasso.svg").as_slice(),
            include_bytes!("../assets/cursors/lasso-add.svg").as_slice(),
            include_bytes!("../assets/cursors/lasso-remove.svg").as_slice(),
            include_bytes!("../assets/cursors/lasso-toggle.svg").as_slice(),
        ];
        let brush_images = [
            include_bytes!("../assets/cursors/brush-select.svg").as_slice(),
            include_bytes!("../assets/cursors/brush-add.svg").as_slice(),
            include_bytes!("../assets/cursors/brush-remove.svg").as_slice(),
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
            rotate: std::array::from_fn(|direction| {
                rotation_artwork(direction)
                    .map(|cursor| cx.register_custom_cursor(cursor))
                    .unwrap_or(CursorStyle::OpenHand)
            }),
            bone_length: std::array::from_fn(|index| {
                register(length_images[index], resize_style(index as u8))
            }),
            lasso: lasso_images.map(|svg| register(svg, CursorStyle::Crosshair)),
            brush: brush_images.map(|svg| register(svg, CursorStyle::Crosshair)),
        };
        cx.set_global(cursors);
        cursors
    }
}
