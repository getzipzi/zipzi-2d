use super::*;
use std::{cell::RefCell, rc::Rc};

#[derive(Default)]
pub(super) struct DrawingState {
    hits: Rc<RefCell<Vec<Point<Pixels>>>>,
}

/// Center the live chain at paint time, independent of its rest structure.
struct Fit {
    center: [f64; 2],
    scale: f64,
    origin: Point<Pixels>,
}
impl Fit {
    fn new(points: impl Iterator<Item = [f32; 2]>, bounds: Bounds<Pixels>, ui: f32) -> Self {
        let mut min = [f64::INFINITY; 2];
        let mut max = [f64::NEG_INFINITY; 2];
        for point in points.filter(|p| p.iter().all(|v| v.is_finite())) {
            for axis in 0..2 {
                min[axis] = min[axis].min(point[axis] as f64);
                max[axis] = max[axis].max(point[axis] as f64);
            }
        }
        if !min[0].is_finite() {
            min = [0.; 2];
            max = [0.; 2];
        }
        let width = (bounds.size.width.as_f32() - 64. * ui).max(1.) as f64;
        let height = (bounds.size.height.as_f32() - 64. * ui).max(1.) as f64;
        let span = [(max[0] - min[0]).max(1e-6), (max[1] - min[1]).max(1e-6)];
        Self {
            center: [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5],
            scale: (width / span[0]).min(height / span[1]),
            origin: bounds.center(),
        }
    }
    fn project(&self, point: [f32; 2]) -> Point<Pixels> {
        self.origin
            + gpui_kit::point(
                px(((point[0] as f64 - self.center[0]) * self.scale) as f32),
                px(((point[1] as f64 - self.center[1]) * self.scale) as f32),
            )
    }
}
impl PropertiesPanel {
    pub(super) fn physics_chain(
        &mut self,
        rig: &PhysicsSetting,
        trace: Option<&kasane_sdk::PhysicsRigSnapshot>,
        manual: bool,
        disabled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut y = 0.;
        let reference = rig
            .vertices
            .iter()
            .enumerate()
            .map(|(i, v)| {
                if i > 0 {
                    y += v.radius;
                }
                [0., y]
            })
            .collect::<Vec<_>>();
        let positions = trace
            .filter(|r| r.particles.len() == reference.len())
            .map_or_else(|| reference.clone(), |r| r.particles.clone());
        let selected = self.physics.node;
        let hits = self.physics.drawing.hits.clone();
        let color = cx.theme().primary;
        let border = cx.theme().border;
        let foreground = cx.theme().foreground;
        let muted = cx.theme().muted_foreground;
        let surface = cx.theme().muted;
        let selected_fill = cx.theme().primary;
        let scale_ui = compact::scale(cx);
        let epoch = self.physics.epoch;
        let mut drawing = div()
            .id("physics-chain-viewport")
            .test_support()
            .size_full()
            .flex_shrink_0()
            .relative()
            .overflow_hidden()
            .bg(surface)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if !this.physics_allowed(epoch, window, cx) || disabled {
                        return;
                    }
                    let radius = 12. * compact::scale(cx);
                    let hit = this
                        .physics
                        .drawing
                        .hits
                        .borrow()
                        .iter()
                        .enumerate()
                        .map(|(i, p)| {
                            (
                                i,
                                ((p.x - event.position.x).as_f32())
                                    .hypot((p.y - event.position.y).as_f32()),
                            )
                        })
                        .filter(|(_, d)| *d <= radius)
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .map(|(i, _)| i);
                    if let Some(index) = hit {
                        this.physics_cancel(cx);
                        this.physics.node = index;
                        if let Some(asset) = this.physics_asset(cx) {
                            if let Some(rig) = asset
                                .data
                                .settings
                                .iter()
                                .find(|r| r.id == this.physics.group)
                            {
                                if let Some(output) =
                                    rig.outputs.iter().position(|o| o.vertex_index == index)
                                {
                                    this.physics.output = output;
                                }
                            }
                        }
                        window.focus(&this.focus, cx);
                        cx.notify();
                        cx.stop_propagation();
                    }
                }),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, cx| {
                        let fit = Fit::new(positions.iter().copied(), bounds, scale_ui);
                        let origin = fit.project([0., 0.]);
                        let project = |p: [f32; 2]| fit.project(p);
                        let points = positions.iter().copied().map(project).collect::<Vec<_>>();
                        hits.replace(points.clone());
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            line(
                                window,
                                point(origin.x, bounds.top()),
                                point(origin.x, bounds.bottom()),
                                border,
                                1.,
                            );
                            line(
                                window,
                                point(bounds.left(), origin.y),
                                point(bounds.right(), origin.y),
                                border,
                                1.,
                            );
                            for pair in reference.windows(2) {
                                line(
                                    window,
                                    project(pair[0]),
                                    project(pair[1]),
                                    muted.opacity(0.3),
                                    1.,
                                );
                            }
                            for pair in points.windows(2) {
                                line(
                                    window,
                                    pair[0],
                                    pair[1],
                                    foreground.opacity(0.65),
                                    1.5 * scale_ui,
                                );
                            }
                            for (i, p) in points.iter().copied().enumerate() {
                                let radius = if i == selected {
                                    5.
                                } else if i == 0 {
                                    4.
                                } else {
                                    3.5
                                } * scale_ui;
                                window.paint_quad(
                                    fill(
                                        Bounds::new(
                                            p - point(px(radius), px(radius)),
                                            size(px(radius * 2.), px(radius * 2.)),
                                        ),
                                        if i == selected {
                                            selected_fill
                                        } else if i == 0 {
                                            foreground
                                        } else {
                                            surface
                                        },
                                    )
                                    .corner_radii(px(radius))
                                    .border_widths(px(1.))
                                    .border_color(if i == selected { color } else { foreground }),
                                );
                                text(
                                    window,
                                    cx,
                                    p + point(px(9. * scale_ui), px(-7. * scale_ui)),
                                    if i == 0 {
                                        t!("physics.root").into_owned()
                                    } else {
                                        i.to_string()
                                    },
                                    if i == selected { color } else { muted },
                                );
                            }
                        });
                    },
                )
                .size_full(),
            );
        if manual {
            drawing = drawing.child(
                div()
                    .flex()
                    .absolute()
                    .right(compact::rem(3.))
                    .top(compact::rem(3.))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(
                        compact::header_button(
                            Button::new("physics-stabilize").label(t!("physics.stabilize")),
                        )
                        .disabled(disabled)
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.physics_session(window, cx, |editor, id| {
                                editor.stabilize_physics_preview(id)
                            })
                        })),
                    ),
            );
        }
        drawing
            .aria_label(if trace.is_some() {
                t!("physics.live_chain").into_owned()
            } else {
                t!("physics.static_chain").into_owned()
            })
            .into_any_element()
    }
}
fn line(window: &mut Window, a: Point<Pixels>, b: Point<Pixels>, color: Hsla, width: f32) {
    let mut path = PathBuilder::stroke(px(width));
    path.move_to(a);
    path.line_to(b);
    if let Ok(path) = path.build() {
        window.paint_path(path, color);
    }
}
fn text(window: &mut Window, cx: &mut App, origin: Point<Pixels>, value: String, color: Hsla) {
    let style = window.text_style();
    let run = TextRun {
        len: value.len(),
        font: style.font(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let shaped = window.text_system().shape_line(
        value.into(),
        compact::scaled(compact::CAPTION, cx),
        &[run],
        None,
    );
    let _ = shaped.paint(
        origin,
        compact::scaled(14., cx),
        TextAlign::Left,
        None,
        window,
        cx,
    );
}

#[cfg(test)]
mod tests {
    use super::Fit;
    use gpui_kit::{Bounds, point, px, size};

    #[test]
    fn auto_fit_keeps_bent_and_changed_chains_inside_resized_viewports() {
        for chain in [
            vec![[0., 0.], [0., 10.]],
            vec![[0., 0.], [-15., -10.], [60., 5.], [0., 120.]],
            vec![[0., 0.]; 3],
        ] {
            for viewport in [
                size(px(180.), px(72.)),
                size(px(360.), px(144.)),
                size(px(600.), px(240.)),
            ] {
                let bounds = Bounds::new(point(px(100.), px(200.)), viewport);
                let fit = Fit::new(chain.iter().copied(), bounds, 1.);
                let projected = chain.iter().map(|p| fit.project(*p)).collect::<Vec<_>>();
                for axis in [0, 1] {
                    let value = |p: gpui_kit::Point<gpui_kit::Pixels>| {
                        if axis == 0 {
                            p.x.as_f32()
                        } else {
                            p.y.as_f32()
                        }
                    };
                    let min = projected
                        .iter()
                        .copied()
                        .map(value)
                        .fold(f32::INFINITY, f32::min);
                    let max = projected
                        .iter()
                        .copied()
                        .map(value)
                        .fold(f32::NEG_INFINITY, f32::max);
                    assert!(((min + max) * 0.5 - value(bounds.center())).abs() < 0.01);
                }
                for position in projected {
                    assert!(position.x.as_f32().is_finite() && position.y.as_f32().is_finite());
                    assert!(
                        position.x >= bounds.left() + px(31.)
                            && position.x <= bounds.right() - px(31.)
                    );
                    assert!(
                        position.y >= bounds.top() + px(31.)
                            && position.y <= bounds.bottom() - px(31.)
                    );
                }
            }
        }
    }
}
