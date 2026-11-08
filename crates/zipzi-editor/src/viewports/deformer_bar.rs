use super::toolbar::SECONDARY;
use crate::{t, ui::compact, viewports::ViewportSurface};
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Sizable, WindowExt, button::Button};
use gpui_kit::{prelude::*, *};
use zipzi_editor_core::{CommandId, CommandSource, EditorCommand, ObjectKind};

pub fn render_deformer_bar(
    canvas: &ViewportSurface,
    modal: bool,
    cx: &mut Context<ViewportSurface>,
) -> AnyElement {
    let Some(owner) = canvas.owner.upgrade() else {
        return div().into_any_element();
    };
    let state = owner.read(cx);
    if state
        .mesh_generation
        .as_ref()
        .is_some_and(|panel| panel.read(cx).viewport == canvas.viewport)
    {
        return div().into_any_element();
    }
    if state.editor.manual_for_viewport(canvas.viewport) {
        return render_mesh_edit_bar(canvas, modal, cx);
    }
    if state.editor.active_tool() == zipzi_editor_core::ToolId::BrushSelect {
        return canvas.brush_bar.clone().into_any_element();
    }
    if !state.editor.document().is_some_and(|d| {
        d.context()
            .selections
            .iter()
            .any(|r| r.kind == ObjectKind::Deformer)
    }) {
        return div().into_any_element();
    }
    let Some(binding) = canvas.binding(&state.editor) else {
        return div().into_any_element();
    };
    let ancestors = state.editor.command_for_viewport(
        CommandId::ToggleAncestorDeformers,
        binding,
        CommandSource::Toolbar,
    );
    let visibility = state.editor.deformer_visibility();
    let grid = state.editor.deformer_grid();
    let mut buttons = Vec::new();
    buttons.push(
        compact::toggle(Button::new("deformer-ancestors"), visibility.ancestors)
            .icon(IconName::Layers)
            .accessibility_label(t!("editor.show_ancestor_deformers").into_owned())
            .tooltip(t!("editor.show_ancestor_deformers"))
            .disabled(modal)
            .on_click(cx.listener(move |this, _, window, cx| {
                if let Some(owner) = this.owner.upgrade() {
                    owner.update(cx, |owner, cx| owner.run_invocation(ancestors, window, cx));
                }
            })),
    );
    buttons.push(
        compact::toggle(Button::new("deformer-grid"), grid)
            .icon(IconName::Grid2x2)
            .accessibility_label(t!("editor.deformer_grid").into_owned())
            .tooltip(t!("editor.deformer_grid"))
            .disabled(modal)
            .on_click(cx.listener(move |this, _, window, cx| {
                if window.has_active_dialog(cx) {
                    return;
                }
                if let Some(owner) = this.owner.upgrade() {
                    owner.update(cx, |owner, cx| {
                        if owner.modal || !owner.editor.viewport_binding_valid(binding) {
                            return;
                        }
                        let result = owner.editor.execute(EditorCommand::ToggleDeformerGrid);
                        owner.complete_edit(result, cx);
                    });
                }
            })),
    );
    buttons.push(
        compact::button(Button::new("deformer-help"))
            .icon(IconName::Info)
            .accessibility_label(t!("editor.deformer_gesture_hint").into_owned())
            .tooltip(t!("editor.deformer_gesture_hint")),
    );
    let mut tools = gpui_kit::component::toolbar::ToolbarGroup::new("deformer-tools")
        .label(t!("editor.deformer_grid"))
        .min_w_0()
        .max_w_full()
        .flex_wrap()
        .justify_center()
        .gap(compact::rem(1.));
    for button in buttons {
        tools = tools.content(
            super::toolbar::attached_button(button, SECONDARY, cx)
                .xsmall()
                .flex_shrink_0()
                .h(compact::rem(SECONDARY.button_size))
                .min_w(compact::rem(SECONDARY.button_size))
                .px(compact::rem(3.)),
        );
    }
    super::toolbar::secondary_bar("deformer-actions", tools, cx)
}

