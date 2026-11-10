use super::controls::{self, WORKSPACE_SWITCHER_HEIGHT, WORKSPACE_SWITCHER_WIDTH};
use crate::{EditorView, t, ui::compact};
use gpui_kit::component::{
    ActiveTheme, Disableable, IconName, Side, TitleBar,
    button::{Button, ButtonVariants},
    menu::{DropdownMenu, PopupMenuItem},
};
use gpui_kit::{prelude::*, *};
use zipzi_editor_core::CommandId;

/// Extend the client area into the native title bar while retaining macOS controls.
pub fn window_options() -> WindowOptions {
    WindowOptions {
        titlebar: Some(TitlebarOptions {
            title: Some(t!("app.title").into()),
            ..TitleBar::title_bar_options()
        }),
        ..TitleBar::window_options()
    }
}

pub fn render_title_bar(
    view: &EditorView,
    modal: bool,
    window: &mut Window,
    cx: &mut Context<EditorView>,
) -> impl IntoElement {
    let owner = cx.entity();
    let application_menu = super::application_menu::state(window, cx);
    application_menu.update(cx, |menu, _| menu.modal = modal);
    let menu_expanded = !cfg!(target_os = "macos") && application_menu.read(cx).expanded();
    // Kit reserves 80px for macOS traffic lights, otherwise a 12px inset.
    // Compensate for that inset so the segment stays at the window's center.
    let native_inset = px(if cfg!(target_os = "macos") && !window.is_fullscreen() {
        80.
    } else {
        12.
    });
    let left_width = (window.viewport_size().width - compact::scaled(WORKSPACE_SWITCHER_WIDTH, cx))
        / 2.
        - native_inset;
    let project_width =
        (left_width - compact::scaled(compact::PADDING + 12., cx)).min(compact::scaled(160., cx));
    let chrome = div()
        .absolute()
        .size_full()
        .flex()
        .items_center()
        .child(
            div()
                .w(left_width)
                .min_w_0()
                .h_full()
                .flex()
                .flex_shrink_0()
                .items_center()
                .gap(compact::rem(compact::PADDING))
                .when(!cfg!(target_os = "macos"), |left| {
                    left.child(
                        control_region("application-menu-control")
                            .min_w_0()
                            .flex_shrink_1()
                            .child(application_menu),
                    )
                })
                .when(!menu_expanded, |left| {
                    left.child(
                        control_region("project-select-control")
                            .w(project_width)
                            .min_w_0()
                            .child(render_project_select(view, modal, project_width, cx)),
                    )
                })
                .child(
                    div()
                        .id("window-titlebar-drag-region")
                        .test_support()
                        .h_full()
                        .min_w(compact::rem(12.))
                        .flex_1(),
                ),
        )
        .child(control_region("workspace-titlebar-switcher").child(
            controls::render_workspace_switcher(view, &owner, modal, window, cx),
        ))
        .child(div().flex_1().h_full().min_w_0())
        .when(crate::updates::ready(cx), |bar| {
            bar.child(
                control_region("workspace-update-control").child(
                    compact::button(Button::new("workspace-update-ready"))
                        .ghost()
                        .icon(IconName::RefreshCw)
                        .text_color(cx.theme().muted_foreground)
                        .accessibility_label(t!("updates.ready_hint"))
                        .tooltip(t!("updates.ready_hint"))
                        .disabled(modal)
                        .on_click(|_, window, cx| crate::updates::ui::show(window, cx, false)),
                ),
            )
        })
        .child(
            control_region("workspace-titlebar-more")
                .mr(compact::chrome_inset(cx))
                .child(controls::render_more_menu(&owner, modal)),
        );

    div()
        .id("workspace-titlebar")
        .test_support()
        .flex_shrink_0()
        .child(
            TitleBar::new()
                .h(px(32.))
                .bg(compact::workspace(cx))
                .border_0()
                .when(window.is_fullscreen(), |bar| bar.pl_0())
                .child(div().relative().flex_1().min_w_0().h_full().child(chrome)),
        )
}

#[cfg(test)]
pub(crate) fn application_menu_button(
    modal: bool,
    window: &mut Window,
    cx: &mut App,
) -> impl IntoElement {
    let menu = super::application_menu::state(window, cx);
    menu.update(cx, |menu, _| menu.modal = modal);
    menu
}

/// Controls must not arm native window dragging or double-click zooming.
fn control_region(id: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .flex_shrink_0()
        .items_center()
        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_click(|_, _, cx| cx.stop_propagation())
}

