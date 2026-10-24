//! Attached toolbar geometry. Insets and button radii also determine the SVG
//! silhouette, so selected buttons and the outer corners remain concentric.
use crate::ui::compact;
use gpui_kit::{prelude::*, *};

#[derive(Clone, Copy, Debug)]
enum Edge {
    Left,
    Top,
}

#[derive(Clone, Copy)]
pub(in crate::viewports) struct Notch {
    edge: Edge,
    pub button_size: f32,
    pub button_radius: f32,
    /// Space between the button and the straight body edges.
    pub inset: f32,
    /// Concave join to the viewport; independent of the outer corner radius.
    shoulder: f32,
}

pub(in crate::viewports) const PRIMARY: Notch = Notch {
    edge: Edge::Left,
    button_size: 24.,
    button_radius: 5.,
    inset: 2.,
    shoulder: 4.,
};

pub(in crate::viewports) const SECONDARY: Notch = Notch {
    edge: Edge::Top,
    button_size: 20.,
    button_radius: 4.,
    inset: 2.,
    shoulder: 4.,
};

impl Notch {
    pub fn end_padding(self) -> f32 {
        self.shoulder + self.inset
    }

    /// Render a single SVG alpha mask, avoiding seams or doubled opacity where
    /// independently rasterized caps meet a rectangular middle at fractional scale.
    pub fn background(self, cx: &App) -> impl IntoElement {
        let color = crate::theme::toolbar_colors(cx).surface.opacity(0.85);
        let scale = compact::scale(cx);
        canvas(
            |_, _, _| (),
            move |bounds, _, window, cx| {
                let width = bounds.size.width.as_f32() / scale;
                let height = bounds.size.height.as_f32() / scale;
                let (depth, length, transform) = match self.edge {
                    Edge::Left => (width, height, ""),
                    Edge::Top => (height, width, " transform=\"matrix(0 1 1 0 0 0)\""),
                };
                let shoulder = self.shoulder;
                let radius = self.button_radius + self.inset;
                let outer_center = depth - radius;
                let start = shoulder + radius;
                let end = length - start;
                let bottom = length - shoulder;
                let svg = format!(
                    "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\">\
                     <path fill=\"white\"{transform} d=\"M0 0\
                     A{shoulder} {shoulder} 0 0 0 {shoulder} {shoulder}\
                     H{outer_center}A{radius} {radius} 0 0 1 {depth} {start}\
                     V{end}A{radius} {radius} 0 0 1 {outer_center} {bottom}\
                     H{shoulder}A{shoulder} {shoulder} 0 0 0 0 {length}Z\"/></svg>"
                );
                // The atlas caches the mask by geometry and actual display size;
                // colors and toolbar state changes reuse the same rasterization.
                let key = format!(
                    "zipzi-notch-{:?}-{width}-{height}-{radius}-{shoulder}", self.edge
                );
                window.paint_svg(bounds, key.into(), Some(svg.as_bytes()), Default::default(), color, cx)
                    .expect("valid viewport notch SVG");
            },
        )
        .absolute()
        .inset_0()
    }
}