fn render_mesh_edit_bar(
    canvas: &ViewportSurface,
    modal: bool,
    cx: &mut Context<ViewportSurface>,
) -> AnyElement {
    let owner = canvas.owner.upgrade().unwrap();
    let state = owner.read(cx);
    let mut tools = gpui_kit::component::toolbar::ToolbarGroup::new("mesh-edit-tools")
        .label(t!("editor.tool_manual_mesh"))
        .gap(compact::rem(1.));
    let targets = state.editor.manual_targets();
    if targets.len() > 1 {
        let active = state.editor.manual_target().unwrap();
        let name = state
            .editor
            .layers()
            .into_iter()
            .find(|mesh| mesh.id == active.object.id)
            .map_or(active.object.id, |mesh| mesh.name);
        let tooltip = t!("editor.manual_multi_target", name = name).into_owned();
        tools = tools.content(
            div()
                .id("mesh-edit-active-target")
                .test_support()
                .aria_label(name.clone())
                .text_size(compact::rem(11.))
                .text_color(cx.theme().foreground)
                .flex_shrink_0()
                .max_w(compact::rem(120.))
                .truncate()
                .px(compact::rem(4.))
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .child(name),
        );
    }
    for (id, icon, command) in [
        (
            "tool_manual_select",
            Icon::new(IconName::SquareDashed),
            CommandId::ManualSelect,
        ),
        (
            "tool_manual_lasso",
            Icon::new(IconName::Lasso),
            CommandId::ManualLasso,
        ),
        (
            "tool_generate_mesh",
            Icon::default().data(include_bytes!("../../assets/icons/generate-mesh.svg")),
            CommandId::GenerateMesh,
        ),
        (
            "tool_manual_reconnect",
            Icon::default().data(include_bytes!("../../assets/icons/mesh-retriangulate.svg")),
            CommandId::ManualReconnect,
        ),
        (
            "tool_manual_pad_source",
            Icon::default().data(include_bytes!("../../assets/icons/mesh-pad-source.svg")),
            CommandId::ManualPadSource,
        ),
        (
            "tool_manual_source",
            Icon::default().data(include_bytes!("../../assets/icons/mesh-source-view.svg")),
            CommandId::ManualSourceView,
        ),
    ] {
        let invocation = canvas.command(&state.editor, command, CommandSource::Toolbar);
        let status = state.editor.command_state(invocation);
        let label = command.spec().label;
        let canvas = cx.entity();
        let owner = owner.clone();
        let generating = state.mesh_generation.is_some() && command == CommandId::GenerateMesh;
        let button = compact::button(Button::new(id))
            .when(generating, |button| compact::toggle(button, true))
            .when(
                matches!(
                    command,
                    CommandId::ManualSourceView | CommandId::ManualSelect | CommandId::ManualLasso
                ),
                |button| compact::toggle(button, status.checked),
            )
            .icon(icon)
            .accessibility_label(t!(label).into_owned())
            .tooltip(t!(match command {
                CommandId::ManualSelect => "editor.manual_box_select_hint",
                CommandId::ManualLasso => "editor.manual_lasso_hint",
                CommandId::ManualPadSource if targets.len() > 1 =>
                    "editor.manual_multi_pad_source_hint",
                CommandId::ManualPadSource => "editor.manual_pad_source_hint",
                CommandId::GenerateMesh if targets.len() > 1 => "editor.manual_multi_generate_hint",
                CommandId::ManualReconnect if targets.len() > 1 =>
                    "editor.manual_multi_reconnect_hint",
                CommandId::ManualSourceView if targets.len() > 1 =>
                    "editor.manual_multi_source_hint",
                _ => label,
            }))
            .disabled(modal || (!status.enabled && !generating))
            .on_click(move |_, window, cx| {
                let focus = canvas.read(cx).focus.clone();
                window.focus(&focus, cx);
                owner.update(cx, |owner, cx| {
                    if let Some(dock) = &mut owner.dock {
                        dock.activate_viewport(canvas.entity_id());
                    }
                    owner.run_invocation(invocation, window, cx);
                });
            });
        tools = tools.content(
            super::toolbar::attached_button(button, SECONDARY, cx)
                .xsmall()
                .w(compact::rem(SECONDARY.button_size))
                .h(compact::rem(SECONDARY.button_size))
                .min_w(compact::rem(SECONDARY.button_size))
                .px(compact::rem(3.)),
        );
    }
    super::toolbar::secondary_bar("mesh-edit-actions", tools, cx)
}
