use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, Theme, ThemeMode, WindowExt,
    button::{Button, ButtonVariants},
    checkbox::Checkbox,
    input::{Input, InputEvent, InputState},
    slider::{Slider, SliderEvent, SliderState},
    tab::{Tab, TabBar},
};
use gpui_kit::{prelude::*, *};

#[path = "../../editor/src/ui/compact.rs"]
mod compact;

// Exercise the exact component used by the editor, including its interaction state.
#[path = "../../editor/src/ui/slide_number_input.rs"]
mod slide_number_input;
use slide_number_input::{SlideNumberChange, SlideNumberInput, SlideNumberOptions};

struct Playground {
    input: Entity<InputState>,
    slider: Entity<SliderState>,
    number_input: Entity<SlideNumberInput>,
    number_text: Entity<InputState>,
    number_value: f64,
    checked: bool,
    clicks: usize,
    page: usize,
    ui_scale_index: usize,
    _subscriptions: Vec<Subscription>,
}
impl Playground {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("输入中文或 English"));
        let slider = cx.new(|_| SliderState::new().default_value(50.));
        let number_text = cx.new(|cx| {
            InputState::new(window, cx)
                .step(0.1)
                .min(-30.)
                .max(30.)
                .default_value("0.0")
        });
        let number_input = cx.new(|cx| {
            SlideNumberInput::new(
                number_text.clone(),
                "Angle",
                SlideNumberOptions {
                    value: 0.,
                    min: -30.,
                    max: 30.,
                    step: 0.1,
                    drag_distance: Some(200.),
                    disabled: false,
                },
                window,
                cx,
            )
        });
        let subscriptions = vec![
            cx.subscribe(&input, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&slider, |_, _, _: &SliderEvent, cx| cx.notify()),
            cx.subscribe_in(
                &number_input,
                window,
                |this, _, event: &SlideNumberChange, window, cx| {
                    this.number_value = event.0;
                    this.number_text.update(cx, |input, cx| {
                        input.set_value(event.0.to_string(), window, cx)
                    });
                    cx.notify();
                },
            ),
            cx.subscribe_in(
                &number_text,
                window,
                |this, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. } | InputEvent::Blur) {
                        if let Ok(value) = this.number_text.read(cx).value().parse::<f64>() {
                            this.number_value = value.clamp(-30., 30.);
                        }
                        this.number_text.update(cx, |input, cx| {
                            input.set_value(this.number_value.to_string(), window, cx)
                        });
                        cx.notify();
                    }
                },
            ),
        ];
        Self {
            input,
            slider,
            number_input,
            number_text,
            number_value: 0.,
            checked: true,
            clicks: 0,
            page: 8,
            ui_scale_index: 0,
            _subscriptions: subscriptions,
        }
    }
}
impl Playground {
    fn compact_sample(&self, cx: &mut Context<Self>) -> Div {
        let panel = |title: &'static str| {
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .rounded(compact::rem(compact::PANEL_RADIUS))
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().background)
                .child(
                    div()
                        .h(compact::rem(compact::DOCK_HEADER))
                        .flex()
                        .items_center()
                        .px(compact::rem(compact::PADDING))
                        .bg(cx.theme().tab_bar)
                        .border_b_1()
                        .border_color(cx.theme().border)
                        .child(title),
                )
        };
        div().flex().flex_col().gap(compact::rem(compact::PADDING))
            .text_size(compact::rem(compact::BODY))
            .child("Compact · 中文 / English · 基准：12px 文字 · 22px 控件")
            .child(div().flex().items_center().gap(compact::rem(compact::GAP))
                .child(compact::button(Button::new("compact-default")).label("默认 Default")
                    .on_click(cx.listener(|this, _, _, cx| { this.clicks += 1; cx.notify(); })))
                .child(compact::toggle(Button::new("compact-selected"), true).label("选中 Selected"))
                .child(compact::button(Button::new("compact-disabled")).label("禁用 Disabled").disabled(true))
                .child(compact::header_button(Button::new("compact-icon")).icon(IconName::Plus)
                    .accessibility_label("添加 Add").tooltip("添加 Add"))
                .child(format!("{} 次", self.clicks)))
            .child(compact::group(
                "compact-tools",
                "工具 Tools",
                Axis::Horizontal,
                vec![
                    compact::toggle(Button::new("compact-tool-select"), self.checked)
                        .icon(IconName::MousePointer)
                        .accessibility_label("选择 Select")
                        .tooltip("选择 Select")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.checked = true;
                            cx.notify();
                        })),
                    compact::toggle(Button::new("compact-tool-hand"), !self.checked)
                        .icon(IconName::Hand)
                        .accessibility_label("平移 Pan")
                        .tooltip("平移 Pan")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.checked = false;
                            cx.notify();
                        })),
                ],
            ))
            .child(div().flex().gap(compact::rem(compact::PANEL_GAP)).p(compact::rem(compact::PANEL_GAP))
                .bg(compact::workspace(cx))
                .child(panel("属性 Properties").child(div().p(compact::rem(compact::PADDING))
                    .child(compact::property_row("名称 Name", compact::property_value("中文图层 · English"), cx))
                    .child(compact::property_row("角度 Angle", self.number_input.clone(), cx))
                    .child(compact::property_row("长属性名称", compact::property_value("长值省略显示，悬停查看完整内容。"), cx))))
                .child(panel("搜索 Search").child(div().p(compact::rem(compact::PADDING))
                    .child(compact::input(Input::new(&self.input)).cleanable(true)
                        .prefix(gpui_kit::component::Icon::new(IconName::Search).xsmall()))
                    .child(compact::property_row("过滤 Filter", compact::property_value(self.input.read(cx).value()), cx)))))
            .child(div().w(compact::rem(180.)).child(panel("窄面板 Narrow").child(div().p(compact::rem(compact::PADDING))
                .child(compact::property_row("名称 Name", compact::property_value("中文长标签 · English value"), cx)))))
            .child(div().text_size(compact::rem(compact::CAPTION)).text_color(cx.theme().muted_foreground)
                .child("将指针移到按钮上检查 hover；点击或按 Tab 检查焦点；数字支持拖动、Shift 精调和 Escape 恢复。"))
    }
}

