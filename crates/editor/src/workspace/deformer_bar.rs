use crate::{canvas::CanvasView, t};
use editor_core::{EditorCommand, ObjectKind};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, Sizable,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::*, *};

pub fn render_deformer_bar(
    canvas: &CanvasView,
    modal: bool,
    cx: &mut Context<CanvasView>,
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
    let visibility = state.editor.deformer_visibility();
    let grid = state.editor.deformer_grid();
    let fine = state.editor.warp_fine();
    let has_warp = state.editor.document().is_some_and(|d| {
        state.frame.as_ref().is_some_and(|f| {
            f.canvas.geometry.controls.iter().any(|c| {
                c.kind == editor_core::ControlKind::Warp
                    && d.context().selections.contains(&c.object)
            })
        })
    });
    div()
        .id("deformer-actions")
        .absolute()
        .top_2()
        .left(px(52.))
        .flex()
        .items_center()
        .gap_1()
        .p_1()
        .rounded_md()
        .bg(cx.theme().background.opacity(0.95))
        .border_1()
        .border_color(cx.theme().border)
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Right, |_, _, cx| cx.stop_propagation())
        .on_mouse_down(MouseButton::Middle, |_, _, cx| cx.stop_propagation())
        .child(
            Button::new("deformer-ancestors")
                .small()
                .ghost()
                .icon(IconName::Layers)
                .when(visibility.ancestors, |b| b.primary())
                .tooltip(t!("editor.show_ancestor_deformers"))
                .disabled(modal)
                .on_click(cx.listener(|this, _, window, cx| {
                    if let Some(owner) = this.owner.upgrade() {
                        owner.update(cx, |owner, cx| {
                            owner.run_command(
                                editor_core::CommandId::ToggleAncestorDeformers,
                                window,
                                cx,
                            )
                        });
                    }
                })),
        )
        .when(has_warp, |bar| {
            bar.child(
                Button::new("deformer-warp-mode")
                    .small()
                    .ghost()
                    .label(if fine {
                        t!("editor.warp_fine")
                    } else {
                        t!("editor.warp_bezier")
                    })
                    .tooltip(t!("editor.warp_mode_hint"))
                    .disabled(modal)
                    .on_click(cx.listener(|this, _, _, cx| {
                        if let Some(owner) = this.owner.upgrade() {
                            owner.update(cx, |owner, cx| {
                                let result = owner.editor.execute(EditorCommand::ToggleWarpFine);
                                owner.complete_edit(result, cx);
                            });
                        }
                    })),
            )
        })
        .child(
            Button::new("deformer-grid")
                .small()
                .ghost()
                .icon(IconName::Grid2x2)
                .when(grid, |b| b.primary())
                .tooltip(t!("editor.deformer_grid"))
                .disabled(modal)
                .on_click(cx.listener(|this, _, _, cx| {
                    if let Some(owner) = this.owner.upgrade() {
                        owner.update(cx, |owner, cx| {
                            let result = owner.editor.execute(EditorCommand::ToggleDeformerGrid);
                            owner.complete_edit(result, cx);
                        });
                    }
                })),
        )
        .child(
            Button::new("deformer-help")
                .small()
                .ghost()
                .icon(IconName::Info)
                .tooltip(t!("editor.deformer_gesture_hint")),
        )
        .into_any_element()
}
