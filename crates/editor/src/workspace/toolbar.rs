use crate::{canvas::CanvasView, t};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::*, *};

pub(crate) type ToolbarAction = fn(&mut CanvasView, &mut Window, &mut Context<CanvasView>);
pub(crate) type ToolbarSelected = fn(&CanvasView) -> bool;
pub(crate) type ToolbarEnabled = fn(&CanvasView, bool) -> bool;

/// One button's appearance and behavior. Items render in registration order.
#[derive(Clone, Copy)]
pub(crate) struct ToolbarItemConfig {
    pub id: &'static str,
    pub icon: IconName,
    pub tooltip_key: &'static str,
    pub selected: ToolbarSelected,
    pub enabled: ToolbarEnabled,
    pub action: Option<ToolbarAction>,
}

#[derive(Default)]
pub(crate) struct ToolbarRegistry {
    items: Vec<ToolbarItemConfig>,
}

impl ToolbarRegistry {
    pub(crate) fn with_defaults() -> Self {
        let mut registry = Self::default();
        for item in [
            ToolbarItemConfig {
                id: "tool_select",
                icon: IconName::MousePointer,
                tooltip_key: "editor.tool_select",
                selected: select_selected,
                enabled: not_modal,
                action: Some(select_tool),
            },
            ToolbarItemConfig {
                id: "tool_mesh",
                icon: IconName::Grid2x2,
                tooltip_key: "editor.tool_mesh",
                selected: never_selected,
                enabled: never_enabled,
                action: None,
            },
            ToolbarItemConfig {
                id: "tool_deformer",
                icon: IconName::Sparkles,
                tooltip_key: "editor.tool_deformer",
                selected: never_selected,
                enabled: never_enabled,
                action: None,
            },
            ToolbarItemConfig {
                id: "tool_hand",
                icon: IconName::Hand,
                tooltip_key: "editor.tool_hand",
                selected: hand_selected,
                enabled: not_modal,
                action: Some(hand_tool),
            },
        ] {
            registry.register(item).unwrap();
        }
        registry
    }

    pub(crate) fn register(&mut self, item: ToolbarItemConfig) -> Result<(), &'static str> {
        if self.items.iter().any(|registered| registered.id == item.id) {
            return Err("toolbar item id is already registered");
        }
        self.items.push(item);
        Ok(())
    }
}

pub fn render_floating_toolbar(
    canvas: &CanvasView,
    modal: bool,
    cx: &mut Context<CanvasView>,
) -> impl IntoElement {
    let theme = cx.theme();
    div()
        .id("canvas-floating-toolbar")
        .test_support()
        .absolute()
        .top_2()
        .left_2()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .p_1()
        .rounded_md()
        .bg(theme.background.opacity(0.85))
        .border_1()
        .border_color(theme.border)
        .shadow_md()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
        .children(
            canvas
                .toolbar
                .items
                .iter()
                .map(|item| {
                    let action = item.action;
                    Button::new(item.id)
                        .small()
                        .ghost()
                        .when((item.selected)(canvas), |button| button.primary())
                        .icon(item.icon)
                        .tooltip(t!(item.tooltip_key))
                        .disabled(!(item.enabled)(canvas, modal))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if let Some(action) = action {
                                action(this, window, cx);
                            }
                        }))
                })
                .collect::<Vec<_>>(),
        )
}

fn select_selected(canvas: &CanvasView) -> bool {
    !canvas.hand_tool()
}

fn hand_selected(canvas: &CanvasView) -> bool {
    canvas.hand_tool()
}

fn never_selected(_: &CanvasView) -> bool {
    false
}

fn not_modal(_: &CanvasView, modal: bool) -> bool {
    !modal
}

fn never_enabled(_: &CanvasView, _: bool) -> bool {
    false
}

fn select_tool(canvas: &mut CanvasView, window: &mut Window, cx: &mut Context<CanvasView>) {
    set_tool(canvas, false, window, cx);
}

fn hand_tool(canvas: &mut CanvasView, window: &mut Window, cx: &mut Context<CanvasView>) {
    set_tool(canvas, true, window, cx);
}

fn set_tool(
    canvas: &mut CanvasView,
    hand: bool,
    window: &mut Window,
    cx: &mut Context<CanvasView>,
) {
    if let Some(owner) = canvas.owner.upgrade() {
        let is_modal = owner.read(cx).modal || window.has_active_dialog(cx);
        if !is_modal {
            canvas.set_hand_tool(hand, cx);
            window.focus(&canvas.focus, cx);
            owner.update(cx, |owner, cx| {
                owner.editor.cancel_gesture();
                owner.redraw(cx);
            });
        }
    }
}
