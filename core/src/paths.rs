//! Config file discovery.

use std::path::PathBuf;

/// Environment variable that overrides the config path.
pub const CONFIG_ENV: &str = "SHELLCRAFT_CONFIG";

/// Resolve the config path from an environment lookup.
///
/// Order: `SHELLCRAFT_CONFIG`, `$XDG_CONFIG_HOME/shellcraft/shellcraft.toml`,
/// `$HOME/.config/shellcraft/shellcraft.toml`. The same `~/.config` layout is used on
/// macOS and Linux, matching the old zshkit location. Returns `None` if no home is known.
pub fn config_path(get: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let non_empty = |k: &str| get(k).filter(|v| !v.is_empty());
    if let Some(p) = non_empty(CONFIG_ENV) {
        return Some(PathBuf::from(p));
    }
    let base = match non_empty("XDG_CONFIG_HOME") {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from(non_empty("HOME")?).join(".config"),
    };
    Some(base.join("shellcraft").join("shellcraft.toml"))
}

/// Cache directory: `$XDG_CACHE_HOME/shellcraft` or `$HOME/.cache/shellcraft`.
pub fn cache_dir(get: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let non_empty = |k: &str| get(k).filter(|v| !v.is_empty());
    let base = match non_empty("XDG_CACHE_HOME") {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from(non_empty("HOME")?).join(".cache"),
    };
    Some(base.join("shellcraft"))
}

/// Data directory: `$XDG_DATA_HOME/shellcraft` or `$HOME/.local/share/shellcraft`.
pub fn data_dir(get: impl Fn(&str) -> Option<String>) -> Option<PathBuf> {
    let non_empty = |k: &str| get(k).filter(|v| !v.is_empty());
    let base = match non_empty("XDG_DATA_HOME") {
        Some(x) => PathBuf::from(x),
        None => PathBuf::from(non_empty("HOME")?)
            .join(".local")
            .join("share"),
    };
    Some(base.join("shellcraft"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |k| map.get(k).cloned()
    }

    #[test]
    fn explicit_override_wins() {
        let p = config_path(lookup(&[
            ("SHELLCRAFT_CONFIG", "/x/c.toml"),
            ("HOME", "/h"),
        ]));
        assert_eq!(p, Some(PathBuf::from("/x/c.toml")));
    }

    #[test]
    fn xdg_before_home() {
        let p = config_path(lookup(&[("XDG_CONFIG_HOME", "/xdg"), ("HOME", "/h")]));
        assert_eq!(p, Some(PathBuf::from("/xdg/shellcraft/shellcraft.toml")));
    }

    #[test]
    fn home_fallback() {
        let p = config_path(lookup(&[("HOME", "/h")]));
        assert_eq!(
            p,
            Some(PathBuf::from("/h/.config/shellcraft/shellcraft.toml"))
        );
    }

    #[test]
    fn empty_values_ignored() {
        let p = config_path(lookup(&[("XDG_CONFIG_HOME", ""), ("HOME", "/h")]));
        assert_eq!(
            p,
            Some(PathBuf::from("/h/.config/shellcraft/shellcraft.toml"))
        );
    }

    #[test]
    fn nothing_known() {
        assert_eq!(config_path(lookup(&[])), None);
    }

    #[test]
    fn cache_dir_order() {
        let p = cache_dir(lookup(&[("XDG_CACHE_HOME", "/c"), ("HOME", "/h")]));
        assert_eq!(p, Some(PathBuf::from("/c/shellcraft")));
        let p = cache_dir(lookup(&[("HOME", "/h")]));
        assert_eq!(p, Some(PathBuf::from("/h/.cache/shellcraft")));
        assert_eq!(cache_dir(lookup(&[])), None);
    }
}
