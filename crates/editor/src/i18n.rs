//! Editor locale selection. The catalogs live in `crates/editor/locales/`.
//! `activate` sets rust-i18n's process-wide locale, also used by GPUI Kit.

/// Select a shipped catalog for a BCP-47 locale or a POSIX-style locale.
/// Traditional Chinese has no catalog yet and therefore uses English.
fn match_supported(code: &str) -> Option<&'static str> {
    let code = code.split(['.', '@']).next()?.replace('_', "-");
    let mut parts = code.split('-');
    let language = parts.next()?;
    if language.eq_ignore_ascii_case("en") {
        return Some("en");
    }
    if !language.eq_ignore_ascii_case("zh") {
        return None;
    }

    let mut script = None;
    let mut region = None;
    for part in parts {
        if part.eq_ignore_ascii_case("Hans") {
            script = Some("Hans");
        } else if part.eq_ignore_ascii_case("Hant") {
            script = Some("Hant");
        } else if part.eq_ignore_ascii_case("CN") || part.eq_ignore_ascii_case("SG") {
            region = Some("simplified");
        } else if part.eq_ignore_ascii_case("TW")
            || part.eq_ignore_ascii_case("HK")
            || part.eq_ignore_ascii_case("MO")
        {
            region = Some("traditional");
        }
    }
    match (script, region) {
        (Some("Hant"), _) | (None, Some("traditional")) => None,
        _ => Some("zh-CN"),
    }
}

/// Resolve an explicit setting, then the system locale, then English.
/// An unsupported explicit setting behaves like "follow system".
fn resolve(language: Option<&str>) -> &'static str {
    language
        .and_then(match_supported)
        .or_else(|| {
            sys_locale::get_locale()
                .as_deref()
                .and_then(match_supported)
        })
        .unwrap_or("en")
}

/// Set the process-wide locale before building menus or windows. Callers that
/// change languages later must refresh their GPUI windows to re-render text.
pub(super) fn activate(language: Option<&str>) {
    let locale = resolve(language);
    rust_i18n::set_locale(locale);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn locale_variants() {
        assert_eq!(match_supported("en-US"), Some("en"));
        assert_eq!(match_supported("zh_CN.UTF-8"), Some("zh-CN"));
        assert_eq!(match_supported("zh-Hans-SG"), Some("zh-CN"));
        assert_eq!(match_supported("zh-Hans-HK"), Some("zh-CN"));
        assert_eq!(match_supported("zh-Hant-TW"), None);
        assert_eq!(match_supported("zh-Hant-CN"), None);
        assert_eq!(match_supported("zh-HK"), None);
        assert_eq!(match_supported("fr-FR"), None);
    }

    #[test]
    fn catalog_keys_and_placeholders_match() {
        let en: toml::Value = include_str!("../locales/en.toml").parse().unwrap();
        let zh: toml::Value = include_str!("../locales/zh-CN.toml").parse().unwrap();
        fn entries(value: &toml::Value) -> BTreeMap<String, Vec<String>> {
            let mut result = BTreeMap::new();
            for (domain, messages) in value.as_table().unwrap() {
                for (key, text) in messages.as_table().unwrap() {
                    let text = text.as_str().unwrap();
                    let mut placeholders = Vec::new();
                    let mut rest = text;
                    while let Some(start) = rest.find("%{") {
                        rest = &rest[start + 2..];
                        let end = rest.find('}').expect("closed placeholder");
                        placeholders.push(rest[..end].to_owned());
                        rest = &rest[end + 1..];
                    }
                    placeholders.sort();
                    result.insert(format!("{domain}.{key}"), placeholders);
                }
            }
            result
        }
        let english = entries(&en);
        assert!(!english.is_empty());
        assert_eq!(entries(&zh), english);
    }
}
