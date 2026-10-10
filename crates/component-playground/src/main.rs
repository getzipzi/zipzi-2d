use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, Theme, ThemeMode, WindowExt,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::{Input, InputEvent, InputState},
    slider::{Slider, SliderEvent, SliderState},
    tab::{Tab, TabBar},
};
use gpui_kit::{prelude::*, *};

struct Playground {
    input: Entity<InputState>,
    slider: Entity<SliderState>,
    checked: bool,
    clicks: usize,
    page: usize,
    _subscriptions: Vec<Subscription>,
}
impl Playground {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("输入中文或 English"));
        let slider = cx.new(|_| SliderState::new().default_value(50.));
        let subscriptions = vec![
            cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&slider, |_, _, _: &SliderEvent, cx| cx.notify()),
        ];
        Self {
            input,
            slider,
            checked: true,
            clicks: 0,
            page: 0,
            _subscriptions: subscriptions,
        }
    }
}
impl Render for Playground {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut content = div().flex().flex_col().gap_4();
        let code = match self.page {
            0 => {
                content = content
                    .child("Button · 点击、禁用与状态反馈")
                    .child(
                        div()
                            .flex()
                            .gap_3()
                            .child(Button::new("count").label("点击计数").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.clicks += 1;
                                    cx.notify();
                                },
                            )))
                            .child(Button::new("disabled").label("禁用按钮").disabled(true)),
                    )
                    .child(format!("已点击 {} 次", self.clicks));
                "Button::new(\"count\").label(\"点击计数\").on_click(cx.listener(...))"
            }
            1 => {
                content = content
                    .child("Checkbox · 点击或使用 Tab / Space")
                    .child(
                        Checkbox::new("check")
                            .label("显示图层")
                            .checked(self.checked)
                            .on_change(cx.listener(|this, checked, _, cx| {
                                this.checked = *checked;
                                cx.notify();
                            })),
                    )
                    .child(format!("checked = {}", self.checked));
                "Checkbox::new(\"check\").label(\"显示图层\").checked(value).on_change(...)"
            }
            2 => {
                content = content
                    .child("Input · 输入、选择、粘贴与中文输入法")
                    .child(div().max_w_96().child(Input::new(&self.input)))
                    .child(format!("当前文本：{}", self.input.read(cx).value()))
                    .child("输入法检查：连续输入中文，确认组合文本、候选窗与上屏没有重复。");
                "let state = cx.new(|cx| InputState::new(window, cx));\nInput::new(&state)"
            }
            3 => {
                content = content
                    .child("Slider · 拖动或点击轨道")
                    .child(div().max_w_96().child(Slider::new(&self.slider)))
                    .child(format!("当前值：{}", self.slider.read(cx).value()));
                "let state = cx.new(|_| SliderState::new().default_value(50.));\nSlider::new(&state)"
            }
            4 => {
                content = content
                    .child("TabBar · 上方导航即为 Kit 标签组件")
                    .child("切换页面后，输入、复选框和滑块的状态会保留。");
                "TabBar::new(\"pages\").selected_index(index).child(Tab::new().label(\"Input\")).on_click(...)"
            }
            5 => {
                content = content.child("Dialog · 打开后按 Escape 关闭").child(
                    Button::new("dialog")
                        .label("打开弹窗…")
                        .on_click(|_, window, cx| {
                            window.open_dialog(cx, |dialog, _, _| {
                                dialog
                                    .title("组件弹窗")
                                    .child("由 GPUI Kit Root 管理浮层。按 Escape 关闭。")
                            });
                        }),
                );
                "window.open_dialog(cx, |dialog, _, _| dialog.title(\"组件弹窗\").child(...))"
            }
            _ => {
                content = content
                    .child(div().text_xl().child("字体与排版 Typography"))
                    .child("中文图层 · English · 日本語 · 한글 · 0123456789")
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child("辅助文字使用主题语义颜色"),
                    )
                    .child(
                        div()
                            .w_48()
                            .truncate()
                            .child("这是一个用于验证窄列单行截断的较长图层名称"),
                    );
                "div().text_color(cx.theme().muted_foreground).child(\"辅助文字\")"
            }
        };
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(cx.theme().background)
            .text_color(cx.theme().foreground)
            .text_sm()
            .child(
                div()
                    .flex()
                    .items_center()
                    .p_4()
                    .gap_4()
                    .child(div().text_lg().flex_1().child("Kit 组件验证"))
                    .child(
                        Button::new("theme")
                            .label("切换主题")
                            .ghost()
                            .small()
                            .on_click(|_, window, cx| {
                                let mode = if cx.theme().is_dark() {
                                    ThemeMode::Light
                                } else {
                                    ThemeMode::Dark
                                };
                                Theme::change(mode, Some(window), cx);
                            }),
                    ),
            )
            .child(
                TabBar::new("pages")
                    .selected_index(self.page)
                    .children(
                        [
                            "Button",
                            "Checkbox",
                            "Input",
                            "Slider",
                            "Tabs",
                            "Dialog",
                            "Typography",
                        ]
                        .into_iter()
                        .map(|label| Tab::new().label(label)),
                    )
                    .on_click(cx.listener(|this, index, _, cx| {
                        this.page = *index;
                        cx.notify();
                    })),
            )
            .child(
                div()
                    .id("content")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_6()
                    .flex()
                    .flex_col()
                    .gap_6()
                    .child(content)
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child("Rust 用法")
                            .child(div().p_4().bg(cx.theme().muted).child(code))
                            .child(Button::new("copy-code").label("复制代码").small().on_click(
                                move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(code.into()))
                                },
                            )),
                    ),
            )
    }
}
fn main() {
    if let Some(pid_file) = std::env::var_os("KASANE_DEV_PID_FILE") {
        let _ = std::fs::write(pid_file, std::process::id().to_string());
    }
    let background_launch = std::env::var_os("KASANE_DEV_BACKGROUND").is_some();
    gpui_kit::application()
        .with_assets(gpui_kit::assets::Assets)
        .run(move |cx| {
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
                        title: Some("Kasane Component Playground".into()),
                        ..Default::default()
                    }),
                    window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
                        None,
                        size(px(1000.), px(680.)),
                        cx,
                    ))),
                    window_min_size: Some(size(px(800.), px(480.))),
                    ..Default::default()
                },
                cx,
                |window, cx| cx.new(|cx| Playground::new(window, cx)),
            )
            .expect("open playground");
            if !background_launch {
                cx.activate(true);
            }
        });
}
