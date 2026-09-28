//! GPUI adapters for the core command catalog. Text editing keeps Kit's actions.
use super::*;
use crate::project::ImportKind;
use editor_core::{COMMANDS, CommandEffect, CommandId, CommandScope};
use gpui_kit::component::{ActiveTheme, WindowExt, input};
use serde::Deserialize;

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace=kasane_editor,no_json)]
pub struct RunCommand {
    pub id: CommandId,
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
        ZoomWindow
    ]
);
struct KitBindings(Vec<KeyBinding>);
impl Global for KitBindings {}
fn scope(scope: CommandScope) -> &'static str {
    match scope {
        CommandScope::Workspace => "KasaneEditor && !KasaneModal",
        CommandScope::Document => "KasaneEditor && !Input && !KasaneModal",
        CommandScope::Canvas => "KasaneCanvas && !Input && !KasaneModal",
        CommandScope::Property => "(KasaneEditor > Input) && !KasaneModal",
    }
}
fn install(cx: &mut App) {
    let bindings = cx
        .global::<settings::Settings>()
        .bindings
        .iter()
        .map(|binding| {
            KeyBinding::load(
                &binding.key,
                Box::new(RunCommand {
                    id: binding.command,
                }),
                Some(
                    KeyBindingContextPredicate::parse(scope(binding.command.spec().scope))
                        .unwrap()
                        .into(),
                ),
                false,
                None,
                cx.keyboard_mapper().as_ref(),
            )
            .expect("validated binding")
        })
        .collect::<Vec<_>>();
    cx.bind_keys(bindings);
    cx.bind_keys([
        KeyBinding::new("cmd-m", Minimize, Some("KasaneEditor")),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("cmd-alt-h", HideOthers, None),
    ]);
}
pub(super) fn init(cx: &mut App) {
    let defaults = cx.key_bindings().borrow().bindings().cloned().collect();
    cx.set_global(KitBindings(defaults));
    install(cx);
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
}
impl EditorView {
    pub(super) fn menu_actions(&self, modal: bool, cx: &Context<Self>) -> Div {
        div()
            .key_context(if modal {
                "KasaneEditor KasaneModal"
            } else {
                "KasaneEditor"
            })
            .track_focus(&self.focus)
            .when(!modal, |e| {
                e.on_action(cx.listener(|this, action: &RunCommand, window, cx| {
                    this.run_command(action.id, window, cx)
                }))
                .on_action(cx.listener(|this, _: &CloseWindow, window, cx| this.close(window, cx)))
                .on_action(|_: &About, window, cx| {
                    window.open_dialog(cx, |dialog, _, _| {
                        dialog.title(t!("app.title")).child(t!("app.description"))
                    })
                })
            })
            // Fallback native text actions only at non-input focus; Kit always owns input history.
            .when(!modal && self.editor.can_undo(), |e| {
                e.on_action(cx.listener(|this, _: &input::Undo, window, cx| {
                    if !this.name.focus_handle(cx).is_focused(window) {
                        this.run_command(CommandId::Undo, window, cx);
                    }
                }))
            })
            .when(!modal && self.editor.can_redo(), |e| {
                e.on_action(cx.listener(|this, _: &input::Redo, window, cx| {
                    if !this.name.focus_handle(cx).is_focused(window) {
                        this.run_command(CommandId::Redo, window, cx);
                    }
                }))
            })
            .on_action(|_: &ToggleTheme, window, cx| {
                let mode = if cx.theme().is_dark() {
                    ThemeMode::Light
                } else {
                    ThemeMode::Dark
                };
                Theme::change(mode, Some(window), cx);
            })
            .on_action(|_: &Minimize, window, _| window.minimize_window())
            .on_action(|_: &ZoomWindow, window, _| window.zoom_window())
    }
    pub(super) fn refresh_menus(&mut self, modal: bool, window: &Window, cx: &mut Context<Self>) {
        let text = self.name.focus_handle(cx).is_focused(window);
        let mut state = COMMANDS
            .iter()
            .map(|s| !modal && self.editor.command_available(self.editor.command(s.id)))
            .collect::<Vec<_>>();
        state.push(text);
        if self.menu_state == state {
            return;
        }
        self.menu_state = state;
        let item = |id: CommandId| {
            MenuItem::action(t!(id.spec().label).into_owned(), RunCommand { id })
                .disabled(modal || !self.editor.command_available(self.editor.command(id)))
        };
        let edit = if text {
            vec![
                MenuItem::os_action(t!("menu.undo"), input::Undo, OsAction::Undo),
                MenuItem::os_action(t!("menu.redo"), input::Redo, OsAction::Redo),
            ]
        } else {
            vec![item(CommandId::Undo), item(CommandId::Redo)]
        };
        cx.set_menus([
            Menu::new(t!("app.title")).items([
                MenuItem::action(t!("app.about"), About),
                MenuItem::separator(),
                MenuItem::os_submenu(t!("menu.services"), SystemMenuType::Services),
                MenuItem::action(t!("app.hide"), Hide),
                MenuItem::action(t!("app.hide_others"), HideOthers),
                MenuItem::action(t!("app.show_all"), ShowAll),
                MenuItem::separator(),
                item(CommandId::Quit),
            ]),
            Menu::new(t!("menu.file")).items([
                item(CommandId::New),
                item(CommandId::Open),
                item(CommandId::ImportModel3),
                item(CommandId::ImportPsd),
                MenuItem::separator(),
                item(CommandId::Save),
                item(CommandId::SaveAs),
                item(CommandId::Close),
                MenuItem::action(t!("menu.close_window"), CloseWindow).disabled(modal),
            ]),
            Menu::new(t!("menu.edit")).items(edit.into_iter().chain([
                MenuItem::separator(),
                MenuItem::os_action(t!("menu.cut"), input::Cut, OsAction::Cut),
                MenuItem::os_action(t!("menu.copy"), input::Copy, OsAction::Copy),
                MenuItem::os_action(t!("menu.paste"), input::Paste, OsAction::Paste),
                MenuItem::os_action(t!("menu.select_all"), input::SelectAll, OsAction::SelectAll),
                item(CommandId::CommitProperty),
                item(CommandId::CancelProperty),
            ])),
            Menu::new(t!("menu.view")).items([
                item(CommandId::ZoomIn),
                item(CommandId::ZoomOut),
                item(CommandId::ResetView),
                item(CommandId::Vertices),
                item(CommandId::CancelGesture),
                MenuItem::separator(),
                item(CommandId::Layers),
                item(CommandId::Properties),
                item(CommandId::ResetLayout),
                MenuItem::action(t!("menu.toggle_theme"), ToggleTheme),
                item(CommandId::ShowShortcuts),
                item(CommandId::ResetShortcuts),
            ]),
            Menu::new(t!("menu.window")).items([
                item(CommandId::Previous),
                item(CommandId::Next),
                MenuItem::action(t!("menu.minimize"), Minimize),
                MenuItem::action(t!("menu.zoom_window"), ZoomWindow),
            ]),
        ]);
    }
    pub(super) fn run_command(
        &mut self,
        id: CommandId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.modal || window.has_active_dialog(cx) {
            return;
        }
        let command = self.editor.command(id);
        if !self.editor.command_available(command) {
            return;
        }
        let previous = self.editor.active_id();
        match self.editor.dispatch(command) {
            Ok(CommandEffect::Done) => {
                if id == CommandId::Vertices {
                    if let Some(dock) = &self.dock {
                        window.focus(&dock.canvas.read(cx).focus_handle(cx), cx);
                    }
                }
                if self.editor.active_id() != previous {
                    window.focus(&self.focus, cx);
                    self.refresh_document(window, cx);
                } else {
                    if matches!(
                        id,
                        CommandId::Undo
                            | CommandId::Redo
                            | CommandId::CancelProperty
                            | CommandId::CommitProperty
                    ) {
                        self.sync_controls(window, cx);
                    }
                    self.redraw(cx);
                }
            }
            Ok(CommandEffect::Host(command)) => match command.id {
                CommandId::Open => self.open(window, cx),
                CommandId::ImportModel3 => self.import(ImportKind::Model3, window, cx),
                CommandId::ImportPsd => self.import(ImportKind::Psd, window, cx),
                CommandId::Save => self.save(false, window, cx),
                CommandId::SaveAs => self.save(true, window, cx),
                CommandId::Close => self.close_active(window, cx),
                CommandId::Quit => self.close(window, cx),
                CommandId::Layers | CommandId::Properties => {
                    if let Some(dock) = &mut self.dock {
                        dock.toggle(
                            if id == CommandId::Layers {
                                docking::PanelKind::Layers
                            } else {
                                docking::PanelKind::Properties
                            },
                            window,
                            cx,
                        );
                    }
                    cx.notify();
                }
                CommandId::ResetLayout => self.reset_layout(window, cx),
                CommandId::ResetShortcuts => {
                    let store = cx.global::<settings::Settings>().store.clone();
                    match store.write(
                        "keybindings.json",
                        &editor_core::ShortcutOverrides::default(),
                    ) {
                        Ok(()) => {
                            let settings = settings::Settings::from_store(
                                store,
                                cx.keyboard_mapper().as_ref(),
                            );
                            cx.set_global(settings);
                            let base = cx.global::<KitBindings>().0.clone();
                            cx.clear_key_bindings();
                            cx.bind_keys(base);
                            install(cx);
                            self.menu_state.clear();
                        }
                        Err(e) => self.edit_error = Some(e.to_string()),
                    }
                    cx.notify();
                }
                CommandId::ShowShortcuts => {
                    let settings = cx.global::<settings::Settings>();
                    let list = COMMANDS
                        .iter()
                        .map(|spec| {
                            format!(
                                "{}  {}",
                                t!(spec.label),
                                settings
                                    .bindings
                                    .iter()
                                    .filter(|b| b.command == spec.id)
                                    .map(|b| b.key.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            )
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    let text = format!(
                        "{}\n\n{}",
                        settings.store.root().join("keybindings.json").display(),
                        list
                    );
                    window.open_dialog(cx, move |dialog, _, _| {
                        dialog.title(t!("menu.shortcuts")).child(text.clone())
                    });
                }
                _ => {}
            },
            Err(e) => {
                self.edit_error = Some(e.to_string());
                cx.notify();
            }
        }
    }
    /// Only suppress repeats for resolved editor bindings; normal input still goes through GPUI.
    pub(super) fn filter_repeat(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !event.is_held || self.modal || window.has_active_dialog(cx) {
            return;
        }
        let text = self.name.focus_handle(cx).is_focused(window);
        let canvas = self
            .dock
            .as_ref()
            .is_some_and(|d| d.canvas.focus_handle(cx).is_focused(window));
        let key = event.keystroke.unparse();
        if cx.global::<settings::Settings>().bindings.iter().any(|b| {
            b.key == key
                && !b.command.spec().repeat
                && match b.command.spec().scope {
                    CommandScope::Workspace => true,
                    CommandScope::Document => !text,
                    CommandScope::Canvas => canvas,
                    CommandScope::Property => text,
                }
        }) {
            cx.stop_propagation();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{RunCommand, input, scope};
    use editor_core::{CommandId, ShortcutOverride, ShortcutOverrides};
    use gpui_kit::{DummyKeyboardMapper, KeyBinding, KeyContext, Keymap, Keystroke};
    #[test]
    fn effective_keymap_has_no_old_binding_and_contexts_protect_text() -> anyhow::Result<()> {
        let overrides = ShortcutOverrides {
            bindings: vec![ShortcutOverride {
                command: CommandId::Open,
                keys: vec!["cmd-alt-o".into()],
            }],
        };
        let effective =
            overrides.resolve(|s| crate::settings::normalize_key(s, &DummyKeyboardMapper))?;
        let mut bindings: Vec<KeyBinding> = effective
            .iter()
            .map(|b| {
                KeyBinding::new(
                    &b.key,
                    RunCommand { id: b.command },
                    Some(scope(b.command.spec().scope)),
                )
            })
            .collect();
        bindings.insert(
            0,
            KeyBinding::new(
                "enter",
                input::Enter {
                    secondary: false,
                    shift: false,
                },
                Some("Input"),
            ),
        );
        let keymap = Keymap::new(bindings);
        let find = |key: &str, context: &str| {
            keymap
                .bindings_for_input(
                    &[Keystroke::parse(key).unwrap()],
                    &context
                        .split(" > ")
                        .map(|c| KeyContext::parse(c).unwrap())
                        .collect::<Vec<_>>(),
                )
                .0
                .len()
        };
        assert_eq!(find("cmd-o", "KasaneEditor"), 0);
        assert_eq!(find("cmd-alt-o", "KasaneEditor > Input"), 1);
        assert_eq!(find("v", "KasaneEditor > Input"), 0);
        assert_eq!(find("cmd-z", "KasaneEditor > Input"), 0);
        assert_eq!(find("v", "KasaneEditor > KasaneCanvas"), 1);
        assert_eq!(find("cmd-s", "KasaneEditor KasaneModal"), 0);
        assert_eq!(find("escape", "KasaneEditor > Input"), 1);
        assert_eq!(find("enter", "KasaneEditor > Input"), 2);
        let contexts = [
            KeyContext::parse("KasaneEditor")?,
            KeyContext::parse("Input")?,
        ];
        let matched = keymap
            .bindings_for_input(&[Keystroke::parse("enter")?], &contexts)
            .0;
        assert!(
            matched[0]
                .action()
                .as_any()
                .downcast_ref::<RunCommand>()
                .is_some_and(|a| a.id == CommandId::CommitProperty)
        );
        assert_eq!(find("enter", "KasaneEditor KasaneModal > Input"), 1);
        assert_eq!(find("v", "KasaneEditor > KasaneCanvas > Input"), 0);
        assert_eq!(find("escape", "KasaneEditor > KasaneCanvas"), 1);
        assert_eq!(
            keymap
                .bindings_for_action(&RunCommand {
                    id: CommandId::Open
                })
                .count(),
            1
        );
        assert_eq!(
            keymap
                .bindings_for_action(&RunCommand {
                    id: CommandId::Save
                })
                .count(),
            1
        );
        Ok(())
    }
}
