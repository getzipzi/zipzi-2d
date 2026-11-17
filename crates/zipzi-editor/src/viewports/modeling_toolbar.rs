use super::toolbar::SECONDARY;
use crate::{t, ui::compact, viewports::ViewportSurface};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, Sizable,
    button::Button,
    separator::Separator,
    toolbar::{Toolbar, ToolbarGroup},
};
use gpui_kit::{prelude::*, *};
use zipzi_editor_core::{CommandId, CommandSource, ToolId};

pub(super) fn render_secondary(
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
        return render_mesh_edit_bar(canvas, &owner, modal, cx);
    }
    // Glue tool first exposes the canvas labels. Brush controls belong to the
    // selected Glue and must not cover the labels before a target is chosen.
    if state.editor.active_tool() == ToolId::Glue && !state.editor.glue_editing_weights() {
        return div().into_any_element();
    }
    if matches!(
        state.editor.active_tool(),
        ToolId::BrushSelect | ToolId::Glue
    ) {
        return canvas.brush_bar.clone().into_any_element();
    }
    div().into_any_element()
}

fn render_mesh_edit_bar(
    canvas: &ViewportSurface,
    owner: &Entity<crate::EditorView>,
    modal: bool,
    cx: &mut Context<ViewportSurface>,
) -> AnyElement {
    let state = owner.read(cx);
    let mut tools = ToolbarGroup::new("mesh-edit-tools")
        .label(t!("editor.tool_manual_mesh"))
        .gap(compact::rem(1.));
    let targets = state.editor.manual_targets();
    if targets.len() > 1 {
        let active = state.editor.manual_target().unwrap();
        let index = state.editor.manual_color_index(&active.object).unwrap();
        let name = state
            .editor
            .layers()
            .into_iter()
            .find(|mesh| mesh.id == active.object.id)
            .map_or(active.object.id, |mesh| mesh.name);
        let tooltip = t!("editor.manual_multi_target", name = name).into_owned();
        let [r, g, b, a] = zipzi_editor_render::canvas_overlay::mesh_edit_color(index);
        tools = tools.content(
            div()
                .id("mesh-edit-active-target")
                .test_support()
                .aria_label(name.clone())
                .flex()
                .items_center()
                .gap(compact::rem(4.))
                .text_size(compact::rem(11.))
                .text_color(cx.theme().foreground)
                .flex_shrink_0()
                .max_w(compact::rem(120.))
                .truncate()
                .px(compact::rem(4.))
                .tooltip(move |window, cx| {
                    gpui_kit::component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .child(
                    div()
                        .size(compact::rem(6.))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(gpui_kit::Rgba { r, g, b, a }),
                )
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
            .tooltip(crate::i18n::shortcut_hint(match command {
                CommandId::ManualSelect => "editor.manual_box_select_hint",
                CommandId::ManualLasso => "editor.manual_lasso_hint",
                CommandId::ManualPadSource if targets.len() > 1 => {
                    "editor.manual_multi_pad_source_hint"
                }
                CommandId::ManualPadSource => "editor.manual_pad_source_hint",
                CommandId::GenerateMesh if targets.len() > 1 => "editor.manual_multi_generate_hint",
                CommandId::ManualReconnect if targets.len() > 1 => {
                    "editor.manual_multi_reconnect_hint"
                }
                CommandId::ManualSourceView if targets.len() > 1 => {
                    "editor.manual_multi_source_hint"
                }
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
        tools = tools.content(super::toolbar::secondary_icon_button(button, cx));
    }
    let binding = canvas.binding(&state.editor);
    let mut content = Toolbar::new("mesh-edit-toolbar")
        .xsmall()
        .disabled(modal)
        .min_w_0()
        .min_h(compact::rem(SECONDARY.button_size))
        .max_w_full()
        .p_0()
        .gap(compact::rem(4.))
        .content(tools);
    if binding.is_some_and(|b| state.editor.manual_glue_controls(b).is_some()) {
        let mut glue = ToolbarGroup::new("mesh-glue-tools")
            .label("Glue")
            .gap(compact::rem(1.));
        for (id, command, icon) in [
            (
                "mesh-glue-add-point",
                CommandId::GlueInsertPoint,
                include_bytes!("../../assets/icons/glue-add-point.svg").as_slice(),
            ),
            (
                "mesh-glue-bind",
                CommandId::GlueBind,
                include_bytes!("../../assets/icons/glue-bind.svg").as_slice(),
            ),
            (
                "mesh-glue-unbind",
                CommandId::GlueUnbind,
                include_bytes!("../../assets/icons/glue-unbind.svg").as_slice(),
            ),
            (
                "mesh-glue-weights",
                CommandId::GlueWeights,
                include_bytes!("../../assets/icons/glue-weights.svg").as_slice(),
            ),
        ] {
            let invocation = canvas.command(&state.editor, command, CommandSource::Toolbar);
            let status = state.editor.command_state(invocation);
            let label = t!(command.spec().label).into_owned();
            let tooltip = match command {
                CommandId::GlueBind => format!("{label}\n{}", t!("editor.glue_bind_pair_hint")),
                CommandId::GlueWeights if !status.enabled => {
                    format!("{label}\n{}", t!("editor.glue_connect_first"))
                }
                _ => label.clone(),
            };
            let owner = owner.clone();
            let focus = canvas.focus.clone();
            let button = compact::button(Button::new(id))
                .when(command == CommandId::GlueInsertPoint, |button| {
                    compact::toggle(button, status.checked)
                })
                .icon(Icon::default().data(icon))
                .accessibility_label(label)
                .tooltip(tooltip)
                .disabled(modal || !status.enabled)
                .on_click(move |_, window, cx| {
                    window.focus(&focus, cx);
                    owner.update(cx, |owner, cx| owner.run_invocation(invocation, window, cx));
                });
            glue = glue.content(super::toolbar::secondary_icon_button(button, cx));
        }
        content = content
            .content(Separator::vertical().h(compact::rem(16.)))
            .content(glue);
    }
    let bar = super::toolbar::secondary_bar("mesh-edit-actions", content, cx);
    if targets.len() != 2 {
        return bar;
    }
    let primary = super::toolbar::PRIMARY;
    div()
        .absolute()
        .top_0()
        .left(compact::rem(primary.button_size + 2. * primary.inset + 2.))
        .right_0()
        .child(bar)
        .into_any_element()
}

/// Weight editing can return directly to Mesh Edit.
pub(super) fn glue_navigation(
    owner: &Entity<crate::EditorView>,
    binding: zipzi_editor_core::ViewportBinding,
    modal: bool,
    cx: &App,
) -> AnyElement {
    let editor = &owner.read(cx).editor;
    let invocation =
        editor.command_for_viewport(CommandId::GlueEditStitches, binding, CommandSource::Toolbar);
    let enabled = editor.command_state(invocation).enabled;
    let owner = owner.clone();
    let button = compact::button(Button::new("glue-tab-points"))
        .icon(Icon::default().data(include_bytes!("../../assets/icons/mesh-edit.svg")))
        .accessibility_label(t!("editor.glue_edit_points").into_owned())
        .tooltip(t!("editor.glue_edit_points").into_owned())
        .disabled(modal || !enabled)
        .on_click(move |_, window, cx| {
            owner.update(cx, |owner, cx| {
                if let Some(canvas) = owner.viewport_surface(binding.viewport, cx) {
                    window.focus(&canvas.read(cx).focus.clone(), cx);
                }
                owner.run_invocation(invocation, window, cx);
            });
        });
    div()
        .id("glue-mode-tabs")
        .test_support()
        .flex()
        .items_center()
        .flex_shrink_0()
        .child(super::toolbar::secondary_icon_button(button, cx))
        .into_any_element()
}
