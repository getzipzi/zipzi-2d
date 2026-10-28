//! Versioned application preferences. No model path or SDK history is involved.
use crate::{COMMANDS, CommandId, CommandScope, KeyboardPolicy, ShortcutPlatform};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortcutOverrides {
    pub bindings: Vec<ShortcutOverride>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShortcutOverride {
    pub command: CommandId,
    pub keys: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<ShortcutPlatform>,
}
#[derive(Clone, Debug)]
pub struct EffectiveBinding {
    pub command: CommandId,
    pub key: String,
}
impl ShortcutOverrides {
    /// Common defaults, platform additions, common overrides, then platform
    /// overrides. Validation is atomic; normalized runtime keys are never saved.
    pub fn resolve(
        &self,
        policy: &KeyboardPolicy,
        normalize: impl Fn(&str) -> Result<String>,
    ) -> Result<Vec<EffectiveBinding>> {
        let normalize = |key: &str| normalize(&policy.expand(key));
        let mut overrides = HashMap::new();
        for binding in &self.bindings {
            ensure!(
                overrides
                    .insert((binding.command, binding.platform), &binding.keys)
                    .is_none(),
                "Duplicate command override for {:?} ({:?})",
                binding.command,
                binding.platform
            );
        }
        let reserved = policy
            .reserved
            .iter()
            .map(|key| normalize(key))
            .collect::<Result<HashSet<_>>>()?;
        let text_editing = policy
            .text_editing
            .iter()
            .map(|key| normalize(key))
            .collect::<Result<HashSet<_>>>()?;
        let mut result: Vec<EffectiveBinding> = Vec::new();
        for spec in COMMANDS {
            let keys = overrides
                .get(&(spec.id, Some(policy.platform)))
                .or_else(|| overrides.get(&(spec.id, None)))
                .map(|keys| keys.iter().map(String::as_str).collect::<Vec<_>>())
                .unwrap_or_else(|| {
                    spec.keys
                        .iter()
                        .copied()
                        .chain(
                            policy
                                .additional_defaults
                                .iter()
                                .filter(|(command, _)| *command == spec.id)
                                .map(|(_, key)| *key),
                        )
                        .collect()
                });
            let mut own = HashSet::new();
            for key in keys {
                ensure!(
                    !key.trim().is_empty() && key.split_whitespace().count() == 1,
                    "Only single-stroke shortcuts are supported: {key}"
                );
                let key = normalize(key).with_context(|| format!("Invalid shortcut: {key}"))?;
                ensure!(
                    own.insert(key.clone()),
                    "Duplicate shortcut for {:?}: {key}",
                    spec.id
                );
                ensure!(
                    !reserved.contains(&key),
                    "Shortcut conflicts with a platform command: {key}"
                );
                if matches!(spec.scope, CommandScope::Window | CommandScope::Property) {
                    ensure!(
                        !text_editing.contains(&key),
                        "Shortcut conflicts with text editing: {key}"
                    );
                    ensure!(
                        (spec.scope == CommandScope::Property
                            && ["enter", "escape"].contains(&key.as_str()))
                            || key
                                .split('-')
                                .any(|part| ["cmd", "super", "win", "ctrl"].contains(&part))
                            || key
                                .strip_prefix('f')
                                .and_then(|n| n.parse::<u8>().ok())
                                .is_some_and(|n| (1..=24).contains(&n)),
                        "Shortcut would intercept text: {key}"
                    );
                }
                for other in &result {
                    if other.key == key && other.command.spec().scope.overlaps(spec.scope) {
                        bail!(
                            "Shortcut conflict: {key} ({:?}, {:?})",
                            other.command,
                            spec.id
                        );
                    }
                }
                result.push(EffectiveBinding {
                    command: spec.id,
                    key,
                });
            }
        }
        Ok(result)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Versioned<T> {
    version: u32,
    data: T,
}
#[derive(Clone)]
pub struct ConfigStore {
    root: PathBuf,
}
impl ConfigStore {
    pub fn from_platform() -> Result<Self> {
        Ok(Self::new(crate::platform_paths::config_directory()?))
    }

    fn path(&self, name: &str) -> Result<PathBuf> {
        ensure!(
            !name.is_empty() && !name.contains(['/', '\\', ':']) && name != "." && name != "..",
            "Config name must be a single filename"
        );
        Ok(self.root.join(name))
    }
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn read<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>> {
        let path = self.path(name)?;
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let value: Versioned<T> = serde_json::from_slice(&bytes)
            .with_context(|| format!("Invalid config {}", path.display()))?;
        ensure!(
            value.version == 1,
            "Unsupported config version: {}",
            value.version
        );
        Ok(Some(value.data))
    }
    pub fn write<T: Serialize>(&self, name: &str, data: &T) -> Result<()> {
        use std::io::Write;
        let destination = self.path(name)?;
        std::fs::create_dir_all(&self.root)?;
        let bytes = serde_json::to_vec_pretty(&Versioned { version: 1, data })?;
        let temporary = self
            .root
            .join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4()));
        let result = (|| {
            let mut file = std::fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            drop(file);
            std::fs::rename(&temporary, &destination)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }
}

#[cfg(test)]
mod storage_tests {
    use super::*;

    #[test]
    fn overwrite_unicode_config_and_reject_path_escape() {
        let folder = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(folder.path().join("中文 settings"));
        store.write("preferences.json", &1u32).unwrap();
        store.write("preferences.json", &2u32).unwrap();
        assert_eq!(store.read::<u32>("preferences.json").unwrap(), Some(2));
        for name in [
            "../escape",
            "..\\escape",
            "C:escape",
            "",
            ".",
            "..",
            "/absolute",
        ] {
            assert!(store.write(name, &0u32).is_err());
            assert!(store.read::<u32>(name).is_err());
        }
        assert_eq!(std::fs::read_dir(store.root()).unwrap().count(), 1);
    }

    #[cfg(windows)]
    #[test]
    fn occupied_config_preserves_old_content_and_cleans_staging() {
        use std::os::windows::fs::OpenOptionsExt;
        let folder = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(folder.path());
        store.write("settings.json", &1u32).unwrap();
        let occupied = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(folder.path().join("settings.json"))
            .unwrap();
        assert!(store.write("settings.json", &2u32).is_err());
        drop(occupied);
        assert_eq!(store.read::<u32>("settings.json").unwrap(), Some(1));
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
        store.write("settings.json", &3u32).unwrap();
        assert_eq!(store.read::<u32>("settings.json").unwrap(), Some(3));
    }
}
