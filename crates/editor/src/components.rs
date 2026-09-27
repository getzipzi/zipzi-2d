//! Interactive rendering checks for common editor controls.
use gpui::{prelude::*, *};
use gpui_elements::editable_text::{
    EditableTextState, StringStorage,
    actions::{DEFAULT_INPUT_CONTEXT, default_bindings},
    text_input,
};

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys(default_bindings().as_keybindings(Some(DEFAULT_INPUT_CONTEXT)));
}

pub struct ComponentLab {
    input: Entity<EditableTextState>,
    checked: bool,
    expanded: bool,
    selected: usize,
    tab: usize,
    menu: bool,
    mode: usize,
    modal: bool,
    focus: FocusHandle,
    value: f32,
    slider_bounds: Bounds<Pixels>,
    dragging: bool,
    status: String,
}
impl ComponentLab {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            input: cx.new(|cx| EditableTextState::new(StringStorage::from("Layer 01 · 图层"), cx)),
            checked: true,
            expanded: true,
            selected: 0,
            tab: 0,
            menu: false,
            mode: 0,
            modal: false,
            focus: cx.focus_handle(),
            value: 65.0,
            slider_bounds: Bounds::default(),
            dragging: false,
            status: "Ready · choose a control".into(),
        }
    }
    fn button(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        action: fn(&mut Self),
        cx: &Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(id)
            .px_3()
            .py_2()
            .rounded_md()
            .bg(rgb(0x34445b))
            .cursor_pointer()
            .hover(|s| s.bg(rgb(0x476489)))
            .child(label.into())
            .on_click(cx.listener(move |this, _, window, cx| {
                window.focus(&this.focus, cx);
                action(this);
                cx.notify();
            }))
    }
    fn slide(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let width = f32::from(self.slider_bounds.size.width);
        if width > 0.0 {
            self.value = ((f32::from(position.x - self.slider_bounds.left()) / width) * 100.0)
                .clamp(0.0, 100.0)
                .round();
            self.status = format!("Slider changed: {:.0}%", self.value);
            cx.notify();
        }
    }
}
fn section(title: &'static str) -> Div {
    div()
        .flex()
        .flex_col()
        .gap_3()
        .p_4()
        .rounded_lg()
        .border_1()
        .border_color(rgb(0x3a4659))
        .bg(rgb(0x202b3b))
        .child(div().text_color(rgb(0x8fadd0)).child(title))
}
impl Render for ComponentLab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.input.read(cx).focus_handle(cx);
        // Center a single line, rather than stretching its paint bounds to the field height.
        let input = div()
            .id("name-input")
            .h(px(38.0))
            .px_2()
            .flex()
            .items_center()
            .bg(rgb(0x111b29))
            .border_1()
            .border_color(if focus.is_focused(window) {
                rgb(0x62cde6)
            } else {
                rgb(0x506078)
            })
            .rounded_md()
            .overflow_hidden()
            .cursor(CursorStyle::IBeam)
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx)
            })
            .child(
                text_input("layer-name")
                    .state(self.input.downgrade())
                    .caret_blink_interval_500ms()
                    .placeholder("Layer name / 图层名称")
                    .w_full()
                    .h(px(20.0))
                    .line_height(px(20.0))
                    .whitespace_nowrap()
                    .overflow_x_scroll(),
            );
        let entity = cx.entity().downgrade();
        let slider = div()
            .id("opacity-slider")
            .relative()
            .h(px(28.0))
            .w_full()
            .cursor_pointer()
            .child(
                div()
                    .absolute()
                    .top(px(11.0))
                    .h(px(6.0))
                    .w_full()
                    .rounded_md()
                    .bg(rgb(0x101a29)),
            )
            .child(
                div()
                    .absolute()
                    .top(px(11.0))
                    .h(px(6.0))
                    .w(relative(self.value / 100.0))
                    .rounded_md()
                    .bg(rgb(0x62cde6)),
            )
            .child(
                div()
                    .absolute()
                    .left(relative(self.value / 100.0))
                    .ml(px(-7.0))
                    .top(px(7.0))
                    .size(px(14.0))
                    .rounded_full()
                    .bg(rgb(0xe5f7ff)),
            )
            .child(
                canvas(
                    move |bounds, _, cx| {
                        let _ = entity.update(cx, |this, _| this.slider_bounds = bounds);
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    this.dragging = true;
                    this.slide(event.position, cx);
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                if this.dragging {
                    this.slide(event.position, cx);
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.dragging = false),
            )
            .on_mouse_up_out(
                MouseButton::Left,
                cx.listener(|this, _, _, _| this.dragging = false),
            );
        let mut dropdown = div().relative().child(self.button(
            "blend-menu",
            format!("{}  ▾", ["Normal", "Multiply", "Screen"][self.mode]),
            |s| s.menu = !s.menu,
            cx,
        ));
        if self.menu {
            dropdown = dropdown.child(
                deferred(
                    div()
                        .absolute()
                        .top(px(42.0))
                        .left_0()
                        .w(px(180.0))
                        .p_1()
                        .rounded_md()
                        .shadow_lg()
                        .border_1()
                        .border_color(rgb(0x6786ac))
                        .bg(rgb(0x31425a))
                        .children(
                            ["Normal", "Multiply", "Screen"]
                                .into_iter()
                                .enumerate()
                                .map(|(index, label)| {
                                    div()
                                        .id(("blend", index))
                                        .p_2()
                                        .cursor_pointer()
                                        .hover(|s| s.bg(rgb(0x4b6487)))
                                        .child(label)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.mode = index;
                                            this.menu = false;
                                            this.status = format!("Blend mode: {label}");
                                            cx.notify();
                                        }))
                                }),
                        ),
                )
                .priority(1),
            );
        }
        let tree = div()
            .id("tree-list")
            .h(px(260.0))
            .overflow_y_scroll()
            .flex()
            .flex_col()
            .gap_1()
            .child(self.button(
                "tree-expand",
                if self.expanded {
                    "▾  Scene / 场景"
                } else {
                    "▸  Scene / 场景"
                },
                |s| s.expanded = !s.expanded,
                cx,
            ))
            .when(self.expanded, |tree| {
                tree.children((0..30).map(|index| {
                    div()
                        .id(("layer", index))
                        .flex_shrink_0()
                        .ml_4()
                        .px_3()
                        .py_2()
                        .rounded_md()
                        .cursor_pointer()
                        .bg(if self.selected == index {
                            rgb(0x386080)
                        } else {
                            rgb(0x202b3b)
                        })
                        .hover(|s| s.bg(rgb(0x41546c)))
                        .child(format!("◇  Layer {:02} / 网格", index + 1))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.selected = index;
                            this.status = format!("Selected layer {:02}", index + 1);
                            cx.notify();
                        }))
                }))
            });
        let mut root = div().id("component-lab").track_focus(&self.focus).relative().size_full().min_h_0().flex().flex_col().p_4().gap_3().bg(rgb(0x141e2d))
            .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| { if event.keystroke.key == "escape" { this.menu = false; this.modal = false; cx.notify(); } }))
            .child(div().flex().gap_2().children(["Controls", "Typography"].into_iter().enumerate().map(|(index, label)| {
                div().id(("lab-tab", index)).px_4().py_2().rounded_md().cursor_pointer().bg(if self.tab == index { rgb(0x3c698a) } else { rgb(0x263448) }).child(label)
                    .on_click(cx.listener(move |this, _, _, cx| { this.tab = index; cx.notify(); }))
            })).child(div().id("compact-window").px_3().py_2().cursor_pointer().child("Compact").on_click(|_, window, _| window.resize(size(px(760.0), px(540.0)))))
                .child(div().id("normal-window").px_3().py_2().cursor_pointer().child("Full size").on_click(|_, window, _| window.resize(size(px(1100.0), px(740.0))))))
            .child(div().id("lab-scroll").flex_1().min_h_0().overflow_y_scroll()
                .when(self.tab == 0, |body| body.child(div().flex().gap_4()
                    .child(div().flex_1().min_w_0().flex().flex_col().gap_3()
                        .child(section("BUTTONS & INPUT").child(div().flex().gap_2()
                            .child(self.button("apply", "Apply", |s| s.status = "Applied successfully".into(), cx))
                            .child(div().px_3().py_2().rounded_md().bg(rgb(0x2a3341)).text_color(rgb(0x727e91)).child("Disabled")))
                            .child(input).child(self.button("read-name", "Read input", |s| s.status = "Input read".into(), cx)))
                        .child(section("PROPERTIES").child(self.button("checkbox", if self.checked { "☑  Visible / 可见" } else { "☐  Visible / 可见" }, |s| { s.checked = !s.checked; s.status = format!("Visible: {}", s.checked); }, cx))
                            .child(format!("Opacity: {:.0}%", self.value)).child(slider).child(dropdown))
                    )
                    .child(div().flex_1().min_w_0().flex().flex_col().gap_3()
                        .child(section("SCENE TREE / SCROLL & CLIPPING").child(tree))
                        .child(section("OVERLAY & STATUS").child(self.button("open-modal", "Open dialog", |s| { s.modal = true; s.menu = false; }, cx))
                            .child(div().text_color(rgb(0x8de0b8)).child("● Ready"))))))
                .when(self.tab == 1, |body| body.child(section("TEXT / FONT FALLBACK / CLIPPING")
                    .child(div().text_size(px(28.0)).child("Editor typography"))
                    .child("中文：图层、参数、时间轴。日本語：レイヤー。한국어: 레이어.")
                    .child("Symbols: ← → ↔ ✓ ✕ ◇ ▾  •  fi fl ffi  0123456789")
                    .child(div().text_size(px(12.0)).child("Small text / 小字号 — 12 px"))
                    .child(div().w(px(260.0)).truncate().child("Long layer name / 很长的图层名称 / should truncate at the boundary")))))
            .child(div().h(px(24.0)).flex_shrink_0().text_color(rgb(0xa5c5e3)).child(if self.status == "Input read" { format!("Input: {}", self.input.read(cx).as_str()) } else { self.status.clone() }));
        if self.modal {
            root = root.child(
                deferred(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(rgba(0x000000a0))
                        .occlude()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .child(
                            div()
                                .w(px(380.0))
                                .p_6()
                                .flex()
                                .flex_col()
                                .gap_4()
                                .rounded_lg()
                                .shadow_lg()
                                .border_1()
                                .border_color(rgb(0x7a9abb))
                                .bg(rgb(0x27384e))
                                .child(div().text_xl().child("Save changes?"))
                                .child("Modal overlay / 弹窗与背景遮挡")
                                .child(
                                    div()
                                        .flex()
                                        .gap_3()
                                        .child(self.button(
                                            "cancel-dialog",
                                            "Cancel",
                                            |s| {
                                                s.modal = false;
                                                s.status = "Dialog cancelled".into();
                                            },
                                            cx,
                                        ))
                                        .child(self.button(
                                            "confirm-dialog",
                                            "Confirm",
                                            |s| {
                                                s.modal = false;
                                                s.status = "Dialog confirmed".into();
                                            },
                                            cx,
                                        )),
                                ),
                        ),
                )
                .priority(2),
            );
        }
        root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::prelude::v1::test;

    /// Exercise the same state mounted by ComponentLab, using macOS IME callbacks.
    #[gpui::test]
    fn ime_updates_replace_composition_instead_of_appending(cx: &mut TestAppContext) {
        let view = cx.add_window(|window, cx| ComponentLab::new(window, cx));
        view.update(cx, |view, window, cx| {
            view.input.update(cx, |input, cx| {
                input.emplace("", cx);
                for text in ["n", "ni", "你"] {
                    let end = text.encode_utf16().count();
                    input.replace_and_mark_text_in_range(None, text, Some(end..end), window, cx);
                    assert_eq!(input.as_str(), text);
                    assert_eq!(input.marked_text_range(window, cx), Some(0..end));
                }
                input.replace_text_in_range(None, "你", window, cx);
                assert_eq!(input.as_str(), "你");
                assert_eq!(input.marked_text_range(window, cx), None);
                for text in ["h", "hao", "好"] {
                    let end = text.encode_utf16().count();
                    input.replace_and_mark_text_in_range(None, text, Some(end..end), window, cx);
                    assert_eq!(input.as_str(), format!("你{text}"));
                }
                input.replace_text_in_range(None, "好", window, cx);
                assert_eq!(input.as_str(), "你好");
                assert_eq!(input.marked_text_range(window, cx), None);
            });
        })
        .unwrap();
    }

    #[gpui::test]
    fn ime_replacement_uses_utf16_ranges_after_emoji(cx: &mut TestAppContext) {
        let view = cx.add_window(|window, cx| ComponentLab::new(window, cx));
        view.update(cx, |view, window, cx| {
            view.input.update(cx, |input, cx| {
                input.emplace("😀旧文", cx);
                input.replace_and_mark_text_in_range(Some(2..4), "zhong", Some(5..5), window, cx);
                assert_eq!(input.as_str(), "😀zhong");
                assert_eq!(input.marked_text_range(window, cx), Some(2..7));
                input.replace_and_mark_text_in_range(None, "中文", Some(2..2), window, cx);
                assert_eq!(input.as_str(), "😀中文");
                input.replace_text_in_range(None, "中文", window, cx);
                assert_eq!(input.as_str(), "😀中文");
                assert_eq!(input.marked_text_range(window, cx), None);
            });
        })
        .unwrap();
    }
}
