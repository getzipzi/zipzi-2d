//! Host-selected keyboard policy. Expand semantic modifiers before using a
//! platform keyboard mapper, so pure tests do not depend on GPUI's build target.
use crate::CommandId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutPlatform {
    Macos,
    Windows,
    Linux,
}

#[derive(Clone, Copy, Debug)]
pub struct KeyboardPolicy {
    pub platform: ShortcutPlatform,
    pub secondary: &'static str,
    pub reserved: &'static [&'static str],
    pub text_editing: &'static [&'static str],
    pub additional_defaults: &'static [(CommandId, &'static str)],
}

impl KeyboardPolicy {
    pub const fn for_platform(platform: ShortcutPlatform) -> Self {
        use ShortcutPlatform::*;
        match platform {
            Macos => Self {
                platform,
                secondary: "cmd",
                reserved: &["cmd-h", "cmd-alt-h", "cmd-m"],
                text_editing: &[
                    "secondary-z",
                    "secondary-shift-z",
                    "secondary-c",
                    "secondary-v",
                    "secondary-x",
                    "secondary-a",
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
                    "ctrl-t",
                ],
                additional_defaults: &[],
            },
            Windows => Self {
                platform,
                secondary: "ctrl",
                reserved: &["alt-f4", "cmd-l", "ctrl-alt-delete"],
                text_editing: &[
                    "secondary-z",
                    "secondary-shift-z",
                    "secondary-c",
                    "secondary-v",
                    "secondary-x",
                    "secondary-a",
                    "ctrl-y",
                    "ctrl-left",
                    "ctrl-right",
                    "ctrl-backspace",
                    "ctrl-delete",
                    "ctrl-home",
                    "ctrl-end",
                ],
                additional_defaults: &[(CommandId::Redo, "ctrl-y")],
            },
            Linux => Self {
                platform,
                secondary: "ctrl",
                reserved: &["alt-f4", "cmd-l", "ctrl-alt-delete"],
                text_editing: &[
                    "secondary-z",
                    "secondary-shift-z",
                    "secondary-c",
                    "secondary-v",
                    "secondary-x",
                    "secondary-a",
                    "ctrl-left",
                    "ctrl-right",
                    "ctrl-backspace",
                    "ctrl-delete",
                    "ctrl-home",
                    "ctrl-end",
                ],
                additional_defaults: &[],
            },
        }
    }

    /// Canonicalize modifier aliases only. Key names, symbols and keyboard
    /// layout normalization are still the host mapper's responsibility.
    pub fn expand(&self, source: &str) -> String {
        let mut remaining = source;
        let mut flags = [false; 5]; // fn, control, alt, platform, shift
        while let Some((prefix, rest)) = remaining.split_once('-') {
            let prefix = prefix.to_ascii_lowercase();
            let modifier = if prefix == "secondary" {
                self.secondary
            } else {
                &prefix
            };
            let index = match modifier {
                "fn" => 0,
                "ctrl" => 1,
                "alt" => 2,
                "cmd" | "super" | "win" => 3,
                "shift" => 4,
                _ => break,
            };
            flags[index] = true;
            remaining = rest;
        }
        let mut result = String::new();
        for (flag, name) in flags
            .into_iter()
            .zip(["fn-", "ctrl-", "alt-", "cmd-", "shift-"])
        {
            if flag {
                result.push_str(name);
            }
        }
        result.push_str(remaining);
        result
    }
}
