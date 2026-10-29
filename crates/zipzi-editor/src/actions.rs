//! GPUI actions shared by menus, shortcuts, and editor controls.
use super::*;
use gpui_kit::component::input;
use serde::Deserialize;
use std::path::PathBuf;
use zipzi_editor_core::CommandId;

#[derive(Action, Clone, Deserialize)]
#[action(namespace=zipzi_editor,no_json)]
pub struct RunCommand {
    pub id: CommandId,
    #[serde(skip)]
    pub invocation: Option<zipzi_editor_core::Command>,
}
impl RunCommand {
    pub fn new(id: CommandId) -> Self {
        Self {
            id,
            invocation: None,
        }
    }
    pub fn targeted(command: zipzi_editor_core::Command) -> Self {
        Self {
            id: command.id,
            invocation: Some(command),
        }
    }
}
// A saved invocation and its keymap entry are the same user command. Keeping
// equality independent of target also preserves native menu accelerators.
impl PartialEq for RunCommand {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace=zipzi_editor,no_json)]
pub struct OpenRecent {
    pub path: PathBuf,
}

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace=zipzi_editor,no_json)]
pub struct SetLanguage {
    pub language: Option<String>,
}

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace=zipzi_editor,no_json)]
pub struct SetColorTheme {
    pub theme: theme::ColorTheme,
}

actions!(
    zipzi_editor,
    [
        CloseWindow,
        About,
        Hide,
        HideOthers,
        ShowAll,
        ToggleTheme,
        Minimize,
        ZoomWindow,
        ClearRecent
    ]
);

pub(super) fn init(cx: &mut App) {
    keybindings::init(cx);
    cx.on_action(|action: &RunCommand, cx| {
        if cx.windows().is_empty() {
            if action.id == CommandId::Quit {
                if let Some(host) = cx.try_global::<host::DesktopHost>() {
                    host.pending_quit.set(true);
                }
                host::open_window(cx, true);
            } else if action.id == CommandId::New {
                if let Some(host) = cx.try_global::<host::DesktopHost>() {
                    host.new_document();
                }
                host::open_window(cx, true);
            }
        }
    });
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
}

impl EditorView {
    pub(super) fn action_handlers(&self, modal: bool, cx: &Context<Self>) -> Div {
        div()
            .key_context(if modal {
                "ZipziEditor ZipziModal"
            } else {
                "ZipziEditor"
            })
            .track_focus(&self.focus)
            .when(!modal, |e| {
                e.on_action(cx.listener(|this, action: &RunCommand, window, cx| {
                    if let Some(viewport) = this.focused_viewport_id(window, cx) {
                        this.focused_viewport = Some(viewport);
                    }
                    let command = action.invocation.unwrap_or_else(|| {
                        if action.id == CommandId::DeleteObject {
                            this.object_deletion_invocation(
                                zipzi_editor_core::CommandSource::Direct,
                                window,
                                cx,
                            )
                        } else {
                            this.viewport_command(
                                action.id,
                                zipzi_editor_core::CommandSource::Direct,
                            )
                        }
                    });
                    this.run_invocation(command, window, cx)
                }))
                .on_action(cx.listener(|this, action: &OpenRecent, window, cx| {
                    this.open_recent(&action.path, window, cx)
                }))
                .on_action(
                    cx.listener(|this, _: &ClearRecent, window, cx| this.clear_recent(window, cx)),
                )
                .on_action(
                    cx.listener(|this, _: &CloseWindow, window, cx| this.close_window(window, cx)),
                )
                .on_action(cx.listener(|this, action: &SetLanguage, window, cx| {
                    let mut settings = cx.global::<settings::Settings>().clone();
                    if settings.language == action.language {
                        return;
                    }
                    match settings.set_language(action.language.clone()) {
                        Ok(()) => {
                            let welcome_message = t!("editor.welcome").into_owned();
                            i18n::activate(settings.language.as_deref());
                            cx.set_global(settings);
                            this.replace_status_message(
                                &welcome_message,
                                &t!("editor.welcome"),
                                cx,
                            );
                            window.set_window_title(&t!("app.title"));
                            this.menu_state = menus::MenuState::default();
                            workspace::status_bar::StatusBar::sync(&this.status_bar, this, cx);
                            cx.refresh_windows();
                        }
                        Err(error) => {
                            this.set_edit_error(error.to_string(), cx);
                        }
                    }
                }))
                .on_action(|_: &About, window, cx| ui::about::show(window, cx))
            })
            // Fallback native text actions only at non-input focus; Kit always owns input history.
            .when(!modal && self.editor.can_undo(), |e| {
                e.on_action(cx.listener(|this, _: &input::Undo, window, cx| {
                    if !this.has_focused_input(window, cx) {
                        this.run_command(CommandId::Undo, window, cx);
                    }
                }))
            })
            .when(!modal && self.editor.can_redo(), |e| {
                e.on_action(cx.listener(|this, _: &input::Redo, window, cx| {
                    if !this.has_focused_input(window, cx) {
                        this.run_command(CommandId::Redo, window, cx);
                    }
                }))
            })
            .on_action(cx.listener(|this, action: &SetColorTheme, _, cx| {
                if let Err(error) = theme::set_preference(action.theme, cx) {
                    this.set_edit_error(error.to_string(), cx);
                }
            }))
            .on_action(cx.listener(|this, _: &ToggleTheme, _, cx| {
                let choice = theme::current(cx).toggled();
                if let Err(error) = theme::set_preference(choice, cx) {
                    this.set_edit_error(error.to_string(), cx);
                }
            }))
            .on_action(|_: &Minimize, window, _| window.minimize_window())
            .on_action(|_: &ZoomWindow, window, _| window.zoom_window())
    }
}
