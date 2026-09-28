use editor_core::{ConfigStore, EffectiveBinding, ShortcutOverrides};
use gpui_kit::{App, Global, KeybindingKeystroke, Keystroke, PlatformKeyboardMapper};

#[derive(Clone)]
pub struct Settings {
    pub store: ConfigStore,
    pub bindings: Vec<EffectiveBinding>,
    pub notices: Vec<String>,
}
impl Global for Settings {}
impl Settings {
    pub fn load(cx: &App) -> Self {
        let root = std::env::var_os("KASANE_CONFIG_DIR")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::path::PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                    .join("Library/Application Support/Kasane Editor")
            });
        Self::from_store(ConfigStore::new(root), cx.keyboard_mapper().as_ref())
    }
    pub fn from_store(store: ConfigStore, mapper: &dyn PlatformKeyboardMapper) -> Self {
        let normalize = |key: &str| normalize_key(key, mapper);
        let mut notices = Vec::new();
        let requested = store
            .read::<ShortcutOverrides>("keybindings.json")
            .and_then(|config| {
                let config = config.unwrap_or_default();
                let effective = config.resolve(normalize)?;
                Ok((config, effective))
            });
        let bindings = match requested {
            Ok((config, effective)) => {
                if let Err(e) = store.write("keybindings.last-good.json", &config) {
                    notices.push(e.to_string());
                }
                effective
            }
            Err(e) => {
                notices.push(e.to_string());
                store
                    .read::<ShortcutOverrides>("keybindings.last-good.json")
                    .ok()
                    .flatten()
                    .and_then(|c| c.resolve(normalize).ok())
                    .unwrap_or_else(|| {
                        ShortcutOverrides::default()
                            .resolve(normalize)
                            .expect("valid defaults")
                    })
            }
        };
        Self {
            store,
            bindings,
            notices,
        }
    }
}

pub fn normalize_key(key: &str, mapper: &dyn PlatformKeyboardMapper) -> anyhow::Result<String> {
    let key = Keystroke::parse(key)?;
    let named = [
        "escape",
        "enter",
        "tab",
        "backspace",
        "delete",
        "space",
        "up",
        "down",
        "left",
        "right",
        "home",
        "end",
        "pageup",
        "pagedown",
    ];
    anyhow::ensure!(
        key.key_char.is_none()
            && (key.key.chars().count() == 1
                || named.contains(&key.key.as_str())
                || key
                    .key
                    .strip_prefix('f')
                    .and_then(|n| n.parse::<u8>().ok())
                    .is_some_and(|n| (1..=24).contains(&n))),
        "Unknown key: {}",
        key.key
    );
    Ok(KeybindingKeystroke::new_with_mapper(key, true, mapper).unparse())
}

#[cfg(test)]
mod tests {
    use super::{Settings, normalize_key};
    use editor_core::{CommandId, ConfigStore, ShortcutOverride, ShortcutOverrides};
    use gpui_kit::DummyKeyboardMapper;
    #[test]
    fn failed_config_keeps_previous_valid_bindings_and_defaults_can_be_restored()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let store = ConfigStore::new(dir.path());
        let valid = ShortcutOverrides {
            bindings: vec![
                ShortcutOverride {
                    command: CommandId::Open,
                    keys: vec!["cmd-alt-o".into()],
                },
                ShortcutOverride {
                    command: CommandId::Save,
                    keys: vec![],
                },
            ],
        };
        store.write("keybindings.json", &valid)?;
        let first = Settings::from_store(store.clone(), &DummyKeyboardMapper);
        assert!(first.notices.is_empty());
        assert!(!first.bindings.iter().any(|b| b.command == CommandId::Save));
        let conflict = ShortcutOverrides {
            bindings: vec![ShortcutOverride {
                command: CommandId::Open,
                keys: vec!["cmd-s".into()],
            }],
        };
        store.write("keybindings.json", &conflict)?;
        let second = Settings::from_store(store.clone(), &DummyKeyboardMapper);
        assert!(!second.notices.is_empty());
        assert_eq!(
            first
                .bindings
                .iter()
                .map(|b| (&b.key, b.command))
                .collect::<Vec<_>>(),
            second
                .bindings
                .iter()
                .map(|b| (&b.key, b.command))
                .collect::<Vec<_>>()
        );
        std::fs::write(dir.path().join("keybindings.json"), "{")?;
        assert!(
            !Settings::from_store(store.clone(), &DummyKeyboardMapper)
                .notices
                .is_empty()
        );
        store.write("keybindings.json", &ShortcutOverrides::default())?;
        let restored = Settings::from_store(store, &DummyKeyboardMapper);
        assert!(restored.notices.is_empty());
        assert!(
            restored
                .bindings
                .iter()
                .any(|b| b.command == CommandId::Save)
        );
        Ok(())
    }
    #[test]
    fn gpui_normalization_diagnoses_alias_conflicts_and_invalid_keys() -> anyhow::Result<()> {
        assert_eq!(
            normalize_key("secondary-o", &DummyKeyboardMapper)?,
            normalize_key("cmd-o", &DummyKeyboardMapper)?
        );
        assert!(normalize_key("cmd-this-key-does-not-exist", &DummyKeyboardMapper).is_err());
        assert!(normalize_key("cmd-unicorn", &DummyKeyboardMapper).is_err());
        let config = ShortcutOverrides {
            bindings: vec![ShortcutOverride {
                command: CommandId::Open,
                keys: vec!["secondary-s".into()],
            }],
        };
        assert!(
            config
                .resolve(|s| normalize_key(s, &DummyKeyboardMapper))
                .is_err()
        );
        for key in ["a", "cmd-c", "ctrl-a"] {
            let config = ShortcutOverrides {
                bindings: vec![ShortcutOverride {
                    command: CommandId::CancelProperty,
                    keys: vec![key.into()],
                }],
            };
            assert!(
                config
                    .resolve(|s| normalize_key(s, &DummyKeyboardMapper))
                    .is_err()
            );
        }
        Ok(())
    }
}
