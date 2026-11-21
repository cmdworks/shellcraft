//! Put the whole init script together: the config, the bundled plugins, the installed plugins.
//!
//! `apply` and `doctor` both call [`generate`], so "what apply would write" is one function.

use std::fs;
use std::path::Path;

use anyhow::Result;
use shellcraft_core::{Config, Shell};
use shellcraft_plugin::store::Installed;

use crate::emitter;

/// Attach installed plugins: their data fills gaps in the config (your entries win), their
/// snippets for `shell` go into the init script. Untrusted plugins are skipped with a warning.
pub fn load_plugins(
    cfg: &mut Config,
    shell: Shell,
    config_path: &Path,
) -> Vec<shellcraft_plugin::store::Installed> {
    let Some(data) = shellcraft_core::paths::data_dir(crate::select::env) else {
        return Vec::new();
    };
    let lock =
        shellcraft_plugin::store::Lock::load(&shellcraft_plugin::store::lock_path(config_path));
    let (found, problems) = shellcraft_plugin::store::installed(&data, &lock);
    for p in problems {
        eprintln!("warning: plugin {p}; skipped");
    }
    attach_bundled(cfg, shell);
    for p in &found {
        let m = &p.manifest;
        if let Err(why) = &p.trust {
            eprintln!("warning: plugin {}: {why}; skipped", m.name);
            continue;
        }
        for h in m.hook.values() {
            if !cfg.hook_events.contains(&h.event) {
                cfg.hook_events.push(h.event.clone());
            }
        }
        for (k, v) in &m.theme {
            cfg.theme_data.entry(k.clone()).or_insert_with(|| v.clone());
        }
        for (k, v) in &m.aliases {
            cfg.aliases.entry(k.clone()).or_insert_with(|| v.clone());
        }
        for (k, v) in &m.abbr {
            cfg.abbr.entry(k.clone()).or_insert_with(|| v.clone());
        }
        for (k, v) in &m.env {
            cfg.env.entry(k.clone()).or_insert_with(|| v.clone());
        }
        if let Some(file) = m
            .snippets
            .get(shell.name())
            .filter(|_| m.supports(shell.name()))
        {
            match fs::read_to_string(p.dir.join(file)) {
                Ok(code) => {
                    let mut snippets = std::collections::BTreeMap::new();
                    snippets.insert(shell.name().to_string(), code);
                    cfg.installed.push(shellcraft_core::InstalledPlugin {
                        name: m.name.clone(),
                        snippets,
                        slots: Vec::new(),
                    });
                }
                Err(e) => eprintln!("warning: plugin {}: cannot read {file}: {e}", m.name),
            }
        }
    }
    attach_slots(cfg, shell, &found);
    found
}

/// Theme names that can be chosen: the built-in ones and those of trusted installed plugins.
pub fn available_themes(config_path: &Path) -> Vec<String> {
    let mut names: Vec<String> = shellcraft_core::theme::NAMES
        .iter()
        .map(|s| (*s).to_string())
        .collect();
    if let Some(data) = shellcraft_core::paths::data_dir(crate::select::env) {
        let lock =
            shellcraft_plugin::store::Lock::load(&shellcraft_plugin::store::lock_path(config_path));
        for p in shellcraft_plugin::store::installed(&data, &lock)
            .0
            .iter()
            .filter(|p| p.trust.is_ok())
        {
            names.extend(p.manifest.theme.keys().cloned());
        }
    }
    names.sort_by_key(|n| (n != "default", n.clone()));
    names.dedup();
    names
}

