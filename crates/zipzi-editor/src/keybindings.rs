//! Editor shortcut installation, scope matching, and repeat handling.
use super::*;
use crate::actions::RunCommand;
#[cfg(target_os = "macos")]
use crate::actions::{Hide, HideOthers, Minimize};
use gpui_kit::component::WindowExt;
use zipzi_editor_core::{COMMANDS, CommandScope};

struct KitBindings(Vec<KeyBinding>);
impl Global for KitBindings {}
fn scope(scope: CommandScope) -> &'static str {
    match scope {
        CommandScope::Property => "(ZipziEditor > Input) && !ZipziModal && !(List > Input)",
        CommandScope::Palette => "(ZipziEditor > ZipziPalette) && !Input && !ZipziModal",
        _ => "ZipziShortcutDisplay",
    }
}
fn install(cx: &mut App) {
    // Keep action bindings for native accelerators/help. Window routing resolves
    // business keys once from hover/capture and receives the real repeat flag.
    // Properties and palette commands use GPUI's focused control keymap.
    let bindings = cx
        .global::<settings::Settings>()
        .bindings
        .iter()
        .map(|binding| {
            KeyBinding::load(
                &binding.key,
                Box::new(RunCommand::new(binding.command)),
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
    #[cfg(target_os = "macos")]
    cx.bind_keys([
        KeyBinding::new("cmd-m", Minimize, Some("ZipziEditor")),
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
        &zipzi_editor_core::ShortcutOverrides::default(),
    )?;
    reload(cx);
    Ok(())
}

pub(super) fn reload(cx: &mut App) {
    let store = cx.global::<settings::Settings>().store.clone();
    let settings = settings::Settings::from_store(store, cx.keyboard_mapper().as_ref());
    cx.set_global(settings);
    let base = cx.global::<KitBindings>().0.clone();
    cx.clear_key_bindings();
    cx.bind_keys(base);
    install(cx);
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
                    .map(|b| gpui_kit::Keystroke::parse(&b.key)
                        .expect("validated binding")
                        .to_string())
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

#[cfg(test)]
mod tests {
    use super::{RunCommand, scope};
    use gpui_kit::component::input;
    use gpui_kit::{DummyKeyboardMapper, KeyBinding, KeyContext, Keymap, Keystroke};
    use zipzi_editor_core::{CommandId, ShortcutOverride, ShortcutOverrides};
    #[test]
    fn effective_keymap_has_no_old_binding_and_contexts_protect_text() -> anyhow::Result<()> {
        let overrides = ShortcutOverrides {
            bindings: vec![ShortcutOverride {
                platform: None,
                command: CommandId::Open,
                keys: vec!["cmd-alt-o".into()],
            }],
        };
        let effective = overrides.resolve(&crate::settings::keyboard_policy(), |s| {
            crate::settings::normalize_key(s, &DummyKeyboardMapper)
        })?;
        let mut bindings: Vec<KeyBinding> = effective
            .iter()
            .map(|b| {
                KeyBinding::new(
                    &b.key,
                    RunCommand::new(b.command),
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
        assert_eq!(find("cmd-o", "ZipziEditor"), 0);
        assert_eq!(find("cmd-alt-o", "ZipziEditor > Input"), 0);
        assert_eq!(find("v", "ZipziEditor > Input"), 0);
        assert_eq!(find("cmd-z", "ZipziEditor > Input"), 0);
        assert_eq!(find("space", "ZipziEditor > Input"), 0);
        assert_eq!(find("space", "ZipziEditor"), 0);
        assert_eq!(find("space", "ZipziEditor > ZipziViewport"), 0);
        assert_eq!(find("space", "ZipziEditor ZipziModal"), 0);
        assert_eq!(find("v", "ZipziEditor > ZipziViewport"), 0);
        assert_eq!(find("cmd-c", "ZipziEditor > ZipziTimeline"), 0);
        assert_eq!(find("cmd-c", "ZipziEditor > ZipziViewport"), 0);
        assert_eq!(find("cmd-c", "ZipziEditor > ZipziTimeline > Input"), 0);
        assert_eq!(find("left", "ZipziEditor > ZipziTimeline"), 0);
        assert_eq!(find("left", "ZipziEditor > ZipziViewport"), 0);
        assert_eq!(find("left", "ZipziEditor ZipziModal > ZipziTimeline"), 0);
        assert_eq!(find("cmd-s", "ZipziEditor ZipziModal"), 0);
        assert_eq!(find("alt-up", "ZipziEditor > ZipziPalette"), 1);
        assert_eq!(find("alt-down", "ZipziEditor > ZipziPalette"), 1);
        assert_eq!(find("alt-up", "ZipziEditor > ZipziPalette > Input"), 0);
        assert_eq!(find("alt-up", "ZipziEditor > ZipziTimeline"), 0);
        assert_eq!(find("alt-up", "ZipziEditor ZipziModal > ZipziPalette"), 0);
        assert_eq!(find("escape", "ZipziEditor > Input"), 1);
        assert_eq!(find("enter", "ZipziEditor > Input"), 2);
        let contexts = [
            KeyContext::parse("ZipziEditor")?,
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
        assert_eq!(find("enter", "ZipziEditor ZipziModal > Input"), 1);
        assert_eq!(find("enter", "ZipziEditor > List > Input"), 1);
        assert_eq!(find("escape", "ZipziEditor > List > Input"), 0);
        assert_eq!(find("v", "ZipziEditor > ZipziViewport > Input"), 0);
        assert_eq!(find("escape", "ZipziEditor > ZipziViewport"), 0);
        assert_eq!(
            keymap
                .bindings_for_action(&RunCommand::new(CommandId::Open))
                .count(),
            1
        );
        assert_eq!(
            keymap
                .bindings_for_action(&RunCommand::new(CommandId::Save))
                .count(),
            1
        );
        let editor = zipzi_editor_core::Editor::new()?;
        let saved_menu_action = RunCommand::targeted(
            editor
                .command(CommandId::Save)
                .with_source(zipzi_editor_core::CommandSource::NativeMenu),
        );
        assert_eq!(keymap.bindings_for_action(&saved_menu_action).count(), 1);
        Ok(())
    }
}
