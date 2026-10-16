//! GPUI actions shared by menus, shortcuts, and editor controls.
use super::*;
use editor_core::CommandId;
use gpui_kit::component::{ActiveTheme, ThemeMode, WindowExt, input};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Action, Clone, Deserialize)]
#[action(namespace=kasane_editor,no_json)]
pub struct RunCommand {
    pub id: CommandId,
    #[serde(skip)]
    pub invocation: Option<editor_core::Command>,
}
impl RunCommand {
    pub fn new(id: CommandId) -> Self {
        Self {
            id,
            invocation: None,
        }
    }
    pub fn targeted(command: editor_core::Command) -> Self {
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
#[action(namespace=kasane_editor,no_json)]
pub struct OpenRecent {
    pub path: PathBuf,
}

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace=kasane_editor,no_json)]
pub struct SetLanguage {
    pub language: Option<String>,
}

actions!(
    kasane_editor,
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
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
}

impl EditorView {
    pub(super) fn action_handlers(&self, modal: bool, cx: &Context<Self>) -> Div {
        div()
            .key_context(if modal {
                "KasaneEditor KasaneModal"
            } else {
                "KasaneEditor"
            })
            .track_focus(&self.focus)
            .when(!modal, |e| {
                e.on_action(cx.listener(|this, action: &RunCommand, window, cx| {
                    let command = action.invocation.unwrap_or_else(|| {
                        this.editor
                            .command(action.id)
                            .with_source(editor_core::CommandSource::Shortcut)
                    });
                    this.run_invocation(command, window, cx)
                }))
                .on_action(cx.listener(|this, action: &OpenRecent, window, cx| {
                    this.open_recent(&action.path, window, cx)
                }))
                .on_action(
                    cx.listener(|this, _: &ClearRecent, window, cx| this.clear_recent(window, cx)),
                )
                .on_action(cx.listener(|this, _: &CloseWindow, window, cx| this.close(window, cx)))
                .on_action(cx.listener(|this, action: &SetLanguage, window, cx| {
                    let mut settings = cx.global::<settings::Settings>().clone();
                    if settings.language == action.language {
                        return;
                    }
                    match settings.set_language(action.language.clone()) {
                        Ok(()) => {
                            let welcome_message = this.message == t!("editor.welcome");
                            i18n::activate(settings.language.as_deref());
                            cx.set_global(settings);
                            if welcome_message {
                                this.message = t!("editor.welcome").into_owned();
                            }
                            window.set_window_title(&t!("app.title"));
                            this.menu_state = menus::MenuState::default();
                            cx.refresh_windows();
                        }
                        Err(error) => {
                            this.edit_error = Some(error.to_string());
                            cx.notify();
                        }
                    }
                }))
                .on_action(|_: &About, window, cx| {
                    window.open_dialog(cx, |dialog, _, _| {
                        dialog.title(t!("app.title")).child(t!("app.description"))
                    })
                })
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
            .on_action(|_: &ToggleTheme, _, cx| {
                let mode = if cx.theme().is_dark() {
                    ThemeMode::Light
                } else {
                    ThemeMode::Dark
                };
                theme::change(mode, cx);
            })
            .on_action(|_: &Minimize, window, _| window.minimize_window())
            .on_action(|_: &ZoomWindow, window, _| window.zoom_window())
    }
}