/// Attach the `[[slot]]` code of trusted plugins for `shell`; slots the shell does not have are
/// reported and skipped.
fn attach_slots(cfg: &mut Config, shell: Shell, found: &[Installed]) {
    let known = emitter(shell).slots();
    for p in found.iter().filter(|p| p.trust.is_ok()) {
        let m = &p.manifest;
        if !m.supports(shell.name()) {
            continue;
        }
        let mut slots = Vec::new();
        for sl in m.slot.iter().filter(|s| s.shell == shell.name()) {
            if !known.contains(&sl.name.as_str()) {
                eprintln!(
                    "warning: plugin {}: {} has no slot `{}` (slots: {}); skipped",
                    m.name,
                    shell.name(),
                    sl.name,
                    known.join(", ")
                );
                continue;
            }
            match fs::read_to_string(p.dir.join(&sl.file)) {
                Ok(text) => slots.push(shellcraft_core::SlotSnippet {
                    slot: sl.name.clone(),
                    position: if sl.position == "before" {
                        shellcraft_core::SlotPosition::Before
                    } else {
                        shellcraft_core::SlotPosition::After
                    },
                    text,
                }),
                Err(e) => eprintln!("warning: plugin {}: cannot read {}: {e}", m.name, sl.file),
            }
        }
        if !slots.is_empty() {
            cfg.installed.push(shellcraft_core::InstalledPlugin {
                name: m.name.clone(),
                snippets: Default::default(),
                slots,
            });
        }
    }
}

/// Attach the shell code of the enabled bundled plugins (docker helpers) like an installed
/// plugin's. Pure: reads no files, so tests can use it.
pub fn attach_bundled(cfg: &mut Config, shell: Shell) {
    for p in shellcraft_bundled::bundled() {
        if cfg.plugin.get(p.name()).map_or(true, |s| s.enabled) {
            if let Some(code) = p.snippet(shell) {
                let mut snippets = std::collections::BTreeMap::new();
                snippets.insert(shell.name().to_string(), code.to_string());
                cfg.installed.push(shellcraft_core::InstalledPlugin {
                    name: p.name().to_string(),
                    snippets,
                    slots: Vec::new(),
                });
            }
        }
    }
}

/// Line that puts `dir` (where the running shellcraft lives) on PATH, for the init script.
pub fn path_line(shell: Shell, dir: &str) -> String {
    let q = crate::quote_for(shell, dir);
    match shell {
        Shell::Fish => format!("contains -- {q} $PATH; or set -gx PATH {q} $PATH\n"),
        Shell::Nushell => format!("$env.PATH = ($env.PATH | prepend {q})\n"),
        Shell::Pwsh => format!(
            "if (-not (($env:PATH -split [IO.Path]::PathSeparator) -contains {q})) {{ $env:PATH = {q} + [IO.Path]::PathSeparator + $env:PATH }}\n"
        ),
        _ => format!("case \":$PATH:\" in *\":{dir}:\"*) ;; *) export PATH={q}:\"$PATH\" ;; esac\n"),
    }
}

/// The PATH line for the running binary. Always included (it only adds the folder when it is
/// missing): leaving it out because the folder is on PATH *now* would drop it from the script the
/// next time `apply` runs inside a shell whose PATH this very line had set. Skipped for cargo
/// `target/` builds, which developers put on PATH themselves.
fn own_path_line(shell: Shell) -> String {
    let Ok(exe) = std::env::current_exe() else {
        return String::new();
    };
    let Some(dir) = exe.parent() else {
        return String::new();
    };
    let dev_build = dir.components().any(|c| c.as_os_str() == "target");
    if exe.file_name().and_then(|n| n.to_str()) != Some("shellcraft") || dev_build {
        return String::new();
    }
    dir.to_str()
        .map(|d| path_line(shell, d))
        .unwrap_or_default()
}

/// Attach plugins to `cfg` and render the init script for `shell`. Also returns the installed
/// plugins that were checked, for `apply` to bake their segment tables.
pub fn generate(
    cfg: &mut Config,
    shell: Shell,
    config_path: &Path,
) -> Result<(String, Vec<Installed>)> {
    let plugins = load_plugins(cfg, shell, config_path);
    let script = format!("{}{}", own_path_line(shell), emitter(shell).emit(cfg)?);
    Ok((script, plugins))
}