fn render_project_select(
    view: &EditorView,
    modal: bool,
    width: Pixels,
    cx: &mut Context<EditorView>,
) -> impl IntoElement {
    let documents = view.editor.documents();
    let active_id = view.editor.active_id();
    let active = documents.iter().find(|doc| Some(doc.id) == active_id);
    let empty = active.is_none();
    let title = active
        .map(|doc| {
            doc.title
                .clone()
                .unwrap_or_else(|| t!("editor.untitled_project").into_owned())
        })
        .unwrap_or_else(|| t!("editor.project_empty").into_owned());
    let modified = active.is_some_and(|doc| doc.modified);
    let tooltip = active
        .and_then(|doc| doc.path.as_ref())
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| title.clone());
    let [can_new, can_open, can_close] = [CommandId::New, CommandId::Open, CommandId::Close]
        .map(|id| !modal && view.editor.command_available(view.editor.command(id)));
    let recent_files = cx
        .global::<crate::settings::Settings>()
        .recent_files
        .paths
        .clone();
    let owner = cx.weak_entity();
    compact::button(Button::new("project-select"))
        .ghost()
        .h(compact::rem(WORKSPACE_SWITCHER_HEIGHT))
        .w(width)
        .min_w_0()
        .icon(if empty {
            IconName::FolderOpen
        } else {
            IconName::File
        })
        .dropdown_caret(true)
        .accessibility_label(format!(
            "{}: {title}{}",
            t!("editor.select_project"),
            if modified {
                t!("editor.unsaved").into_owned()
            } else {
                String::new()
            }
        ))
        .tooltip(tooltip)
        .disabled(modal)
        .when(empty, |button| {
            button.text_color(cx.theme().muted_foreground)
        })
        .child(
            div()
                .id("project-select-title")
                .test_support()
                .flex_1()
                .min_w_0()
                .text_size(compact::rem(compact::BODY))
                .overflow_hidden()
                .text_ellipsis()
                .child(title),
        )
        .when(modified, |button| {
            button.child(
                div()
                    .id("project-select-modified")
                    .test_support()
                    .flex_shrink_0()
                    .size(compact::rem(5.))
                    .rounded_full()
                    .bg(crate::theme::modified(cx.theme())),
            )
        })
        .dropdown_menu(move |mut menu, window, cx| {
            menu = crate::ui::compact::menu(menu);
            menu = menu
                .check_side(Side::Right)
                .min_w(compact::scaled(240., cx))
                .max_w(compact::scaled(360., cx))
                .scrollable(true);
            for document in &documents {
                let id = document.id;
                let title = document
                    .title
                    .clone()
                    .unwrap_or_else(|| t!("editor.untitled_project").into_owned());
                let path = document
                    .path
                    .as_ref()
                    .map(|path| path.display().to_string());
                let modified = document.modified;
                let owner = owner.clone();
                menu = menu.item(
                    PopupMenuItem::element(move |_, cx| {
                        div()
                            .id(SharedString::from(format!("project-select-{id:?}")))
                            .test_support()
                            .w(compact::scaled(280., cx))
                            .flex_shrink_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .py(compact::rem(2.))
                            .child(div().min_w_0().overflow_hidden().text_ellipsis().child(
                                format!(
                                    "{title}{}",
                                    if modified {
                                        t!("editor.unsaved").into_owned()
                                    } else {
                                        String::new()
                                    }
                                ),
                            ))
                            .when_some(path.clone(), |row, path| {
                                row.child(
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .text_size(compact::rem(compact::CAPTION))
                                        .text_color(cx.theme().muted_foreground)
                                        .child(path),
                                )
                            })
                    })
                    .icon(IconName::File)
                    .checked(Some(id) == active_id)
                    .on_click(move |_, window, cx| {
                        _ = owner.update(cx, |view, cx| view.activate_document(id, window, cx));
                    }),
                );
            }
            if !documents.is_empty() {
                menu = menu.separator();
            }
            for (id, command, label, icon, enabled) in [
                (
                    "project-new",
                    CommandId::New,
                    t!("editor.new_project"),
                    IconName::Plus,
                    can_new,
                ),
                (
                    "project-open",
                    CommandId::Open,
                    t!("editor.open_project"),
                    IconName::FolderOpen,
                    can_open,
                ),
            ] {
                menu = menu.item(
                    PopupMenuItem::element(move |_, _| {
                        div().id(id).test_support().child(label.clone())
                    })
                    .icon(icon)
                    .action(Box::new(crate::actions::RunCommand::new(command)))
                    .disabled(!enabled),
                );
            }
            let recent_owner = owner.clone();
            let recent_files = recent_files.clone();
            menu = menu.submenu(t!("menu.open_recent"), window, cx, move |menu, _, cx| {
                let mut menu = compact::menu(menu)
                    .min_w(compact::scaled(240., cx))
                    .max_w(compact::scaled(360., cx))
                    .scrollable(true);
                if recent_files.is_empty() {
                    return menu
                        .item(PopupMenuItem::new(t!("menu.no_recent_files")).disabled(true));
                }
                for (index, path) in recent_files.iter().enumerate() {
                    let title = crate::menus::path_title(path);
                    let display_path = path.display().to_string();
                    let path = path.clone();
                    let owner = recent_owner.clone();
                    menu = menu.item(
                        PopupMenuItem::element(move |_, cx| {
                            div()
                                .id(SharedString::from(format!("project-recent-{index}")))
                                .test_support()
                                .w(compact::scaled(280., cx))
                                .min_w_0()
                                .flex()
                                .flex_col()
                                .py(compact::rem(2.))
                                .child(
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .child(title.clone()),
                                )
                                .child(
                                    div()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .text_size(compact::rem(compact::CAPTION))
                                        .text_color(cx.theme().muted_foreground)
                                        .child(display_path.clone()),
                                )
                        })
                        .icon(IconName::File)
                        .on_click(move |_, window, cx| {
                            _ = owner.update(cx, |view, cx| view.open_recent(&path, window, cx));
                        }),
                    );
                }
                let owner = recent_owner.clone();
                menu.separator().item(
                    PopupMenuItem::element(|_, _| {
                        div()
                            .id("project-clear-recent")
                            .test_support()
                            .child(t!("menu.clear_recent"))
                    })
                    .on_click(move |_, window, cx| {
                        _ = owner.update(cx, |view, cx| view.clear_recent(window, cx));
                    }),
                )
            });
            if active_id.is_some() {
                menu = menu.separator().item(
                    PopupMenuItem::element(|_, _| {
                        div()
                            .id("project-close")
                            .test_support()
                            .child(t!("editor.close_project"))
                    })
                    .icon(IconName::Close)
                    .action(Box::new(crate::actions::RunCommand::new(CommandId::Close)))
                    .disabled(!can_close),
                );
            }
            menu
        })
        .anchor(Anchor::TopLeft)
}
