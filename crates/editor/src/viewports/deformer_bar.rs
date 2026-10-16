use crate::{t, ui::compact, viewports::ViewportSurface};
use editor_core::{CommandId, CommandSource, EditorCommand, ObjectKind};
use gpui_kit::assets::IconName;
use gpui_kit::component::{Disableable, WindowExt, button::Button};
use gpui_kit::{prelude::*, *};

pub fn render_deformer_bar(
    canvas: &ViewportSurface,
    modal: bool,
    cx: &mut Context<ViewportSurface>,
) -> AnyElement {
    let Some(owner) = canvas.owner.upgrade() else {
        return div().into_any_element();
    };
    let state = owner.read(cx);
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
                c.kind == editor_core::ControlKind::Warp
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
    div()
        .id("deformer-actions")
        .absolute()
        .top(compact::rem(8.))
        .left(compact::rem(48.))
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
        .child(compact::group(
            "deformer-tools",
            t!("editor.deformer_grid"),
            Axis::Horizontal,
            buttons,
        ))
        .into_any_element()
}
