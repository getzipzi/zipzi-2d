use crate::{EditorView, t, ui::compact};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Selectable, Sizable,
    button::{Button, ButtonCustomVariant, ButtonVariants},
};
use gpui_kit::{prelude::*, *};
use zipzi_editor_core::CommandId;

mod notch;
pub(super) use notch::{Notch, PRIMARY, SECONDARY};

/// Shared viewport appearance; callers retain their existing button dimensions.
pub(super) fn button_style(button: Button, cx: &App) -> Button {
    let selected = button.is_selected();
    let crate::theme::ToolbarPalette {
        surface,
        hovered,
        pressed,
    } = crate::theme::toolbar_colors(cx);
    button
        .when(!selected, |button| {
            button
                .custom(
                    ButtonCustomVariant::new(cx)
                        .color(cx.theme().border)
                        .hover(hovered.opacity(0.95))
                        .active(pressed.opacity(0.95))
                        .foreground(cx.theme().foreground),
                )
                .border_color(cx.theme().border)
                .bg(surface.opacity(0.85))
        })
        .when(selected, |button| {
            button.bg(cx.theme().button_primary.opacity(0.85))
        })
        .border_1()
}

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
                id: "tool_manual_mesh",
                icon: IconName::GitBranch,
                command: CommandId::ManualMesh,
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

pub(super) fn attached_button(button: Button, shape: Notch, cx: &App) -> Button {
    let selected = button.is_selected();
    button_style(button, cx)
        .when(!selected, |button| button.bg(transparent_black()))
        .border_0()
        .rounded(compact::scaled(shape.button_radius, cx))
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
        let mut tools = gpui_kit::component::toolbar::ToolbarGroup::new("canvas-tools")
            .label(t!("editor.panel_toolbar_actions"))
            .flex_col()
            .gap(compact::rem(1.));
        for item in items {
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
            tools = tools.content(
                compact::toggle(Button::new(item.id), selected)
                    .xsmall()
                    .w(compact::rem(PRIMARY.button_size))
                    .h(compact::rem(PRIMARY.button_size))
                    .min_w(compact::rem(PRIMARY.button_size))
                    .map(|button| attached_button(button, PRIMARY, cx))
                    .icon(item.icon)
                    .accessibility_label(t!(command.spec().label).into_owned())
                    .tooltip(t!(match command {
                        CommandId::SelectTool => "editor.tool_select_hint",
                        CommandId::LassoTool => "editor.tool_lasso_hint",
                        CommandId::ManualMesh => "editor.manual_mesh_edit_hint",
                        _ => command.spec().label,
                    }))
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
                        owner.update(cx, |view, cx| view.run_invocation(invocation, window, cx));
                    }),
            );
        }
        div()
            .id("canvas-toolbar")
            .test_support()
            .absolute()
            .top(compact::rem(8.))
            .left_0()
            .px(compact::rem(PRIMARY.inset))
            .py(compact::rem(PRIMARY.end_padding()))
            .occlude()
            .flex()
            .flex_col()
            .items_center()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
            .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
            .child(PRIMARY.background(cx))
            .child(tools)
    }
}

/// The shared top-attached secondary toolbar, used by each editing context.
pub(super) fn secondary_bar(id: &'static str, content: impl IntoElement, cx: &App) -> AnyElement {
    div()
        .absolute()
        .top_0()
        .left_0()
        .w_full()
        .flex()
        .justify_center()
        .child(
            div()
                .id(id)
                .test_support()
                .relative()
                .px(compact::rem(SECONDARY.end_padding()))
                .py(compact::rem(SECONDARY.inset))
                .occlude()
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
                .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
                .child(SECONDARY.background(cx))
                .child(content),
        )
        .into_any_element()
}
