//! Lightweight start panel. Project admission remains in the ordinary Editor queue.
use crate::{EditorView, settings, t};
use gpui_kit::assets::IconName;
use gpui_kit::component::{
    ActiveTheme, Icon, Sizable, WindowExt,
    button::{Button, ButtonVariants},
};
use gpui_kit::{prelude::*, *};
use zipzi_editor_core::CommandId;

use super::compact;

impl EditorView {
    pub(crate) fn dismiss_welcome(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.welcome.take().is_some() {
            window.close_dialog(cx);
            window.focus(&self.focus, cx);
            cx.notify();
        }
    }

    pub(crate) fn show_welcome(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.modal || window.has_active_dialog(cx) || self.editor.exiting() {
            return;
        }
        if self.suspend_input(zipzi_editor_core::EditingLifecycle::BlockingUi, window, cx) {
            self.redraw(cx);
        }
        let owner = cx.weak_entity();
        let document = self.editor.active_id();
        let panel = cx.new(|cx| WelcomePanel {
            owner,
            document,
            focus: cx.focus_handle(),
        });
        let focus = panel.read(cx).focus.clone();
        self.welcome = Some(panel.clone());
        let owner = cx.weak_entity();
        window.open_dialog(cx, move |dialog, window, cx| {
            dialog
                .entrance(gpui_kit::component::dialog::DialogEntrance::None)
                .w(compact::scaled(480., cx))
                .margin_top(
                    ((window.viewport_size().height - compact::scaled(370., cx)) / 2.).max(px(16.)),
                )
                .child(panel.clone())
                .on_close({
                    let owner = owner.clone();
                    move |_, _, cx| {
                        _ = owner.update(cx, |view, cx| {
                            view.welcome = None;
                            cx.notify();
                        });
                    }
                })
        });
        window.defer(cx, move |window, cx| window.focus(&focus, cx));
        cx.notify();
    }

    pub(crate) fn sync_welcome(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let has_documents = self.editor.document_count() != 0;
        let queued = !self.project_queue.lock().unwrap().is_empty();
        let document_changed = self
            .welcome
            .as_ref()
            .is_some_and(|panel| panel.read(cx).document != self.editor.active_id());
        if document_changed || queued {
            self.dismiss_welcome(window, cx);
        }
        if !self.welcome_pending || queued || self.editor.pending_io_count() != 0 {
            return;
        }
        if has_documents {
            self.welcome_pending = false;
        } else if !self.modal && !window.has_active_dialog(cx) && !self.editor.exiting() {
            self.welcome_pending = false;
            // Open outside render so Kit can update its window layer safely.
            cx.defer_in(window, |view, window, cx| {
                if view.editor.document_count() == 0
                    && view.project_queue.lock().unwrap().is_empty()
                    && view.editor.pending_io_count() == 0
                {
                    view.show_welcome(window, cx);
                }
            });
        }
    }
}

pub(crate) struct WelcomePanel {
    owner: WeakEntity<EditorView>,
    focus: FocusHandle,
    document: Option<zipzi_editor_core::DocumentId>,
}

impl Render for WelcomePanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let recent = cx.global::<settings::Settings>().recent_files.paths.clone();
        let new_owner = self.owner.clone();
        let open_owner = self.owner.clone();
        div()
            .id("welcome-panel")
            .track_focus(&self.focus)
            .test_support()
            .on_action(
                cx.listener(|this, action: &crate::actions::RunCommand, window, cx| {
                    if matches!(
                        action.id,
                        CommandId::New | CommandId::Open | CommandId::Quit
                    ) {
                        _ = this.owner.update(cx, |view, cx| {
                            view.dismiss_welcome(window, cx);
                            view.run_command(action.id, window, cx);
                        });
                    }
                }),
            )
            .on_action(
                cx.listener(|this, _: &crate::actions::CloseWindow, window, cx| {
                    _ = this
                        .owner
                        .update(cx, |view, cx| view.close_window(window, cx));
                }),
            )
            .flex()
            .flex_col()
            .gap_5()
            .text_size(compact::rem(compact::BODY))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_1()
                    .py_3()
                    .child(super::branding::lockup(window, cx))
                    .child(
                        div()
                            .text_size(compact::rem(compact::CAPTION))
                            .text_color(cx.theme().muted_foreground)
                            .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .gap_3()
                    .child(
                        compact::button(Button::new("welcome-new"))
                            .ghost()
                            .flex_1()
                            .h(compact::rem(30.))
                            .icon(IconName::Plus)
                            .label(t!("editor.new_project"))
                            .on_click(move |_, window, cx| {
                                _ = new_owner.update(cx, |view, cx| {
                                    view.dismiss_welcome(window, cx);
                                    view.run_command(CommandId::New, window, cx);
                                });
                            }),
                    )
                    .child(
                        compact::button(Button::new("welcome-open"))
                            .ghost()
                            .flex_1()
                            .h(compact::rem(30.))
                            .icon(IconName::FolderOpen)
                            .label(t!("welcome.open_project"))
                            .on_click(move |_, window, cx| {
                                _ = open_owner.update(cx, |view, cx| {
                                    view.dismiss_welcome(window, cx);
                                    view.open(window, cx);
                                });
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(
                        div()
                            .text_color(cx.theme().muted_foreground)
                            .child(t!("welcome.recent_projects")),
                    )
                    .child(
                        div()
                            .id("welcome-recents")
                            .max_h(compact::rem(160.))
                            .overflow_y_scroll()
                            .when(recent.is_empty(), |list| {
                                list.child(
                                    div()
                                        .py_3()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(t!("menu.no_recent_files")),
                                )
                            })
                            .children(recent.iter().enumerate().map(|(index, path)| {
                                let owner = self.owner.clone();
                                let path = path.clone();
                                compact::button(Button::new(("welcome-recent", index)))
                                    .ghost()
                                    .w_full()
                                    .justify_start()
                                    .accessibility_label(crate::menus::recent_file_label(
                                        &path, &recent,
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .w_full()
                                            .min_w_0()
                                            .items_center()
                                            .gap_1()
                                            .child(Icon::new(IconName::File).xsmall())
                                            .child(div().min_w_0().truncate().child(
                                                crate::menus::recent_file_label(&path, &recent),
                                            )),
                                    )
                                    .tooltip(path.display().to_string())
                                    .on_click(move |_, window, cx| {
                                        _ = owner.update(cx, |view, cx| {
                                            view.dismiss_welcome(window, cx);
                                            view.open_recent(&path, window, cx);
                                        });
                                    })
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(
                        div()
                            .id("welcome-github")
                            .test_support()
                            .child(super::about::link(
                                "welcome-github-link",
                                "GitHub",
                                super::about::REPOSITORY,
                                cx,
                            )),
                    ),
            )
    }
}
