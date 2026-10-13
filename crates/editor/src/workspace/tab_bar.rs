use crate::{EditorView, t};
use editor_core::CommandId;
use gpui_kit::component::{
    ActiveTheme, Disableable, Icon, IconName, Sizable, TitleBar,
    button::{Button, ButtonVariants},
    menu::DropdownMenu,
};
use gpui_kit::{prelude::*, *};

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

pub fn render_tab_bar(
    view: &EditorView,
    modal: bool,
    window: &mut Window,
    cx: &mut Context<EditorView>,
) -> impl IntoElement {
    let theme = cx.theme();
    let documents = view.editor.documents();
    let active_id = view.editor.active_id();
    let [can_new, can_open] = [CommandId::New, CommandId::Open]
        .map(|id| !modal && view.editor.command_available(view.editor.command(id)));

    let tabs = div()
        .id("global-document-tabs")
        .test_support()
        .absolute()
        .size_full()
        .min_w_0()
        .bg(theme.tab_bar)
        .flex()
        .child(
            div()
                .min_w_0()
                .h_full()
                .flex()
                .items_center()
                .child(
                    div()
                        .id("document-tabs-scroll")
                        .min_w_0()
                        .h_full()
                        .flex()
                        .overflow_x_scroll()
                        .children(documents.into_iter().map(|doc| {
                            let id = doc.id;
                            let is_active = active_id == Some(id);
                            let title = doc
                                .title
                                .unwrap_or_else(|| t!("editor.untitled_project").into_owned());

                            let bg = if is_active {
                                theme.background
                            } else {
                                gpui::transparent_black()
                            };

                            let text_color = if is_active {
                                theme.foreground
                            } else {
                                theme.muted_foreground
                            };

                            div()
                                .id(SharedString::from(format!("tab-item-{:?}", id)))
                                .test_support()
                                .flex()
                                .flex_shrink_0()
                                .items_center()
                                .h_full()
                                .px_3()
                                .gap_1p5()
                                .bg(bg)
                                .border_r_1()
                                .border_color(theme.border)
                                .hover(|s| {
                                    if !is_active {
                                        s.bg(theme.muted.opacity(0.3)).text_color(theme.foreground)
                                    } else {
                                        s
                                    }
                                })
                                .cursor_pointer()
                                // Tab interactions must not arm TitleBar's window drag
                                // or bubble a double click into its zoom action.
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    cx.stop_propagation();
                                    if modal {
                                        return;
                                    }
                                    this.activate_document(id, window, cx);
                                }))
                                // File Icon
                                .child(
                                    div()
                                        .text_color(if is_active {
                                            theme.primary
                                        } else {
                                            theme.muted_foreground
                                        })
                                        .child(Icon::new(IconName::File).small()),
                                )
                                // Tab Title
                                .child(
                                    div()
                                        .text_xs()
                                        .max_w(px(180.))
                                        .overflow_hidden()
                                        .text_ellipsis()
                                        .font_weight(if is_active {
                                            FontWeight::MEDIUM
                                        } else {
                                            FontWeight::NORMAL
                                        })
                                        .text_color(text_color)
                                        .child(title),
                                )
                                // Modified Dot
                                .when(doc.modified, |e| {
                                    e.child(
                                        div()
                                            .w(px(6.))
                                            .h(px(6.))
                                            .rounded_full()
                                            .bg(crate::theme::modified(theme)),
                                    )
                                })
                                // Close button (x)
                                .child(
                                    div()
                                        .id(SharedString::from(format!("tab-close-{:?}", id)))
                                        .test_support()
                                        .ml_1()
                                        .p_0p5()
                                        .rounded(px(3.))
                                        .text_color(theme.muted_foreground)
                                        .hover(|s| s.bg(theme.muted).text_color(theme.foreground))
                                        .on_click(cx.listener(move |this, _, window, cx| {
                                            cx.stop_propagation();
                                            if modal {
                                                return;
                                            }
                                            this.close_document(id, window, cx);
                                        }))
                                        .child(Icon::new(IconName::Close).small()),
                                )
                        })),
                )
                .child(
                    div()
                        .id("project-entry-control")
                        .h_full()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .px_1()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(
                            Button::new("project-new-or-open")
                                .small()
                                .ghost()
                                .icon(IconName::Plus)
                                .accessibility_label(t!("editor.new_or_open_project"))
                                .tooltip(t!("editor.new_or_open_project"))
                                .disabled(!can_new && !can_open)
                                .dropdown_menu(move |menu, _, _| {
                                    menu.menu_with_enable(
                                        t!("editor.new_project"),
                                        Box::new(crate::actions::RunCommand { id: CommandId::New }),
                                        can_new,
                                    )
                                    .menu_with_enable(
                                        t!("editor.open_project"),
                                        Box::new(crate::actions::RunCommand {
                                            id: CommandId::Open,
                                        }),
                                        can_open,
                                    )
                                })
                                .anchor(Anchor::TopLeft),
                        ),
                ),
        )
        .child(
            div()
                .id("window-titlebar-drag-region")
                .test_support()
                .h_full()
                .min_w(px(72.))
                .flex_1(),
        );

    div()
        .id("workspace-titlebar")
        .test_support()
        .flex_shrink_0()
        .child(
            TitleBar::new()
                .bg(theme.tab_bar)
                .border_t_1()
                .border_color(theme.border)
                // Fullscreen uses the system's retractable controls rather than the
                // in-content traffic lights. TitleBar supplies a small inset itself.
                .when(window.is_fullscreen(), |bar| bar.pl_0())
                // Keep the tab strip's intrinsic width out of TitleBar's
                // layout so overflowing tabs scroll within the available area.
                .child(div().relative().flex_1().min_w_0().h_full().child(tabs)),
        )
}
