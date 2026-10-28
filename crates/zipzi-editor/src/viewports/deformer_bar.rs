use super::toolbar::SECONDARY;
use crate::{t, ui::compact, viewports::ViewportSurface};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Disableable, Sizable, WindowExt, button::Button};
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
    if state.editor.manual_active() || state.mesh_generation.is_some() {
        return render_mesh_edit_bar(canvas, modal, cx);
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
    let fine = state.editor.warp_fine();
    let has_warp = state.editor.document().is_some_and(|d| {
        state.modeling_frame_for(canvas.viewport).is_some_and(|f| {
            f.canvas.geometry.controls.iter().any(|c| {
                c.kind == zipzi_editor_core::ControlKind::Warp
                    && d.context().selections.contains(&c.object)
            })
        })
    });
    let mut buttons = vec![
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
    ];
    if has_warp {
        buttons.push(
            compact::button(Button::new("deformer-warp-mode"))
                .label(if fine {
                    t!("editor.warp_fine")
                } else {
                    t!("editor.warp_bezier")
                })
                .accessibility_label(t!("editor.warp_mode_hint").into_owned())
                .tooltip(t!("editor.warp_mode_hint"))
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
                            let result = owner.editor.execute(EditorCommand::ToggleWarpFine);
                            owner.complete_edit(result, cx);
                        });
                    }
                })),
        );
    }
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
        .gap(compact::rem(1.));
    for button in buttons {
        tools = tools.content(
            super::toolbar::attached_button(button, SECONDARY, cx)
                .xsmall()
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
    for (id, icon, command) in [
        (
            "tool_generate_mesh",
            IconName::Grid2x2,
            CommandId::GenerateMesh,
        ),
        (
            "tool_manual_preview",
            IconName::Eye,
            CommandId::ManualPreviewResult,
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
            .when(command == CommandId::ManualPreviewResult, |button| {
                compact::toggle(button, status.checked)
            })
            .icon(icon)
            .accessibility_label(t!(label).into_owned())
            .tooltip(t!(label))
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
                .h(compact::rem(SECONDARY.button_size))
                .min_w(compact::rem(SECONDARY.button_size))
                .px(compact::rem(3.)),
        );
    }
    super::toolbar::secondary_bar("mesh-edit-actions", tools, cx)
}
