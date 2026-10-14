use crate::{EditorView, t};
use editor_core::{CommandId, ToolId};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::*, *};

pub(crate) type ToolbarAction = fn(&mut EditorView, &mut Window, &mut Context<EditorView>);
pub(crate) type ToolbarSelected = fn(&EditorView, &App) -> bool;
pub(crate) type ToolbarEnabled = fn(&EditorView, bool, &App) -> bool;

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
                enabled: |view, modal, cx| command_enabled(view, modal, cx, CommandId::SelectTool),
                action: Some(select_tool),
            },
            ToolbarItemConfig {
                id: "tool_glue",
                icon: IconName::Link,
                tooltip_key: "editor.tool_glue",
                selected: glue_selected,
                enabled: |view, modal, cx| command_enabled(view, modal, cx, CommandId::GlueTool),
                action: Some(glue_tool),
            },
            ToolbarItemConfig {
                id: "tool_hand",
                icon: IconName::Hand,
                tooltip_key: "editor.tool_hand",
                selected: hand_selected,
                enabled: |view, modal, cx| command_enabled(view, modal, cx, CommandId::HandTool),
                action: Some(hand_tool),
            },
            ToolbarItemConfig {
                id: "tool_generate_mesh",
                icon: IconName::Grid2x2,
                tooltip_key: "editor.tool_generate_mesh",
                selected: |_, _| false,
                enabled: |view, modal, cx| {
                    command_enabled(view, modal, cx, CommandId::GenerateMesh)
                },
                action: Some(generate_mesh),
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

pub fn render_toolbar(
    owner: &Entity<EditorView>,
    canvas: &Entity<crate::canvas::CanvasView>,
    modal: bool,
    cx: &App,
) -> impl IntoElement {
    let view = owner.read(cx);
    div()
        .id("canvas-toolbar")
        .test_support()
        .absolute()
        .top_2()
        .left_2()
        .occlude()
        .flex()
        .flex_col()
        .items_center()
        .gap_1()
        .p_1()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
        .children(
            view.toolbar
                .items
                .iter()
                .filter(|item| {
                    item.id == "tool_hand"
                        || view.editor.workspace() == editor_core::WorkspaceKind::Modeling
                })
                .map(|item| {
                    let action = item.action;
                    let owner = owner.clone();
                    let canvas = canvas.clone();
                    Button::new(item.id)
                        .small()
                        .ghost()
                        .when((item.selected)(view, cx), |button| button.primary())
                        .icon(item.icon)
                        .accessibility_label(t!(item.tooltip_key).into_owned())
                        .tooltip(t!(item.tooltip_key))
                        .disabled(!(item.enabled)(view, modal, cx))
                        .on_click(move |_, window, cx| {
                            let id = canvas.entity_id();
                            let focus = canvas.read(cx).focus.clone();
                            window.focus(&focus, cx);
                            owner.update(cx, |view, _| {
                                if let Some(dock) = &mut view.dock {
                                    dock.activate_canvas(id);
                                }
                            });
                            if let Some(action) = action {
                                owner.update(cx, |view, cx| action(view, window, cx));
                            }
                        })
                })
                .collect::<Vec<_>>(),
        )
}

fn select_selected(view: &EditorView, _: &App) -> bool {
    view.editor.active_tool() == ToolId::Select
}
fn glue_selected(view: &EditorView, _: &App) -> bool {
    view.editor.active_tool() == ToolId::Glue
}
fn hand_selected(view: &EditorView, _: &App) -> bool {
    view.editor.active_tool() == ToolId::Hand
}
fn command_enabled(view: &EditorView, modal: bool, _: &App, command: CommandId) -> bool {
    !modal && view.editor.command_available(view.editor.command(command))
}
fn generate_mesh(view: &mut EditorView, window: &mut Window, cx: &mut Context<EditorView>) {
    view.run_command(CommandId::GenerateMesh, window, cx);
}
fn select_tool(view: &mut EditorView, window: &mut Window, cx: &mut Context<EditorView>) {
    view.run_command(CommandId::SelectTool, window, cx);
}
fn glue_tool(view: &mut EditorView, window: &mut Window, cx: &mut Context<EditorView>) {
    view.run_command(CommandId::GlueTool, window, cx);
}
fn hand_tool(view: &mut EditorView, window: &mut Window, cx: &mut Context<EditorView>) {
    view.run_command(CommandId::HandTool, window, cx);
}
