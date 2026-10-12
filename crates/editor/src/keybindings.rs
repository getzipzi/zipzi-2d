//! Editor shortcut installation, scope matching, and repeat handling.
use super::*;
use crate::actions::{Hide, HideOthers, Minimize, RunCommand};
use editor_core::{COMMANDS, CommandScope};
use gpui_kit::component::WindowExt;

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
}

pub(super) fn reset(cx: &mut App) -> anyhow::Result<()> {
    let store = cx.global::<settings::Settings>().store.clone();
    store.write(
        "keybindings.json",
        &editor_core::ShortcutOverrides::default(),
    )?;
    let settings = settings::Settings::from_store(store, cx.keyboard_mapper().as_ref());
    cx.set_global(settings);
    let base = cx.global::<KitBindings>().0.clone();
    cx.clear_key_bindings();
    cx.bind_keys(base);
    install(cx);
    Ok(())
}

pub(super) fn show(window: &mut Window, cx: &mut App) {
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

impl EditorView {
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
        let text = self.has_focused_input(window, cx);
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
    use super::{RunCommand, scope};
    use editor_core::{CommandId, ShortcutOverride, ShortcutOverrides};
    use gpui_kit::component::input;
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
        assert_eq!(find("space", "KasaneEditor > Input"), 0);
        assert_eq!(find("space", "KasaneEditor"), 1);
        assert_eq!(find("space", "KasaneEditor > KasaneCanvas"), 1);
        assert_eq!(find("space", "KasaneEditor KasaneModal"), 0);
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
