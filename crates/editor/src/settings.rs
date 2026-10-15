use editor_core::{ConfigStore, EffectiveBinding, RecentFiles, ShortcutOverrides, WorkspaceKind};
use gpui_kit::{App, Global, KeybindingKeystroke, Keystroke, PlatformKeyboardMapper};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Default, Deserialize, Serialize)]
struct LanguagePreference {
    language: Option<String>,
}
#[derive(Default, Deserialize, Serialize)]
struct WorkspacePreference {
    workspace: WorkspaceKind,
}

#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub enum UiScale {
    #[default]
    #[serde(rename = "100")]
    Standard,
    #[serde(rename = "110")]
    Large,
    #[serde(rename = "125")]
    Largest,
}
impl UiScale {
    pub const ALL: [Self; 3] = [Self::Standard, Self::Large, Self::Largest];
    pub fn factor(self) -> f32 {
        match self {
            Self::Standard => 1.,
            Self::Large => 1.1,
            Self::Largest => 1.25,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Standard => "100%",
            Self::Large => "110%",
            Self::Largest => "125%",
        }
    }
}
#[derive(Default, Deserialize, Serialize)]
struct AppearancePreference {
    ui_scale: UiScale,
}

#[derive(Clone)]
pub struct Settings {
    pub store: ConfigStore,
    pub bindings: Vec<EffectiveBinding>,
    pub recent_files: RecentFiles,
    pub language: Option<String>,
    pub workspace: WorkspaceKind,
    pub ui_scale: UiScale,
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
        let recent_files = store
            .read::<RecentFiles>("recent-files.json")
            .ok()
            .flatten()
            .unwrap_or_default();
        let language = match store.read::<LanguagePreference>("language.json") {
            Ok(preference) => preference.and_then(|preference| preference.language),
            Err(error) => {
                notices.push(error.to_string());
                None
            }
        };
        let workspace = match store.read::<WorkspacePreference>("workspace.json") {
            Ok(preference) => preference.unwrap_or_default().workspace,
            Err(error) => {
                notices.push(error.to_string());
                WorkspaceKind::default()
            }
        };
        let ui_scale = match store.read::<AppearancePreference>("appearance.json") {
            Ok(preference) => preference.unwrap_or_default().ui_scale,
            Err(error) => {
                notices.push(error.to_string());
                UiScale::default()
            }
        };
        Self {
            store,
            bindings,
            recent_files,
            language,
            workspace,
            ui_scale,
            notices,
        }
    }

    pub fn set_ui_scale(&mut self, ui_scale: UiScale) -> anyhow::Result<()> {
        self.store
            .write("appearance.json", &AppearancePreference { ui_scale })?;
        self.ui_scale = ui_scale;
        Ok(())
    }

    pub fn set_workspace(&mut self, workspace: WorkspaceKind) -> anyhow::Result<()> {
        self.store
            .write("workspace.json", &WorkspacePreference { workspace })?;
        self.workspace = workspace;
        Ok(())
    }

    pub fn set_language(&mut self, language: Option<String>) -> anyhow::Result<()> {
        self.store.write(
            "language.json",
            &LanguagePreference {
                language: language.clone(),
            },
        )?;
        self.language = language;
        Ok(())
    }

    pub fn push_recent_file(&mut self, path: impl AsRef<Path>) -> anyhow::Result<()> {
        self.recent_files.push(path.as_ref());
        self.store.write("recent-files.json", &self.recent_files)
    }

    pub fn remove_recent_file(&mut self, path: &Path) -> anyhow::Result<()> {
        self.recent_files.remove(path);
        self.store.write("recent-files.json", &self.recent_files)
    }

    pub fn clear_recent_files(&mut self) -> anyhow::Result<()> {
        self.recent_files.clear();
        self.store.write("recent-files.json", &self.recent_files)
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
    fn language_preference_survives_reload() -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let store = ConfigStore::new(dir.path());
        let mut settings = Settings::from_store(store.clone(), &DummyKeyboardMapper);
        assert_eq!(settings.language, None);
        settings.set_language(Some("zh-CN".into()))?;
        assert_eq!(
            Settings::from_store(store.clone(), &DummyKeyboardMapper)
                .language
                .as_deref(),
            Some("zh-CN")
        );
        settings.set_language(None)?;
        assert_eq!(
            Settings::from_store(store, &DummyKeyboardMapper).language,
            None
        );
        Ok(())
    }
    #[test]
    fn workspace_preference_survives_reload_and_invalid_config_defaults_to_modeling()
    -> anyhow::Result<()> {
        let dir = tempfile::tempdir()?;
        let store = ConfigStore::new(dir.path());
        let mut settings = Settings::from_store(store.clone(), &DummyKeyboardMapper);
        assert_eq!(settings.workspace, editor_core::WorkspaceKind::Modeling);
        settings.set_workspace(editor_core::WorkspaceKind::Animation)?;
        assert_eq!(
            Settings::from_store(store.clone(), &DummyKeyboardMapper).workspace,
            editor_core::WorkspaceKind::Animation
        );
        std::fs::write(dir.path().join("workspace.json"), "{")?;
        let settings = Settings::from_store(store, &DummyKeyboardMapper);
        assert_eq!(settings.workspace, editor_core::WorkspaceKind::Modeling);
        assert!(!settings.notices.is_empty());
        Ok(())
    }
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
