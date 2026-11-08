//! Register artwork once per application; viewport requests only copy a style ID.
use gpui_kit::{App, CursorStyle, CustomCursor, Global, point, size};

#[derive(Clone, Copy)]
pub(crate) struct ToolCursors {
    pub(crate) mesh_add: CursorStyle,
}
impl Global for ToolCursors {}

impl ToolCursors {
    pub(crate) fn get_or_init(cx: &mut App) -> Self {
        if let Some(cursors) = cx.try_global::<Self>() {
            return *cursors;
        }
        let mesh_add = CustomCursor::from_svg(
            include_bytes!("../assets/cursors/mesh-add.svg"),
            size(24, 24),
            point(7, 7),
            CursorStyle::Crosshair,
        )
        .map(|cursor| cx.register_custom_cursor(cursor))
        .unwrap_or(CursorStyle::Crosshair);
        let cursors = Self { mesh_add };
        cx.set_global(cursors);
        cursors
    }
}
