use crate::{EditorView, t};
use editor_core::CommandId;
use gpui_kit::component::{ActiveTheme, Icon, IconName, Sizable};
use gpui_kit::{prelude::*, *};

pub fn render_tab_bar(
    view: &EditorView,
    modal: bool,
    _window: &mut Window,
    cx: &mut Context<EditorView>,
) -> impl IntoElement {
    let theme = cx.theme();
    let documents = view.editor.documents();
    let active_id = view.editor.active_id();

    div()
        .id("global-document-tabs")
        .h(px(34.))
        .w_full()
        .flex_shrink_0()
        .bg(theme.tab_bar)
        .border_b_1()
        .border_color(theme.border)
        .flex()
        .items_center()
        .px_2()
        .gap_1()
        .child(
            div()
                .id("document-tabs-scroll")
                .min_w_0()
                .h_full()
                .flex()
                .items_center()
                .gap_1()
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
                        .h(px(26.))
                        .px_2p5()
                        .gap_1p5()
                        .rounded_t_md()
                        .bg(bg)
                        .border_t_1()
                        .border_l_1()
                        .border_r_1()
                        .border_color(if is_active {
                            theme.border
                        } else {
                            gpui::transparent_black()
                        })
                        .hover(|s| {
                            if !is_active {
                                s.bg(theme.muted.opacity(0.3)).text_color(theme.foreground)
                            } else {
                                s
                            }
                        })
                        .cursor_pointer()
                        .on_click(cx.listener(move |this, _, window, cx| {
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
                            e.child(div().w(px(6.)).h(px(6.)).rounded_full().bg(theme.primary))
                        })
                        // Close button (x)
                        .child(
                            div()
                                .id(SharedString::from(format!("tab-close-{:?}", id)))
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
        // Plus button to open/new
        .child(
            div()
                .id("tab-new-button")
                .test_support()
                .flex_shrink_0()
                .p_1()
                .rounded(px(4.))
                .text_color(theme.muted_foreground)
                .hover(|s| s.bg(theme.muted).text_color(theme.foreground))
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, window, cx| {
                    if modal {
                        return;
                    }
                    this.run_command(CommandId::New, window, cx);
                }))
                .child(Icon::new(IconName::Plus).small()),
        )
}
