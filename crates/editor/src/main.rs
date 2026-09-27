mod preview;

use editor_scene::Document;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, Theme, ThemeMode, WindowExt,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::{Input, InputEvent, InputState},
    slider::{Slider, SliderEvent, SliderState, SliderValue},
};
use gpui_kit::{prelude::*, *};
use std::sync::mpsc;

struct Editor {
    document: Document,
    name: Entity<InputState>,
    opacity: Entity<SliderState>,
    frame: Option<preview::Image>,
    error: Option<String>,
    requests: Option<mpsc::Sender<Document>>,
    gpu_device: Option<metal::Device>,
    _frames: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}
impl Editor {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = cx.new(|cx| {
            InputState::new(window, cx)
                .default_value("Diamond")
                .placeholder("图层名称")
        });
        let opacity = cx.new(|_| {
            SliderState::new()
                .min(0.)
                .max(100.)
                .step(1.)
                .default_value(100.)
        });
        let subscriptions = vec![
            cx.subscribe(&name, |_, _, event: &InputEvent, cx| {
                if matches!(event, InputEvent::Change) {
                    cx.notify();
                }
            }),
            cx.subscribe(&opacity, |this: &mut Self, _, event: &SliderEvent, cx| {
                if let SliderEvent::Change(SliderValue::Single(value)) = event {
                    this.document.opacity = value / 100.;
                    this.redraw(cx);
                }
            }),
        ];
        let mut editor = Self {
            document: Document::default(),
            name,
            opacity,
            frame: None,
            error: None,
            requests: None,
            gpu_device: None,
            _frames: None,
            _subscriptions: subscriptions,
        };
        editor.connect_preview(window, cx);
        editor
    }
    fn connect_preview(&mut self, window: &Window, cx: &mut Context<Self>) {
        // Drop the old receiver before replacing a lost GPU's producer.
        self._frames = None;
        self.requests = None;
        self.frame = None;
        let (requests, frames, device) = match preview::start(window) {
            Ok(connection) => connection,
            Err(error) => {
                self.error = Some(error.to_string());
                return;
            }
        };
        self.gpu_device = device;
        self.error = None;
        self._frames = Some(cx.spawn(async move |this, cx| {
            while let Ok(frame) = frames.recv().await {
                if this
                    .update(cx, |this, cx| {
                        match frame {
                            Ok(frame) => {
                                this.frame = Some(frame);
                                this.error = None;
                            }
                            Err(error) => {
                                this.error = Some(error);
                            }
                        }
                        cx.notify();
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
        let _ = requests.send(self.document);
        self.requests = Some(requests);
    }
    fn redraw(&mut self, cx: &mut Context<Self>) {
        if self
            .requests
            .as_ref()
            .is_none_or(|requests| requests.send(self.document).is_err())
        {
            self.error = Some("预览渲染器已停止，请重新打开窗口".into());
        }
        cx.notify();
    }
}
impl Render for Editor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if preview::device_changed(window, self.gpu_device.as_ref()) {
            self.connect_preview(window, cx);
        }
        let theme = cx.theme();
        let name = self.name.read(cx).value();
        let layer_name = if name.is_empty() {
            SharedString::from("未命名图层")
        } else {
            name
        };
        let mut preview = div()
            .flex_1()
            .min_w_0()
            .h_full()
            .p_6()
            .bg(theme.muted)
            .flex()
            .items_center()
            .justify_center();
        if let Some(error) = &self.error {
            preview = preview.child(div().text_color(theme.danger).child(error.clone()));
        } else if let Some(frame) = &self.frame {
            preview = preview.child(preview::element(frame.clone()));
        } else {
            preview = preview.child("正在初始化预览…");
        }
        div().size_full().flex().flex_col().bg(theme.background).text_color(theme.foreground).text_sm()
            .child(div().h_16().flex_shrink_0().px_4().flex().items_center().gap_2().border_b_1().border_color(theme.border)
                .child(div().font_weight(FontWeight::SEMIBOLD).mr_4().child("Kasane"))
                .child(Button::new("zoom-out").label("缩小").small().disabled(self.document.zoom <= 0.25).on_click(cx.listener(|this, _, _, cx| {
                    this.document.zoom = (this.document.zoom - 0.25).max(0.25); this.redraw(cx);
                })))
                .child(div().w_12().text_center().child(format!("{:.0}%", self.document.zoom * 100.)))
                .child(Button::new("zoom-in").label("放大").small().disabled(self.document.zoom >= 3.).on_click(cx.listener(|this, _, _, cx| {
                    this.document.zoom = (this.document.zoom + 0.25).min(3.); this.redraw(cx);
                })))
                .child(Button::new("reset").label("重置预览").small().on_click(cx.listener(|this, _, window, cx| {
                    this.document = Document::default();
                    this.opacity.update(cx, |state, cx| state.set_value(100., window, cx));
                    this.redraw(cx);
                })))
                .child(div().flex_1())
                .child(Button::new("theme").label("切换主题").ghost().small().on_click(|_, window, cx| {
                    let mode = if cx.theme().is_dark() { ThemeMode::Light } else { ThemeMode::Dark };
                    Theme::change(mode, Some(window), cx);
                }))
                .child(Button::new("about").label("关于…").ghost().small().on_click(|_, window, cx| {
                    window.open_dialog(cx, |dialog, _, _| dialog.title("Kasane 组件原型").child("使用 GPUI Kit 组装界面。场景由 Kasane 绘制；当前原型不保存文档。"));
                })))
            .child(div().flex().flex_1().min_h_0()
                .child(div().w_48().flex_shrink_0().h_full().p_4().flex().flex_col().gap_3().border_r_1().border_color(theme.border).bg(theme.sidebar)
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("图层"))
                    .child(div().text_color(theme.muted_foreground).text_xs().child("当前图层"))
                    .child(div().truncate().child(layer_name))
                    .child(div().text_color(theme.muted_foreground).text_xs().child(if self.document.visible { "可见" } else { "已隐藏" })))
                .child(preview)
                .child(div().w_64().flex_shrink_0().h_full().p_4().flex().flex_col().gap_4().border_l_1().border_color(theme.border)
                    .child(div().font_weight(FontWeight::SEMIBOLD).child("属性"))
                    .child(div().flex().flex_col().gap_2().child("名称").child(Input::new(&self.name)))
                    .child(Checkbox::new("visible").label("显示图层").checked(self.document.visible).on_change(cx.listener(|this, checked, _, cx| {
                        this.document.visible = *checked; this.redraw(cx);
                    })))
                    .child(div().flex().flex_col().gap_3()
                        .child(format!("不透明度  {:.0}%", self.document.opacity * 100.))
                        .child(Slider::new(&self.opacity)))
                    .child(div().text_xs().text_color(theme.muted_foreground).child("网格 · 4 个顶点 · 2 个三角形"))))
            .child(div().h_8().flex_shrink_0().px_4().flex().items_center().border_t_1().border_color(theme.border).text_xs().text_color(theme.muted_foreground)
                .child("组件原型 · 更改仅保留在当前窗口"))
    }
}
fn main() {
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            Theme::change(ThemeMode::Dark, None, cx);
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();
            gpui_kit::open_window(
                WindowOptions {
                    titlebar: Some(TitlebarOptions {
                        title: Some("Kasane — GPUI Kit".into()),
                        ..Default::default()
                    }),
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1100.), px(740.)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(800.), px(480.))),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| Editor::new(window, cx)),
            )
            .expect("open kit editor");
            cx.activate(true);
        });
}
