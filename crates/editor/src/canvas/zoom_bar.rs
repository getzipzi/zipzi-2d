use crate::{EditorView, t, ui::compact};
use editor_core::CommandId;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Disableable, WindowExt,
    button::{Button, ButtonVariants},
    toolbar::{Toolbar, ToolbarGroup},
};
use gpui_kit::{prelude::*, *};
use std::rc::Rc;

const ZOOM_ACTIONS: [(&str, CommandId, IconName); 3] = [
    ("btn_zoom_out", CommandId::ZoomOut, IconName::Minus),
    ("btn_zoom_in", CommandId::ZoomIn, IconName::Plus),
    ("btn_zoom_fit", CommandId::FitView, IconName::Maximize),
];

type ZoomAction = Rc<dyn Fn(CommandId, &mut Window, &mut App)>;

pub(super) fn state(
    owner: &EditorView,
    canvas: &super::CanvasView,
    modal: bool,
) -> (i32, [bool; 3]) {
    (
        (canvas
            .binding(&owner.editor)
            .and_then(|binding| owner.editor.viewport_view(binding))
            .unwrap_or_default()
            .zoom
            * 100.0)
            .round() as i32,
        ZOOM_ACTIONS.map(|(_, command, _)| {
            !modal
                && owner.editor.command_available(canvas.command(
                    &owner.editor,
                    command,
                    editor_core::CommandSource::Toolbar,
                ))
        }),
    )
}

/// One framed control: zoom steps and readout share a surface, fit is its trailing segment.
#[derive(gpui_kit::IntoElement)]
struct ZoomControl {
    percentage: i32,
    enabled: [bool; 3],
    on_action: ZoomAction,
}

impl RenderOnce for ZoomControl {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let focus_fill = cx.theme().primary.opacity(0.2);
        let mut group = ToolbarGroup::new("canvas-zoom-group")
            .label(t!("dock.zoom_controls"))
            .h_full()
            .gap_0();
        for (index, (id, command, icon)) in ZOOM_ACTIONS.into_iter().enumerate() {
            let on_action = self.on_action.clone();
            group = group.content(
                compact::header_button(Button::new(id))
                    .ghost()
                    .h_full()
                    .w(compact::rem(compact::HEADER_CONTROL))
                    .px_0()
                    .border_0()
                    .rounded_tl(px(0.))
                    .rounded_tr(px(0.))
                    .rounded_bl(px(0.))
                    .rounded_br(px(0.))
                    .when(command == CommandId::FitView, |segment| {
                        segment.border_l_1().border_color(cx.theme().border)
                    })
                    .focus(move |style| style.bg(focus_fill))
                    .icon(icon)
                    .accessibility_label(t!(command.spec().label).into_owned())
                    .tooltip(t!(command.spec().label))
                    .disabled(!self.enabled[index])
                    .on_click(move |_, window, cx| on_action(command, window, cx)),
            );
            if index == 0 {
                group = group.content(
                    div()
                        .id("zoom-percentage")
                        .test_support()
                        .h_full()
                        .min_w(compact::rem(40.))
                        .px(compact::rem(compact::GAP))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(compact::rem(compact::CAPTION))
                        .font_family(".AppleSystemUIFontMonospaced")
                        .child(format!("{}%", self.percentage)),
                );
            }
        }
        div()
            .id("canvas-zoom-controls")
            .test_support()
            .h(compact::rem(compact::HEADER_CONTROL))
            .flex_shrink_0()
            .flex()
            .items_center()
            .rounded(compact::rem(compact::RADIUS))
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().button)
            .overflow_hidden()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                Toolbar::new("canvas-zoom-actions")
                    .h_full()
                    .p_0()
                    .gap_0()
                    .content(group),
            )
    }
}

/// Header controls belong to the same canvas as the overlay tools.
#[derive(gpui_kit::IntoElement)]
pub(super) struct CanvasZoomBar {
    pub owner: Entity<EditorView>,
    pub canvas: Entity<super::CanvasView>,
    pub modal: bool,
}

impl RenderOnce for CanvasZoomBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = self.owner.read(cx);
        let (percentage, enabled) = self::state(state, self.canvas.read(cx), self.modal);
        let commands = ZOOM_ACTIONS.map(|(_, id, _)| {
            self.canvas
                .read(cx)
                .command(&state.editor, id, editor_core::CommandSource::Toolbar)
        });
        let owner = self.owner;
        let canvas = self.canvas;
        let on_action: ZoomAction = Rc::new(move |command, window, cx| {
            if window.has_active_dialog(cx) {
                return;
            }
            let Some(invocation) = commands
                .iter()
                .find(|invocation| invocation.id == command)
                .copied()
            else {
                return;
            };
            let state = owner.read(cx);
            if state.modal || !state.editor.command_available(invocation) {
                return;
            }
            let focus = canvas.read(cx).focus.clone();
            window.focus(&focus, cx);
            owner.update(cx, |owner, cx| {
                if let Some(dock) = &mut owner.dock {
                    dock.activate_canvas(canvas.entity_id());
                }
                owner.run_invocation(invocation, window, cx);
            });
        });
        div()
            .id("canvas-zoom-toolbar")
            .test_support()
            .w_full()
            .h_full()
            .min_w_0()
            .flex()
            .items_center()
            .justify_end()
            .text_color(cx.theme().muted_foreground)
            .child(ZoomControl {
                percentage,
                enabled,
                on_action,
            })
    }
}
