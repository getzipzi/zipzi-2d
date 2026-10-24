use crate::{EditorView, t, ui::compact};
use gpui_kit::component::{
    ActiveTheme, Disableable, IconName,
    button::{Button, ButtonVariants},
    menu::DropdownMenu,
};
use gpui_kit::{prelude::*, *};
use zipzi_editor_core::{CommandId, WorkspaceKind};

pub(super) const WORKSPACE_SWITCHER_HEIGHT: f32 = 24.;
pub(super) const WORKSPACE_SWITCHER_WIDTH: f32 = 160.;

pub(super) fn render_more_menu(owner: &Entity<EditorView>, modal: bool) -> impl IntoElement {
    compact::button(Button::new("workspace-layout-menu"))
        .ghost()
        .icon(IconName::Ellipsis)
        .accessibility_label(t!("dock.layout_actions"))
        .tooltip(t!("dock.layout_actions"))
        .disabled(modal)
        .dropdown_menu({
            let owner = owner.downgrade();
            move |mut menu, window, cx| {
                menu = compact::menu(menu);
                let node = owner.upgrade().and_then(|view| {
                    let dock = view.read(cx).dock.as_ref()?;
                    dock.canvas_node(cx)
                });
                if let Some(node) = node {
                    menu = crate::docking::add_panel::menu(
                        menu,
                        owner.clone(),
                        node,
                        false,
                        window,
                        cx,
                    )
                    .separator();
                }
                let selected_theme = crate::theme::current(cx);
                let theme_owner = owner.clone();
                menu = menu.submenu(
                    t!("editor.color_theme"),
                    window,
                    cx,
                    move |mut menu, _, _| {
                        for choice in crate::theme::ColorTheme::ALL {
                            let owner = theme_owner.clone();
                            menu = menu.item(
                                gpui_kit::component::menu::PopupMenuItem::new(choice.label())
                                    .checked(choice == selected_theme)
                                    .on_click(move |_, _, cx| {
                                        if let Err(error) = crate::theme::set_preference(choice, cx)
                                        {
                                            _ = owner.update(cx, |owner, cx| {
                                                owner.set_edit_error(error.to_string(), cx);
                                            });
                                        }
                                    }),
                            );
                        }
                        menu
                    },
                );
                let scale = cx.global::<crate::settings::Settings>().ui_scale;
                let scale_owner = owner.clone();
                menu = menu
                    .submenu(t!("editor.ui_scale"), window, cx, move |mut menu, _, _| {
                        for choice in crate::settings::UiScale::ALL {
                            let owner = scale_owner.clone();
                            menu = menu.item(
                                gpui_kit::component::menu::PopupMenuItem::new(choice.label())
                                    .checked(choice == scale)
                                    .on_click(move |_, window, cx| {
                                        let result = cx
                                            .update_global::<crate::settings::Settings, _>(
                                                |settings, _| settings.set_ui_scale(choice),
                                            );
                                        if let Err(error) = result {
                                            _ = owner.update(cx, |owner, cx| {
                                                owner.set_edit_error(error.to_string(), cx);
                                            });
                                            return;
                                        }
                                        crate::theme::change(crate::theme::current(cx), cx);
                                        window.refresh();
                                    }),
                            );
                        }
                        menu
                    })
                    .separator();
                let reset_owner = owner.clone();
                menu.item(
                    gpui_kit::component::menu::PopupMenuItem::element(|_, _| {
                        div()
                            .id("workspace-reset-layout")
                            .test_support()
                            .child(t!("menu.reset_layout"))
                    })
                    .on_click(move |_, window, cx| {
                        _ = reset_owner.update(cx, |owner, cx| {
                            owner.run_command(CommandId::ResetLayout, window, cx)
                        });
                    }),
                )
            }
        })
        .anchor(Anchor::TopRight)
}

pub(super) fn render_workspace_switcher(
    view: &EditorView,
    owner: &Entity<EditorView>,
    modal: bool,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    use gpui_kit::base::{Tab, Tabs, spring};

    // Two segments with a 2px outer inset and a 2px gap.
    const SEGMENT_WIDTH: f32 = (WORKSPACE_SWITCHER_WIDTH - 6.) / 2.;
    const SEGMENT_STEP: f32 = SEGMENT_WIDTH + 2.;
    let workspace = view.editor.workspace();
    let modes = [
        (
            WorkspaceKind::Modeling,
            CommandId::ModelingWorkspace,
            "workspace-modeling",
        ),
        (
            WorkspaceKind::Animation,
            CommandId::AnimationWorkspace,
            "workspace-animation",
        ),
    ]
    .map(|(kind, command, id)| {
        let enabled = !modal && view.editor.command_available(view.editor.command(command));
        (kind, command, id, enabled)
    });
    let disabled = modes.iter().all(|(_, _, _, enabled)| !enabled);
    let foreground = cx.theme().foreground;
    let muted = cx.theme().muted_foreground;
    let dark = cx.theme().is_dark();
    let indicator_left = spring(
        "workspace-mode-indicator",
        compact::scaled(2. + workspace.index() as f32 * SEGMENT_STEP, cx),
        cx.theme().motion_tokens().spring_move,
        window,
        cx,
    );

    div()
        .id("workspace-switcher")
        .test_support()
        .flex()
        .flex_shrink_0()
        .items_center()
        .child(
            Tabs::new("workspace-mode-tabs")
                .relative()
                .flex()
                .items_center()
                .w(crate::ui::compact::rem(WORKSPACE_SWITCHER_WIDTH))
                .h(crate::ui::compact::rem(WORKSPACE_SWITCHER_HEIGHT))
                .p(crate::ui::compact::rem(2.))
                .gap(crate::ui::compact::rem(2.))
                .rounded(crate::ui::compact::rem(compact::RADIUS))
                .border_0()
                .when(disabled, |tabs| tabs.opacity(0.5))
                .child(
                    div()
                        .absolute()
                        .left(indicator_left)
                        .top(crate::ui::compact::rem(2.))
                        .w(crate::ui::compact::rem(SEGMENT_WIDTH))
                        .h(crate::ui::compact::rem(20.))
                        .rounded(crate::ui::compact::rem(2.))
                        .bg(foreground.opacity(if dark { 0.08 } else { 0.07 })),
                )
                .children(modes.into_iter().map(|(kind, command, id, enabled)| {
                    let selected = workspace == kind;
                    let owner = owner.clone();
                    Tab::new(id)
                        .relative()
                        .flex_1()
                        .h_full()
                        .rounded(crate::ui::compact::rem(2.))
                        .text_xs()
                        .text_color(if selected { foreground } else { muted })
                        .font_weight(if selected {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .selected(selected)
                        .disabled(!enabled)
                        .accessibility_label(t!(command.spec().label))
                        .set_position(kind.index() + 1, WorkspaceKind::ALL.len())
                        .when(!selected && enabled, |tab| {
                            tab.cursor_pointer().hover(|style| {
                                style.bg(foreground.opacity(0.06)).text_color(foreground)
                            })
                        })
                        .child(t!(command.spec().label))
                        .on_click(move |_, window, cx| {
                            owner.update(cx, |view, cx| view.run_command(command, window, cx));
                        })
                })),
        )
}
