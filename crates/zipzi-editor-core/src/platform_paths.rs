//! Persistent platform directories. Missing homes are errors, never temporary storage.
use anyhow::{Context, Result, ensure};
use std::path::PathBuf;

fn absolute(name: &str, value: Option<std::ffi::OsString>) -> Result<PathBuf> {
    let path = PathBuf::from(value.with_context(|| format!("{name} is not configured"))?);
    ensure!(path.is_absolute(), "{name} must be an absolute path");
    Ok(path)
}

pub fn user_home() -> Result<PathBuf> {
    let name = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    absolute(name, std::env::var_os(name))
}

pub fn config_directory() -> Result<PathBuf> {
    config_from(|name| std::env::var_os(name))
}

fn config_from(env: impl Fn(&str) -> Option<std::ffi::OsString>) -> Result<PathBuf> {
    if let Some(value) = env("ZIPZI_CONFIG_DIR") {
        return absolute("ZIPZI_CONFIG_DIR", Some(value));
    }
    if cfg!(windows) {
        return Ok(absolute("APPDATA", env("APPDATA"))?.join("Zipzi Editor"));
    }
    if cfg!(target_os = "macos") {
        return Ok(absolute("HOME", env("HOME"))?.join("Library/Application Support/Zipzi Editor"));
    }
    match env("XDG_CONFIG_HOME") {
        Some(value) => Ok(absolute("XDG_CONFIG_HOME", Some(value))?.join("zipzi-editor")),
        None => Ok(absolute("HOME", env("HOME"))?.join(".config/zipzi-editor")),
    }
}

pub fn cache_directory() -> Result<PathBuf> {
    cache_from(|name| std::env::var_os(name))
}

fn cache_from(env: impl Fn(&str) -> Option<std::ffi::OsString>) -> Result<PathBuf> {
    if let Some(value) = env("ZIPZI_CACHE_DIR") {
        return absolute("ZIPZI_CACHE_DIR", Some(value));
    }
    if cfg!(windows) {
        return Ok(absolute("LOCALAPPDATA", env("LOCALAPPDATA"))?.join("Zipzi Editor/Cache"));
    }
    if cfg!(target_os = "macos") {
        return Ok(absolute("HOME", env("HOME"))?.join("Library/Caches/Zipzi Editor"));
    }
    match env("XDG_CACHE_HOME") {
        Some(value) => Ok(absolute("XDG_CACHE_HOME", Some(value))?.join("zipzi-editor")),
        None => Ok(absolute("HOME", env("HOME"))?.join(".cache/zipzi-editor")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_directory_never_falls_back_to_temp() {
        assert!(config_from(|_| None).is_err());
        assert!(config_from(|key| (key == "ZIPZI_CONFIG_DIR").then(|| "relative".into())).is_err());
        assert!(cache_from(|_| None).is_err());
        assert!(cache_from(|key| (key == "ZIPZI_CACHE_DIR").then(|| "relative".into())).is_err());
    }

    #[test]
    fn explicit_cache_override_needs_no_home() {
        let expected = std::env::temp_dir().join("中文 cache");
        assert_eq!(
            cache_from(|key| (key == "ZIPZI_CACHE_DIR").then(|| expected.clone().into_os_string()))
                .unwrap(),
            expected
        );
    }

    #[test]
    fn native_cache_location_uses_platform_environment() {
        let home = std::env::temp_dir().join("platform user");
        let expected = if cfg!(windows) {
            home.join("Zipzi Editor/Cache")
        } else if cfg!(target_os = "macos") {
            home.join("Library/Caches/Zipzi Editor")
        } else {
            home.join(".cache/zipzi-editor")
        };
        assert_eq!(
            cache_from(
                |key| matches!(key, "HOME" | "LOCALAPPDATA").then(|| home.clone().into_os_string())
            )
            .unwrap(),
            expected
        );
    }

    #[test]
    fn explicit_config_override_needs_no_home() {
        let expected = std::env::temp_dir().join("中文 settings");
        assert_eq!(
            config_from(
                |key| (key == "ZIPZI_CONFIG_DIR").then(|| expected.clone().into_os_string())
            )
            .unwrap(),
            expected
        );
    }
}
