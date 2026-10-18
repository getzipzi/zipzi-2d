//! Reusable image/line viewport. Content is normalized to [0, 1] in both axes;
//! source pixel dimensions restore its proportions. Camera state is independent of content.
use crate::t;
use editor_core::{MAX_ZOOM, MIN_ZOOM, ViewportPoint};
use gpui_kit::component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::*, *};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::Arc,
};

pub(crate) type PreviewPainter =
    Rc<RefCell<dyn FnMut(Bounds<Pixels>, Bounds<Pixels>, &mut Window) -> anyhow::Result<()>>>;
#[derive(Clone, Debug)]
pub(crate) struct PreviewPaintStatus(pub u64, pub Result<(), String>);
impl EventEmitter<PreviewPaintStatus> for MeshGenerationPreview {}

#[derive(Clone)]
pub(crate) struct PreviewContent {
    pub source_size: [f32; 2],
    pub painter: Option<PreviewPainter>,
    pub request: u64,
    pub edges: Arc<[([f32; 2], [f32; 2])]>,
}

pub(crate) struct MeshGenerationPreview {
    content: Option<PreviewContent>,
    painted: Rc<Cell<Option<(u64, bool)>>>,
    message: SharedString,
    zoom: f32,
    pan: Point<Pixels>,
    drag: Option<Point<Pixels>>,
    wireframe: bool,
    bounds: Rc<Cell<Bounds<Pixels>>>,
}

impl MeshGenerationPreview {
    pub fn new(_cx: &mut Context<Self>) -> Self {
        Self {
            content: None,
            painted: Rc::new(Cell::new(None)),
            message: "".into(),
            zoom: 1.,
            pan: point(px(0.), px(0.)),
            drag: None,
            wireframe: true,
            bounds: Rc::new(Cell::new(Bounds::default())),
        }
    }

    /// Replacing a candidate preserves the user's inspection position.
    pub fn set_content(
        &mut self,
        content: Option<PreviewContent>,
        message: SharedString,
        cx: &mut Context<Self>,
    ) {
        self.content = content.filter(|c| {
            c.source_size
                .iter()
                .all(|dimension| dimension.is_finite() && *dimension > 0.)
        });
        self.message = message;
        cx.notify();
    }

    fn fit(&mut self, _window: &Window, cx: &mut Context<Self>) {
        let Some(content) = &self.content else { return };
        let bounds = self.bounds.get();
        self.zoom = (((bounds.size.width.as_f32() - 32.).max(1.) / content.source_size[0])
            .min((bounds.size.height.as_f32() - 32.).max(1.) / content.source_size[1]))
        .clamp(MIN_ZOOM, MAX_ZOOM);
        self.pan = point(px(0.), px(0.));
        self.drag = None;
        cx.notify();
    }

    fn zoom_at(
        &mut self,
        factor: f32,
        anchor: Point<Pixels>,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        if !factor.is_finite() || factor <= 0. {
            return;
        }
        let bounds = self.bounds.get();
        let local = anchor - bounds.origin;
        let mut view = editor_core::ViewState {
            zoom: self.zoom,
            pan: ViewportPoint {
                x: self.pan.x.as_f32(),
                y: self.pan.y.as_f32(),
            },
        };
        view.zoom_about(
            self.zoom * factor,
            Some(ViewportPoint {
                x: local.x.as_f32(),
                y: local.y.as_f32(),
            }),
            editor_core::ViewportMetrics {
                width: bounds.size.width.as_f32(),
                height: bounds.size.height.as_f32(),
                scale_factor: window.scale_factor(),
            },
        );
        self.pan = point(px(view.pan.x), px(view.pan.y));
        self.zoom = view.zoom;
        cx.notify();
    }
}

