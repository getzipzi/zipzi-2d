//! Native macOS menus and focus-aware command routing.
use super::*;
use crate::project::ImportKind;
use gpui_kit::component::{ActiveTheme, WindowExt, input};

// Text actions intentionally reuse Kit's types: the focused input gets first refusal.
actions!(
    kasane_editor,
    [
        OpenProject,
        ImportModel3,
        ImportPsd,
        SaveProject,
        SaveProjectAs,
        CloseWindow,
        Quit,
        About,
        Hide,
        HideOthers,
        ShowAll,
        ZoomIn,
        ZoomOut,
        ResetView,
        ToggleTheme,
        Minimize,
        ZoomWindow
    ]
);

pub(super) fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("cmd-o", OpenProject, Some("KasaneEditor")),
        KeyBinding::new("cmd-s", SaveProject, Some("KasaneEditor")),
        KeyBinding::new("cmd-shift-s", SaveProjectAs, Some("KasaneEditor")),
        KeyBinding::new("cmd-w", CloseWindow, Some("KasaneEditor")),
        KeyBinding::new("cmd-q", Quit, Some("KasaneEditor")),
        KeyBinding::new("cmd-z", input::Undo, Some("KasaneEditor")),
        KeyBinding::new("cmd-shift-z", input::Redo, Some("KasaneEditor")),
        KeyBinding::new("cmd-=", ZoomIn, Some("KasaneEditor")),
        KeyBinding::new("cmd-+", ZoomIn, Some("KasaneEditor")),
        KeyBinding::new("cmd--", ZoomOut, Some("KasaneEditor")),
        KeyBinding::new("cmd-0", ResetView, Some("KasaneEditor")),
        KeyBinding::new("cmd-m", Minimize, Some("KasaneEditor")),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("cmd-alt-h", HideOthers, None),
    ]);
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.set_menus([
        Menu::new(t!("app.title")).items([
            MenuItem::action(t!("app.about"), About),
            MenuItem::separator(),
            MenuItem::os_submenu(t!("menu.services"), SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action(t!("app.hide"), Hide),
            MenuItem::action(t!("app.hide_others"), HideOthers),
            MenuItem::action(t!("app.show_all"), ShowAll),
            MenuItem::separator(),
            MenuItem::action(t!("app.quit"), Quit),
        ]),
        Menu::new(t!("menu.file")).items([
            MenuItem::action(t!("menu.open"), OpenProject),
            MenuItem::separator(),
            MenuItem::action(t!("menu.import_model3"), ImportModel3),
            MenuItem::action(t!("menu.import_psd"), ImportPsd),
            MenuItem::separator(),
            MenuItem::action(t!("menu.save"), SaveProject),
            MenuItem::action(t!("menu.save_as"), SaveProjectAs),
            MenuItem::separator(),
            MenuItem::action(t!("menu.close_window"), CloseWindow),
        ]),
        Menu::new(t!("menu.edit")).items([
            MenuItem::os_action(t!("menu.undo"), input::Undo, OsAction::Undo),
            MenuItem::os_action(t!("menu.redo"), input::Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action(t!("menu.cut"), input::Cut, OsAction::Cut),
            MenuItem::os_action(t!("menu.copy"), input::Copy, OsAction::Copy),
            MenuItem::os_action(t!("menu.paste"), input::Paste, OsAction::Paste),
            MenuItem::os_action(t!("menu.select_all"), input::SelectAll, OsAction::SelectAll),
        ]),
        Menu::new(t!("menu.view")).items([
            MenuItem::action(t!("menu.zoom_in"), ZoomIn),
            MenuItem::action(t!("menu.zoom_out"), ZoomOut),
            MenuItem::action(t!("menu.reset_view"), ResetView),
            MenuItem::separator(),
            MenuItem::action(t!("menu.toggle_theme"), ToggleTheme),
        ]),
        Menu::new(t!("menu.window")).items([
            MenuItem::action(t!("menu.minimize"), Minimize),
            MenuItem::action(t!("menu.zoom_window"), ZoomWindow),
        ]),
    ]);
}

impl Editor {
    /// Only register enabled document commands; AppKit validates menu items from
    /// the active action path. The input's own text handlers take precedence.
    pub(super) fn menu_actions(&self, cx: &Context<Self>) -> Div {
        let (undo, redo) = self.document.history_lengths();
        div()
            .key_context("KasaneEditor")
            .track_focus(&self.focus)
            .when(!self.busy, |element| {
                element
                    .on_action(
                        cx.listener(|this, _: &OpenProject, window, cx| this.open(window, cx)),
                    )
                    .on_action(cx.listener(|this, _: &ImportModel3, window, cx| {
                        this.import(ImportKind::Model3, window, cx)
                    }))
                    .on_action(cx.listener(|this, _: &ImportPsd, window, cx| {
                        this.import(ImportKind::Psd, window, cx)
                    }))
                    .on_action(
                        cx.listener(|this, _: &SaveProject, window, cx| {
                            this.save(false, window, cx)
                        }),
                    )
                    .on_action(cx.listener(|this, _: &SaveProjectAs, window, cx| {
                        this.save(true, window, cx)
                    }))
                    .on_action(
                        cx.listener(|this, _: &CloseWindow, window, cx| this.close(window, cx)),
                    )
                    // This editor has one window; closing it quits through on_window_closed.
                    .on_action(cx.listener(|this, _: &Quit, window, cx| this.close(window, cx)))
                    .on_action(|_: &About, window, cx| {
                        window.open_dialog(cx, |dialog, _, _| {
                            dialog.title(t!("app.title")).child(t!("app.description"))
                        });
                    })
            })
            .when(
                !self.busy && (undo > 0 || self.document.has_pending_edit()),
                |element| {
                    element.on_action(
                        cx.listener(|this, _: &input::Undo, window, cx| this.undo(window, cx)),
                    )
                },
            )
            .when(
                !self.busy && redo > 0 && !self.document.has_pending_edit(),
                |element| {
                    element.on_action(
                        cx.listener(|this, _: &input::Redo, window, cx| this.redo(window, cx)),
                    )
                },
            )
            .when(self.view.zoom < 3., |element| {
                element.on_action(cx.listener(|this, _: &ZoomIn, _, cx| this.zoom_by(0.25, cx)))
            })
            .when(self.view.zoom > 0.25, |element| {
                element.on_action(cx.listener(|this, _: &ZoomOut, _, cx| this.zoom_by(-0.25, cx)))
            })
            .on_action(cx.listener(|this, _: &ResetView, _, cx| this.reset_view(cx)))
            .on_action(|_: &ToggleTheme, window, cx| Self::toggle_theme(window, cx))
            .on_action(|_: &Minimize, window, _| window.minimize_window())
            .on_action(|_: &ZoomWindow, window, _| window.zoom_window())
    }

    pub(super) fn undo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let result = self.document.undo();
        self.complete_edit(result, window, cx, true);
    }

    pub(super) fn redo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let result = self.document.redo();
        self.complete_edit(result, window, cx, true);
    }

    pub(super) fn zoom_by(&mut self, delta: f32, cx: &mut Context<Self>) {
        self.view.zoom = (self.view.zoom + delta).clamp(0.25, 3.);
        self.redraw(cx);
    }

    pub(super) fn reset_view(&mut self, cx: &mut Context<Self>) {
        self.view = ViewState::default();
        self.redraw(cx);
    }

    pub(super) fn toggle_theme(window: &mut Window, cx: &mut App) {
        let mode = if cx.theme().is_dark() {
            ThemeMode::Light
        } else {
            ThemeMode::Dark
        };
        Theme::change(mode, Some(window), cx);
    }

    pub(super) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        if self.document.modified() {
            self.confirm_close(window, cx);
        } else {
            window.remove_window();
        }
    }
}
