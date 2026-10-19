//! Screen-space backdrop, composited beneath the transparent model texture.
use gpui_kit::{prelude::*, *};

const TILE_SIZE: f32 = 12.;

pub(super) fn transparency_grid(cx: &App) -> impl IntoElement {
    let (base, alternate) = crate::theme::transparency_colors(cx);
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            window.paint_quad(fill(bounds, base));
            // Paint directly instead of creating one UI element per square.
            // Coordinates stay viewport-local and independent of camera/DPI.
            let columns = (bounds.size.width.as_f32() / TILE_SIZE).ceil() as usize;
            let rows = (bounds.size.height.as_f32() / TILE_SIZE).ceil() as usize;
            for row in 0..rows {
                for column in ((row + 1) % 2..columns).step_by(2) {
                    let tile = Bounds::new(
                        bounds.origin
                            + point(px(column as f32 * TILE_SIZE), px(row as f32 * TILE_SIZE)),
                        size(px(TILE_SIZE), px(TILE_SIZE)),
                    );
                    window.paint_quad(fill(tile.intersect(&bounds), alternate));
                }
            }
        },
    )
    .absolute()
    .size_full()
}