impl Render for MeshGenerationPreview {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let measured = self.bounds.clone();
        let content = self.content.clone();
        let painted = self.painted.clone();
        let entity = cx.entity().downgrade();
        let zoom = self.zoom;
        let pan = self.pan;
        let wireframe = self.wireframe;
        let color = cx.theme().primary;
        let mut viewport = div()
            .id("preview-viewport")
            .test_support()
            .relative()
            .w_full()
            .flex_1()
            .min_h_0()
            .overflow_hidden()
            .rounded_md()
            .bg(rgb(0x24272b))
            .cursor_grab()
            .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, window, cx| {
                match event.delta {
                    ScrollDelta::Pixels(delta)
                        if !event.modifiers.control && !event.modifiers.platform =>
                    {
                        if delta.x.as_f32().is_finite() && delta.y.as_f32().is_finite() {
                            this.pan += delta;
                            cx.notify();
                        }
                    }
                    delta => {
                        let y = match delta {
                            ScrollDelta::Lines(p) => p.y * 0.12,
                            ScrollDelta::Pixels(p) => p.y.as_f32() * 0.01,
                        };
                        if y.is_finite() {
                            this.zoom_at(y.clamp(-4., 4.).exp(), event.position, window, cx);
                        }
                    }
                }
                cx.stop_propagation();
            }))
            .on_pinch(cx.listener(|this, event: &PinchEvent, window, cx| {
                this.zoom_at(1. + event.delta, event.position, window, cx);
                cx.stop_propagation();
            }))
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if let Some(previous) = this.drag {
                    if matches!(
                        event.pressed_button,
                        Some(MouseButton::Left | MouseButton::Middle)
                    ) {
                        this.pan += event.position - previous;
                        this.drag = Some(event.position);
                        cx.notify();
                        cx.stop_propagation();
                    } else {
                        this.drag = None;
                    }
                }
            }));
        for button in [MouseButton::Left, MouseButton::Middle] {
            viewport = viewport
                .on_mouse_down(
                    button,
                    cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        this.drag = Some(event.position);
                        cx.stop_propagation();
                    }),
                )
                .on_mouse_up(
                    button,
                    cx.listener(|this, _: &MouseUpEvent, _, _| this.drag = None),
                )
                .on_mouse_up_out(
                    button,
                    cx.listener(|this, _: &MouseUpEvent, _, _| this.drag = None),
                );
        }
        let viewport = viewport
            .child(
                canvas(
                    move |bounds, _, _| measured.set(bounds),
                    move |bounds, _, window, cx| {
                        let Some(content) = &content else { return };
                        let scale = zoom;
                        let width = content.source_size[0] * scale;
                        let height = content.source_size[1] * scale;
                        let origin = bounds.center() + pan - point(px(width / 2.), px(height / 2.));
                        let image_bounds = Bounds::new(origin, size(px(width), px(height)));
                        // Clip both the image and line paths, even at large zoom/pan offsets.
                        window.with_content_mask(Some(ContentMask { bounds }), |window| {
                            let visible = image_bounds.intersect(&bounds);
                            let tile = 12.;
                            // Bound checkerboard work by viewport area, not zoomed image area.
                            let x0 = ((visible.left() - origin.x).as_f32() / tile)
                                .floor()
                                .max(0.) as usize;
                            let y0 = ((visible.top() - origin.y).as_f32() / tile).floor().max(0.)
                                as usize;
                            let x1 = ((visible.right() - origin.x).as_f32() / tile)
                                .ceil()
                                .max(0.) as usize;
                            let y1 = ((visible.bottom() - origin.y).as_f32() / tile)
                                .ceil()
                                .max(0.) as usize;
                            if visible.size.width > px(0.) && visible.size.height > px(0.) {
                                for y in y0..y1 {
                                    for x in x0..x1 {
                                        let tile_bounds = Bounds::new(
                                            origin
                                                + point(px(x as f32 * tile), px(y as f32 * tile)),
                                            size(
                                                px(tile.min(width - x as f32 * tile)),
                                                px(tile.min(height - y as f32 * tile)),
                                            ),
                                        );
                                        window.paint_quad(fill(
                                            tile_bounds,
                                            if (x + y) % 2 == 0 {
                                                rgb(0x45494f)
                                            } else {
                                                rgb(0x383c42)
                                            },
                                        ));
                                    }
                                }
                                let result = if let Some(painter) = &content.painter {
                                    (painter.borrow_mut())(bounds, image_bounds, window)
                                        .map_err(|e| e.to_string())
                                } else {
                                    Ok(())
                                };
                                if painted.get() != Some((content.request, result.is_ok())) {
                                    painted.set(Some((content.request, result.is_ok())));
                                    let entity = entity.clone();
                                    let request = content.request;
                                    cx.defer(move |cx| {
                                        _ = entity.update(cx, |_, cx| {
                                            cx.emit(PreviewPaintStatus(request, result))
                                        });
                                    });
                                }
                                if wireframe {
                                    let edges: Vec<_> = content
                                        .edges
                                        .iter()
                                        .map(|(a, b)| {
                                            (
                                                ViewportPoint {
                                                    x: a[0] * width,
                                                    y: a[1] * height,
                                                },
                                                ViewportPoint {
                                                    x: b[0] * width,
                                                    y: b[1] * height,
                                                },
                                            )
                                        })
                                        .collect();
                                    crate::mesh_edges::paint_edges(&edges, origin, |path| {
                                        window.paint_path(path, color)
                                    });
                                }
                            }
                        });
                    },
                )
                .size_full(),
            )
            .when(self.content.is_none(), |v| {
                v.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_sm()
                        .text_color(rgb(0xffffff))
                        .child(self.message.clone()),
                )
            });
        div()
            .size_full()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .flex()
                    .h(px(28.))
                    .flex_shrink_0()
                    .items_center()
                    .gap_1()
                    .child(
                        Button::new("preview-wireframe")
                            .small()
                            .label(t!("preview_viewport.wireframe"))
                            .when(self.wireframe, |b| b.primary())
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.wireframe = !this.wireframe;
                                cx.notify();
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::new("preview-zoom-out")
                            .small()
                            .label("−")
                            .tooltip(t!("preview_viewport.zoom_out"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.zoom_at(1. / 1.25, this.bounds.get().center(), window, cx)
                            })),
                    )
                    .child(
                        div()
                            .id("preview-zoom")
                            .test_support()
                            .text_xs()
                            .w(px(42.))
                            .text_center()
                            .child(format!("{:.0}%", self.zoom * 100.)),
                    )
                    .child(
                        Button::new("preview-zoom-in")
                            .small()
                            .label("+")
                            .tooltip(t!("preview_viewport.zoom_in"))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.zoom_at(1.25, this.bounds.get().center(), window, cx)
                            })),
                    )
                    .child(
                        Button::new("preview-fit")
                            .small()
                            .label(t!("preview_viewport.fit"))
                            .on_click(cx.listener(|this, _, window, cx| this.fit(window, cx))),
                    ),
            )
            .child(viewport)
    }
}
