//! Versioned application preferences. No model path or SDK history is involved.
use crate::{COMMANDS, CommandId, CommandScope};
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
}
#[derive(Clone, Debug)]
pub struct EffectiveBinding {
    pub command: CommandId,
    pub key: String,
}
impl ShortcutOverrides {
    /// Parse/normalize through the host platform. Validation is all-or-nothing.
    pub fn resolve(
        &self,
        normalize: impl Fn(&str) -> Result<String>,
    ) -> Result<Vec<EffectiveBinding>> {
        let mut overrides = HashMap::new();
        for binding in &self.bindings {
            ensure!(
                overrides.insert(binding.command, &binding.keys).is_none(),
                "Duplicate command override"
            );
        }
        let mut result: Vec<EffectiveBinding> = Vec::new();
        for spec in COMMANDS {
            let keys = overrides
                .get(&spec.id)
                .map(|k| k.iter().map(String::as_str).collect::<Vec<_>>())
                .unwrap_or_else(|| spec.keys.to_vec());
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
                    !["cmd-h", "alt-cmd-h", "cmd-alt-h", "cmd-m"].contains(&key.as_str()),
                    "Shortcut conflicts with a platform command: {key}"
                );
                if matches!(spec.scope, CommandScope::Workspace | CommandScope::Property) {
                    ensure!(
                        ![
                            "cmd-z",
                            "cmd-shift-z",
                            "cmd-c",
                            "cmd-v",
                            "cmd-x",
                            "cmd-a",
                            "ctrl-a",
                            "ctrl-e",
                            "ctrl-b",
                            "ctrl-f",
                            "ctrl-p",
                            "ctrl-n",
                            "ctrl-d",
                            "ctrl-h",
                            "ctrl-k",
                            "ctrl-y",
                            "ctrl-t"
                        ]
                        .contains(&key.as_str()),
                        "Shortcut conflicts with text editing: {key}"
                    );
                }
                if matches!(spec.scope, CommandScope::Workspace | CommandScope::Property) {
                    ensure!(
                        (spec.scope == CommandScope::Property
                            && ["enter", "escape"].contains(&key.as_str()))
                            || key.contains("cmd-")
                            || key.contains("ctrl-")
                            || (key
                                .strip_prefix('f')
                                .and_then(|n| n.parse::<u8>().ok())
                                .is_some_and(|n| (1..=24).contains(&n))),
                        "Shortcut would intercept text: {key}"
                    );
                }
                for other in &result {
                    let a = other.command.spec().scope;
                    let b = spec.scope;
                    let overlap = a == b
                        || a == CommandScope::Workspace
                        || b == CommandScope::Workspace
                        || (a != CommandScope::Property && b != CommandScope::Property);
                    if other.key == key && overlap {
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
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn read<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>> {
        let path = self.root.join(name);
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
            std::fs::rename(&temporary, self.root.join(name))?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(temporary);
        }
        result
    }
}
