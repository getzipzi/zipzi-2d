use crate::{canvas::CanvasView, t};
use editor_core::{CommandId, ToolId};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable, WindowExt,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::*, *};

pub(crate) type ToolbarAction = fn(&mut CanvasView, &mut Window, &mut Context<CanvasView>);
pub(crate) type ToolbarSelected = fn(&CanvasView, &App) -> bool;
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
                selected: mesh_selected,
                enabled: not_modal,
                action: Some(mesh_tool),
            },
            ToolbarItemConfig {
                id: "tool_deformer",
                icon: IconName::Sparkles,
                tooltip_key: "editor.tool_deformer",
                selected: deformer_selected,
                enabled: not_modal,
                action: Some(deformer_tool),
            },
            ToolbarItemConfig {
                id: "tool_glue",
                icon: IconName::Link,
                tooltip_key: "editor.tool_glue",
                selected: glue_selected,
                enabled: not_modal,
                action: Some(glue_tool),
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
                        .when((item.selected)(canvas, cx), |button| button.primary())
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

fn select_selected(canvas: &CanvasView, cx: &App) -> bool {
    canvas.active_tool(cx) == ToolId::Select
}
fn mesh_selected(canvas: &CanvasView, cx: &App) -> bool {
    canvas.active_tool(cx) == ToolId::Vertices
}
fn deformer_selected(canvas: &CanvasView, cx: &App) -> bool {
    canvas.active_tool(cx) == ToolId::Deformers
}
fn glue_selected(canvas: &CanvasView, cx: &App) -> bool {
    canvas.active_tool(cx) == ToolId::Glue
}
fn hand_selected(canvas: &CanvasView, cx: &App) -> bool {
    canvas.active_tool(cx) == ToolId::Hand
}
fn not_modal(_: &CanvasView, modal: bool) -> bool {
    !modal
}
fn select_tool(canvas: &mut CanvasView, window: &mut Window, cx: &mut Context<CanvasView>) {
    set_tool(canvas, CommandId::SelectTool, window, cx);
}
fn mesh_tool(canvas: &mut CanvasView, window: &mut Window, cx: &mut Context<CanvasView>) {
    set_tool(canvas, CommandId::Vertices, window, cx);
}
fn deformer_tool(canvas: &mut CanvasView, window: &mut Window, cx: &mut Context<CanvasView>) {
    set_tool(canvas, CommandId::DeformerTool, window, cx);
}
fn glue_tool(canvas: &mut CanvasView, window: &mut Window, cx: &mut Context<CanvasView>) {
    set_tool(canvas, CommandId::GlueTool, window, cx);
}
fn hand_tool(canvas: &mut CanvasView, window: &mut Window, cx: &mut Context<CanvasView>) {
    set_tool(canvas, CommandId::HandTool, window, cx);
}
fn set_tool(
    canvas: &mut CanvasView,
    command: CommandId,
    window: &mut Window,
    cx: &mut Context<CanvasView>,
) {
    if let Some(owner) = canvas.owner.upgrade() {
        if !owner.read(cx).modal && !window.has_active_dialog(cx) {
            canvas.cancel_pointer();
            window.focus(&canvas.focus, cx);
            owner.update(cx, |owner, cx| {
                let result = owner
                    .editor
                    .dispatch(owner.editor.command(command))
                    .map(|_| ());
                owner.complete_edit(result, cx);
            });
            cx.notify();
        }
    }
}
