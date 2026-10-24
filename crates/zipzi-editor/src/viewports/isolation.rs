//! Modeling-only title bar isolation toggle and context display options.
use super::*;
use crate::ui::compact;
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    Disableable,
    button::Button,
    menu::{DropdownMenu, PopupMenuItem},
    toolbar::ToolbarGroup,
};
use zipzi_editor_core::{IsolationDisplay, ViewportBinding};

#[derive(Clone, Copy, PartialEq)]
pub(super) struct IsolationHeaderState {
    binding: Option<ViewportBinding>,
    active: bool,
    enabled: bool,
    display: IsolationDisplay,
}
pub(super) fn state(
    owner: &EditorView,
    canvas: &ViewportSurface,
    modal: bool,
) -> Option<IsolationHeaderState> {
    (canvas.kind == zipzi_editor_core::ViewportKind::Modeling).then(|| {
        let binding = canvas.binding(&owner.editor);
        IsolationHeaderState {
            binding,
            active: binding.is_some_and(|b| owner.editor.viewport_isolated(b)),
            enabled: !modal
                && binding.is_some_and(|b| {
                    owner
                        .editor
                        .command_available(owner.editor.command_for_viewport(
                            zipzi_editor_core::CommandId::ToggleViewportIsolation,
                            b,
                            zipzi_editor_core::CommandSource::Direct,
                        ))
                }),
            display: binding.map_or(IsolationDisplay::Dim, |b| {
                owner.editor.viewport_isolation_display(b)
            }),
        }
    })
}

fn apply(
    owner: &Entity<EditorView>,
    canvas: &Entity<ViewportSurface>,
    binding: ViewportBinding,
    display: Option<IsolationDisplay>,
    window: &mut Window,
    cx: &mut App,
) {
    if window.has_active_dialog(cx) || owner.read(cx).modal {
        return;
    }
    window.focus(&canvas.read(cx).focus.clone(), cx);
    owner.update(cx, |owner, cx| {
        if let Some(display) = display {
            let result = owner
                .editor
                .set_viewport_isolation_display(binding, display);
            owner.complete_edit(result, cx);
        } else {
            let command = owner.editor.command_for_viewport(
                zipzi_editor_core::CommandId::ToggleViewportIsolation,
                binding,
                zipzi_editor_core::CommandSource::Direct,
            );
            owner.run_invocation(command, window, cx);
        }
    });
}

pub(super) fn controls(
    owner: Entity<EditorView>,
    canvas: Entity<ViewportSurface>,
    header: IsolationHeaderState,
) -> AnyElement {
    let binding = header.binding;
    let toggle_owner = owner.clone();
    let toggle_canvas = canvas.clone();
    let label = if header.active {
        t!("dock.exit_isolation")
    } else {
        t!("dock.isolate_selection")
    };
    compact::header_control(
        ToolbarGroup::new("canvas-isolation-controls")
            .label(t!("dock.isolate_selection"))
            .gap_0()
            .content(
                compact::header_button(compact::toggle(
                    Button::new("btn_isolate_selection"),
                    header.active,
                ))
                .border_1()
                .rounded_tr(px(0.))
                .rounded_br(px(0.))
                .icon(IconName::Focus)
                .accessibility_label(label.clone().into_owned())
                .tooltip_with_action(
                    label,
                    &crate::actions::RunCommand::new(
                        zipzi_editor_core::CommandId::ToggleViewportIsolation,
                    ),
                    Some("ZipziShortcutDisplay"),
                )
                .disabled(!header.enabled)
                .on_click(move |_, window, cx| {
                    if let Some(binding) = binding {
                        apply(&toggle_owner, &toggle_canvas, binding, None, window, cx);
                    }
                }),
            )
            .content(
                compact::header_button(Button::new("popup"))
                    .border_1()
                    .ml(-px(1.))
                    .rounded_tl(px(0.))
                    .rounded_bl(px(0.))
                    .icon(IconName::ChevronDown)
                    .accessibility_label(t!("dock.isolation_display").into_owned())
                    .tooltip(t!("dock.isolation_display"))
                    .disabled(!header.enabled || !header.active)
                    .dropdown_menu(move |mut menu, _, _| {
                        menu = compact::menu(menu);
                        for (display, label) in [
                            (IsolationDisplay::Dim, "dock.isolation_dim"),
                            (IsolationDisplay::Hide, "dock.isolation_hide"),
                        ] {
                            let owner = owner.clone();
                            let canvas = canvas.clone();
                            menu = menu.item(
                                PopupMenuItem::new(t!(label).into_owned())
                                    .checked(header.display == display)
                                    .disabled(!header.active)
                                    .on_click(move |_, window, cx| {
                                        if let Some(binding) = binding {
                                            apply(
                                                &owner,
                                                &canvas,
                                                binding,
                                                Some(display),
                                                window,
                                                cx,
                                            );
                                        }
                                    }),
                            );
                        }
                        menu
                    }),
            ),
    )
    .into_any_element()
}
