//! Logical-pixel geometry shared by viewport painting and picking.
pub const POINT_RADIUS: f32 = 3.;
/// Secondary Warp lattice controls stay smaller than primary editing handles.
pub const WARP_GRID_POINT_RADIUS: f32 = 2.5;
pub const POINT_HIT_RADIUS: f32 = 6.;
pub const LINE_HIT_RADIUS: f32 = 3.;
pub const FRAME_PADDING: f32 = 6.;
pub const IDLE_LINE_WIDTH: f32 = 0.5;
pub const ACTIVE_LINE_WIDTH: f32 = 1.;
pub const WARP_OUTLINE_OPACITY: f32 = 0.1;
pub const ROTATION_IDLE_OPACITY: f32 = 0.2;
/// Hover changes emphasis without changing the silhouette.
pub const POINT_HOVER_COLOR: [f32; 4] = [98. / 255., 200. / 255., 120. / 255., 1.];

pub const fn line_width(selected: bool) -> f32 {
    if selected {
        ACTIVE_LINE_WIDTH
    } else {
        IDLE_LINE_WIDTH
    }
}
