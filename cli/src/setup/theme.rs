//! `shellcraft theme`: list themes or switch the active one.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use shellcraft_core::Config;
use shellcraft_shell::available_themes;

/// Set `name` in the `[theme]` section of `text`, adding the section if missing.
fn set_theme_text(text: &str, name: &str) -> String {
    let new_line = format!("name = \"{name}\"");
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    if let Some(h) = lines.iter().position(|l| l.trim() == "[theme]") {
        let end = lines[h + 1..]
            .iter()
            .position(|l| l.trim_start().starts_with('['))
            .map_or(lines.len(), |i| h + 1 + i);
        match lines[h + 1..end]
            .iter()
            .position(|l| l.trim_start().starts_with("name"))
        {
            Some(i) => lines[h + 1 + i] = new_line,
            None => lines.insert(h + 1, new_line),
        }
    } else {
        lines.push(String::new());
        lines.push("[theme]".to_string());
        lines.push(new_line);
    }
    lines.join("\n") + "\n"
}

pub fn run(config_path: &Path, name: Option<String>, shell: Option<&str>) -> Result<()> {
    let text = fs::read_to_string(config_path).unwrap_or_default();
    let current = Config::from_toml(&text)
        .map(|c| c.theme.name)
        .unwrap_or_default();
    let names = available_themes(config_path);
    let Some(name) = name else {
        for n in &names {
            println!("{} {n}", if *n == current { "*" } else { " " });
        }
        println!("(more themes: `shellcraft search` lists the official themes)");
        return Ok(());
    };
    if !names.contains(&name) {
        bail!(
            "unknown theme `{name}` (available: {}; install more with a theme plugin)",
            names.join(", ")
        );
    }
    if let Some(dir) = config_path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(config_path, set_theme_text(&text, &name))
        .with_context(|| format!("writing {}", config_path.display()))?;
    println!("theme set to {name}");
    // Regenerate the init script and the cache the prompt reads.
    crate::setup::apply::run(config_path, shell, false, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_replaces_and_keeps_other_sections() {
        let added = set_theme_text("[shell]\nhistory_size = 5\n", "nord");
        assert!(added.ends_with("[theme]\nname = \"nord\"\n"));
        let replaced = set_theme_text(&added, "gruvbox");
        assert_eq!(replaced.matches("[theme]").count(), 1);
        assert!(replaced.contains("name = \"gruvbox\"") && !replaced.contains("nord"));
        let mid = set_theme_text("[theme]\n[env]\nA = \"1\"\n", "dracula");
        assert_eq!(mid, "[theme]\nname = \"dracula\"\n[env]\nA = \"1\"\n");
        assert!(Config::from_toml(&replaced).is_ok());
    }
}
