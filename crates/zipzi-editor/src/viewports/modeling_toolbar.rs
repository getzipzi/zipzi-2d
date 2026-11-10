use super::toolbar::SECONDARY;
use crate::{t, ui::compact, viewports::ViewportSurface};
use gpui_kit::assets::IconName;
use gpui_kit::component::{ActiveTheme, Disableable, Icon, Sizable, button::Button};
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
    if state.editor.stitch_for_viewport(canvas.viewport) {
        return render_stitch_bar(canvas, &owner, modal, cx);
    }
    if state.editor.manual_for_viewport(canvas.viewport) {
        return render_mesh_edit_bar(canvas, &owner, modal, cx);
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

fn render_stitch_bar(
    canvas: &ViewportSurface,
    owner: &Entity<crate::EditorView>,
    modal: bool,
    cx: &Context<ViewportSurface>,
) -> AnyElement {
    let state = owner.read(cx);
    let controls = canvas
        .binding(&state.editor)
        .and_then(|b| state.editor.stitch_controls(b));
    let hint = if let Some(c) = &controls {
        if state.editor.stitch_removing() {
            t!("editor.glue_remove_hint").into_owned()
        } else {
            t!("editor.glue_stitch_progress", count = c.pair_count).into_owned()
        }
    } else if state.editor.stitch_parts() == 0 {
        t!("editor.glue_pick_first").into_owned()
    } else {
        t!("editor.glue_pick_second").into_owned()
    };
    let title = controls.as_ref().map_or_else(
        || t!("editor.glue_stitch").into_owned(),
        |c| format!("{}  ↔  {}", c.meshes[0], c.meshes[1]),
    );
    let mut actions = div().flex().items_center().gap(compact::rem(6.));
    for (id, command, label, selected) in [
        (
            "glue-remove-points",
            CommandId::GlueUnbind,
            "editor.glue_remove_points",
            state.editor.stitch_removing(),
        ),
        (
            "glue-stitch-action",
            CommandId::GlueFinish,
            "editor.glue_done",
            true,
        ),
    ] {
        if command == CommandId::GlueUnbind && controls.is_none() {
            continue;
        }
        let invocation = canvas.command(&state.editor, command, CommandSource::Toolbar);
        let enabled = state.editor.command_state(invocation).enabled;
        let owner = owner.clone();
        let focus = canvas.focus.clone();
        actions = actions.child(
            compact::toggle(Button::new(id), selected)
                .label(t!(label).into_owned())
                .disabled(modal || !enabled)
                .on_click(move |_, window, cx| {
                    window.focus(&focus, cx);
                    owner.update(cx, |owner, cx| owner.run_invocation(invocation, window, cx));
                }),
        );
    }
    super::toolbar::secondary_bar(
        "glue-stitch-bar",
        div()
            .flex()
            .flex_col()
            .gap(compact::rem(6.))
            .p(compact::rem(6.))
            .max_w(compact::rem(440.))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(compact::rem(8.))
                    .child(Icon::new(IconName::Link).size(compact::rem(14.)))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(compact::rem(12.))
                            .child(title),
                    )
                    .child(actions),
            )
            .child(
                div()
                    .text_size(compact::rem(11.))
                    .text_color(cx.theme().muted_foreground)
                    .child(hint),
            ),
        cx,
    )
}
