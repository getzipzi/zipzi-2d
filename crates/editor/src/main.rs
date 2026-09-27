use editor_scene::{Document, SceneRenderer, TargetConfig};
use gpui::{prelude::*, *};
use gpui_wgpu::{WgpuContextHandle, WgpuRenderTarget};

struct GpuCanvas {
    context: WgpuContextHandle,
    target: WgpuRenderTarget,
    scene: SceneRenderer,
}
struct Editor {
    document: Document,
    gpu: Option<GpuCanvas>,
    error: Option<String>,
}
impl Editor {
    fn draw_canvas(&mut self, window: &Window) {
        let result = (|| -> anyhow::Result<()> {
            let context = WgpuContextHandle::from_window(window)
                .ok_or_else(|| anyhow::anyhow!("Window has no wgpu context"))?;
            anyhow::ensure!(
                !context.device_lost(),
                "wgpu device lost; restart the experiment"
            );
            let scale = window.scale_factor();
            let viewport = window.viewport_size();
            let extent = size(
                DevicePixels(((f32::from(viewport.width) - 400.0).max(1.0) * scale).round() as i32),
                DevicePixels(
                    ((f32::from(viewport.height) - 108.0).max(1.0) * scale).round() as i32,
                ),
            );
            if self
                .gpu
                .as_ref()
                .is_some_and(|gpu| !context.is_same_device(&gpu.context))
            {
                self.gpu = None;
            }
            if self.gpu.is_none() {
                let target = WgpuRenderTarget::with_format(
                    &context,
                    extent,
                    wgpu::TextureFormat::Rgba8Unorm,
                );
                let scene = SceneRenderer::new(
                    context.device(),
                    context.queue(),
                    TargetConfig {
                        width: extent.width.0 as u32,
                        height: extent.height.0 as u32,
                        format: target.format(),
                    },
                )?;
                eprintln!(
                    "Kasane editor: shared wgpu device: {:?}",
                    context.adapter_info()
                );
                self.gpu = Some(GpuCanvas {
                    context,
                    target,
                    scene,
                });
            }
            let gpu = self.gpu.as_mut().unwrap();
            gpu.target.resize(&gpu.context, extent);
            gpu.scene.draw(
                gpu.context.device(),
                gpu.context.queue(),
                gpu.target.view(),
                TargetConfig {
                    width: extent.width.0 as u32,
                    height: extent.height.0 as u32,
                    format: gpu.target.format(),
                },
                self.document,
            )
        })();
        self.error = result.err().map(|e| e.to_string());
    }
    fn button(
        &self,
        label: &'static str,
        cx: &Context<Self>,
        action: fn(&mut Document),
    ) -> impl IntoElement {
        ui::button(label, label).on_click(cx.listener(move |this, _, _, cx| {
            action(&mut this.document);
            cx.notify();
        }))
    }
}
impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.draw_canvas(window);
        let mut canvas = div().flex_1().h_full().min_w_0().bg(rgb(0x111722));
        if let Some(error) = &self.error {
            canvas = canvas.child(error.clone());
        } else if let Some(gpu) = &self.gpu {
            canvas = canvas.child(gpu.target.surface().size_full());
        }
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(0x1b2230))
            .text_color(rgb(0xe3e9f2))
            .text_sm()
            .child(
                div()
                    .h(px(64.0))
                    .flex()
                    .items_center()
                    .px_4()
                    .gap_3()
                    .child("KASANE / Assembly Lab")
                    .child(self.button("−", cx, |d| d.zoom = (d.zoom - 0.1).max(0.2)))
                    .child(format!("{:.0}%", self.document.zoom * 100.0))
                    .child(self.button("+", cx, |d| d.zoom = (d.zoom + 0.1).min(3.0)))
                    .child(self.button("Reset", cx, |d| *d = Document::default())),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .w(px(180.0))
                            .h_full()
                            .p_4()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child("SCENE")
                            .child(self.button("Diamond", cx, |d| d.visible = !d.visible))
                            .child(if self.document.visible {
                                "Visible"
                            } else {
                                "Hidden"
                            }),
                    )
                    .child(canvas)
                    .child(
                        div()
                            .w(px(220.0))
                            .h_full()
                            .p_4()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child("INSPECTOR")
                            .child("Mesh / 4 vertices / 2 triangles")
                            .child(format!("Opacity: {:.0}%", self.document.opacity * 100.0))
                            .child(self.button("Opacity −", cx, |d| {
                                d.opacity = (d.opacity - 0.1).max(0.0)
                            }))
                            .child(self.button("Opacity +", cx, |d| {
                                d.opacity = (d.opacity + 0.1).min(1.0)
                            })),
                    ),
            )
            .child(
                div()
                    .h(px(44.0))
                    .px_4()
                    .flex()
                    .items_center()
                    .child("gpui-ce + kasane-2d  •  wgpu 29 / shared device / direct texture"),
            )
    }
}
fn main() {
    gpui_platform::application().run(|cx: &mut App| {
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();
        cx.open_window(
            WindowOptions {
                titlebar: Some(TitlebarOptions {
                    title: Some("Kasane Editor — wgpu experiment".into()),
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                    None,
                    size(px(1100.0), px(740.0)),
                    cx,
                ))),
                window_min_size: Some(size(px(720.0), px(480.0))),
                ..Default::default()
            },
            |_, cx| {
                cx.new(|_| Editor {
                    document: Document::default(),
                    gpu: None,
                    error: None,
                })
            },
        )
        .expect("open editor window");
        cx.activate(true);
    });
}