impl Render for Playground {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.number_input.update(cx, |input, cx| {
            input.configure(
                "Angle",
                SlideNumberOptions {
                    value: self.number_value,
                    min: -30.,
                    max: 30.,
                    step: 0.1,
                    drag_distance: Some(200.),
                    disabled: false,
                },
                cx,
            )
        });
        let scale_factor = [1., 1.1, 1.25][self.ui_scale_index];
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
            6 => {
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
            7 => {
                content = content
                    .child("Slide NumberInput · 横向拖动调值，单击输入")
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child("Angle")
                            .child(div().w(px(48.)).child(self.number_input.clone())),
                    )
                    .child(format!("当前值：{}  ·  范围：−30 … 30", self.number_value))
                    .child("向右增加，向左减少；拖动 200px 覆盖整个范围，按住 Shift 以十分之一速度微调。")
                    .child(
                        "Escape 撤销本次拖动；单击后可输入数值，用 ↑ / ↓ 步进，Enter 或失焦提交。",
                    );
                "SlideNumberInput::new(input, label, SlideNumberOptions { value: 0., min: -30., max: 30., step: 0.1, drag_distance: Some(200.), disabled: false }, window, cx)"
            }
            _ => {
                content = self.compact_sample(cx);
                "compact::button(Button::new(id))\ncompact::input(Input::new(&state))\ncompact::property_row(label, value, cx)"
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
                        compact::button(Button::new("ui-scale-sample"))
                            .label(format!("UI {}%", [100, 110, 125][self.ui_scale_index]))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.ui_scale_index = (this.ui_scale_index + 1) % 3;
                                let scale = [1., 1.1, 1.25][this.ui_scale_index];
                                Theme::update(cx, |theme| {
                                    theme.font_size = px(16. * scale);
                                    theme.radius = px(compact::RADIUS * scale);
                                });
                                window.refresh();
                                cx.notify();
                            })),
                    )
                    .child(
                        Button::new("theme")
                            .label("切换主题")
                            .ghost()
                            .small()
                            .on_click(move |_, window, cx| {
                                let mode = if cx.theme().is_dark() {
                                    ThemeMode::Light
                                } else {
                                    ThemeMode::Dark
                                };
                                Theme::change(mode, Some(window), cx);
                                Theme::update(cx, |theme| {
                                    compact::apply_theme(theme);
                                    theme.font_size = px(16. * scale_factor);
                                    theme.radius = px(compact::RADIUS * scale_factor);
                                });
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
                            "Slide Number",
                            "Compact",
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
            Theme::update(cx, compact::apply_theme);
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
