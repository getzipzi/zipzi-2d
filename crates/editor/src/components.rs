//! Interactive rendering checks for common editor controls.
use gpui::{prelude::*, *};
#[path = "vendor/text_editor.rs"]
mod text_editor;

actions!(component_lab, [Backspace, Delete, End, Home, Left, Right]);

pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some("LabInput")),
        KeyBinding::new("delete", Delete, Some("LabInput")),
        KeyBinding::new("left", Left, Some("LabInput")),
        KeyBinding::new("right", Right, Some("LabInput")),
        KeyBinding::new("home", Home, Some("LabInput")),
        KeyBinding::new("end", End, Some("LabInput")),
    ]);
}

pub struct ComponentLab {
    input: Entity<text_editor::Editor>,
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
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            input: cx.new(|cx| text_editor::Editor::new("Layer 01 · 图层", window, cx)),
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
        let focus = self.input.read(cx).focus_handle.clone();
        let input = div()
            .id("name-input")
            .key_context("LabInput")
            .track_focus(&focus)
            .map(text_editor::standard_actions(self.input.clone()))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                window.focus(&focus, cx)
            })
            .h(px(38.0))
            .px_2()
            .flex()
            .items_center()
            .bg(rgb(0x111b29))
            .border_1()
            .border_color(if self.input.read(cx).focus_handle.is_focused(window) {
                rgb(0x62cde6)
            } else {
                rgb(0x506078)
            })
            .rounded_md()
            .overflow_hidden()
            .cursor(CursorStyle::IBeam)
            .child(
                self.input
                    .clone()
                    .cached(StyleRefinement::default().size_full()),
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
            .child(div().h(px(24.0)).flex_shrink_0().text_color(rgb(0xa5c5e3)).child(if self.status == "Input read" { format!("Input: {}", self.input.read(cx).text(cx)) } else { self.status.clone() }));
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
