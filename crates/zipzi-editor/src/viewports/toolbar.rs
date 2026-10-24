use crate::{EditorView, t, ui::compact};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
};
use gpui_kit::{prelude::*, *};
use zipzi_editor_core::CommandId;

const TOOL_BUTTON_SIZE: f32 = 28.;

/// One button's appearance and behavior. Items render in registration order.
#[derive(Clone, Copy)]
pub(crate) struct ToolbarItemConfig {
    pub id: &'static str,
    pub icon: IconName,
    pub command: CommandId,
}

#[derive(Default)]
pub(crate) struct ToolbarRegistry {
    items: Vec<ToolbarItemConfig>,
}

impl ToolbarRegistry {
    pub(super) fn items(&self) -> Vec<ToolbarItemConfig> {
        self.items.clone()
    }

    pub(crate) fn with_defaults() -> Self {
        let mut registry = Self::default();
        for item in [
            ToolbarItemConfig {
                id: "tool_select",
                icon: IconName::MousePointer,
                command: CommandId::SelectTool,
            },
            ToolbarItemConfig {
                id: "tool_lasso",
                icon: IconName::Lasso,
                command: CommandId::LassoTool,
            },
            ToolbarItemConfig {
                id: "tool_glue",
                icon: IconName::Link,
                command: CommandId::GlueTool,
            },
            ToolbarItemConfig {
                id: "tool_generate_mesh",
                icon: IconName::Grid2x2,
                command: CommandId::GenerateMesh,
            },
        ] {
            registry.register(item).unwrap();
        }
        registry
    }

    pub(crate) fn navigation() -> Self {
        Self::default()
    }
    pub(crate) fn register(&mut self, item: ToolbarItemConfig) -> Result<(), &'static str> {
        if self
            .items
            .iter()
            .any(|registered| registered.id == item.id || registered.command == item.command)
        {
            return Err("toolbar item id is already registered");
        }
        self.items.push(item);
        Ok(())
    }
}

/// Canvas-owned overlay; tool configuration follows the canvas when docked.
#[derive(gpui_kit::IntoElement)]
pub(super) struct CanvasToolbar {
    pub owner: Entity<EditorView>,
    pub canvas: Entity<super::ViewportSurface>,
    pub modal: bool,
    pub items: Vec<ToolbarItemConfig>,
}

impl RenderOnce for CanvasToolbar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let Self {
            owner,
            canvas,
            modal,
            items,
        } = self;
        let view = owner.read(cx);
        // Opaque tool surfaces stay distinct from both the canvas and model pixels.
        let crate::theme::ToolbarPalette {
            surface,
            hovered,
            pressed,
        } = crate::theme::toolbar_colors(cx);
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
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .child(compact::group(
                "canvas-tools",
                t!("editor.panel_toolbar_actions"),
                Axis::Vertical,
                items
                    .iter()
                    .map(|item| {
                        let command = item.command;
                        let invocation = canvas.read(cx).command(
                            &view.editor,
                            command,
                            zipzi_editor_core::CommandSource::Toolbar,
                        );
                        let state = view.editor.command_state(invocation);
                        let selected = state.checked;
                        let enabled = !modal && state.enabled;
                        let owner = owner.clone();
                        let canvas = canvas.clone();
                        compact::button(Button::new(item.id))
                            .when(item.command != CommandId::GenerateMesh, |button| {
                                compact::toggle(button, selected)
                            })
                            .small()
                            .w(compact::rem(TOOL_BUTTON_SIZE))
                            .h(compact::rem(TOOL_BUTTON_SIZE))
                            .min_w(compact::rem(TOOL_BUTTON_SIZE))
                            .when(!selected, |button| {
                                button
                                    .custom(
                                        ButtonCustomVariant::new(cx)
                                            // Kit derives hover/pressed borders from this color.
                                            // The opaque idle fill is supplied below separately.
                                            .color(cx.theme().border)
                                            .hover(hovered)
                                            .active(pressed)
                                            .foreground(cx.theme().foreground),
                                    )
                                    .border_color(cx.theme().border)
                                    .bg(surface)
                            })
                            .border_1()
                            .icon(item.icon)
                            .accessibility_label(t!(item.command.spec().label).into_owned())
                            .tooltip(t!(item.command.spec().label))
                            .disabled(!enabled)
                            .on_click(move |_, window, cx| {
                                let id = canvas.entity_id();
                                let focus = canvas.read(cx).focus.clone();
                                window.focus(&focus, cx);
                                owner.update(cx, |view, _| {
                                    if let Some(dock) = &mut view.dock {
                                        dock.activate_viewport(id);
                                    }
                                });
                                owner.update(cx, |view, cx| {
                                    view.run_invocation(invocation, window, cx)
                                });
                            })
                    })
                    .collect::<Vec<_>>(),
            ))
    }
}
