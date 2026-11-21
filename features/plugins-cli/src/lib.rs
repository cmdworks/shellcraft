//! `shellcraft plugin`: list, install, update, remove, scaffold and check plugins.
//!
//! `manage` installs and updates, `scaffold` creates skeletons, `check` validates, `runtime` runs
//! plugin commands and hooks. This file holds the clap definitions, `list` and the cache refresh.

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Subcommand;

mod binaries;
mod check;
mod manage;
mod registry;
mod runtime;
mod scaffold;
mod sign;

pub use manage::{confirm, fetch_source, git, install, Spec};
pub use registry::{run_registry, RegistryCmd};
pub use runtime::{run_external, run_hooks, HookArgs};
use shellcraft_core::{cache, paths, Config};
use shellcraft_plugin::store::{self, Lock};

#[derive(Subcommand)]
pub enum PluginCmd {
    /// List bundled, configured and installed plugins
    List,
    /// Install a plugin from a directory, `owner/repo[@rev]` (GitHub) or a git URL
    Add {
        source: String,
        /// Do not ask for confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Use a plugin from its folder while you develop it (edits apply at once, not hash-checked)
    Link { dir: PathBuf },
    /// Search the configured registries by name or description
    Search { query: Option<String> },
    /// Show a registry entry (repo, revision, pinned hash)
    Info { name: String },
    /// List the slots of a shell's init script, where plugins can attach code (`[[slot]]`)
    Slots { shell: String },
    /// Remove an installed plugin
    Remove { name: String },
    /// Update installed plugins from their source (all, or the named one)
    Update {
        name: Option<String>,
        /// Do not ask for confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Create a plugin skeleton in ./<name>
    New {
        name: String,
        /// segment, snippet, command, hook or data
        #[arg(long, default_value = "segment")]
        kind: String,
    },
    /// Validate a plugin directory and run its segments once
    Check { path: PathBuf },
    /// Refresh a plugin cache (started in the background by the prompt)
    #[command(hide = true)]
    Refresh {
        /// `versions` or `segment`
        kind: String,
        name: String,
        dir: PathBuf,
    },
}

pub(crate) fn load_config(config_path: &Path) -> Config {
    let env = |k: &str| std::env::var(k).ok();
    paths::cache_dir(env)
        .and_then(|d| cache::load(&d.join("config.bin"), config_path))
        .or_else(|| {
            std::fs::read_to_string(config_path)
                .ok()
                .and_then(|t| Config::from_toml(&t).ok())
        })
        .unwrap_or_default()
}

/// Is `name` (a plugin, or `plugin.id`) one of the installed plugins' baked segments?
fn cache_dir_baked(name: &str) -> bool {
    paths::cache_dir(|k| std::env::var(k).ok()).is_some_and(|d| {
        shellcraft_plugin::store::load_baked(&d)
            .iter()
            .any(|r| r.plugin == name || format!("{}.{}", r.plugin, r.id) == name)
    })
}

/// What the plugin commands need from the binary that hosts them.
pub struct Host<'a> {
    pub config_path: &'a Path,
    /// Names of the built-in commands: plugin commands may not take them, and they feed the
    /// did-you-mean hint.
    pub builtin_commands: &'a [String],
    /// Regenerate the init script quietly after a plugin was added, updated or removed.
    pub reapply: &'a dyn Fn(&Path) -> Result<()>,
}

pub fn run(cmd: PluginCmd, host: &Host) -> Result<()> {
    let config_path = host.config_path;
    let cfg = load_config(config_path);
    match cmd {
        PluginCmd::List => {
            let on = |name: &str| cfg.plugin.get(name).map_or(true, |p| p.enabled);
            let shown = |name: &str| cfg.prompt.segments.iter().any(|s| s == name);
            println!(
                "{:<12} {:<9} {:<8} {:<7} description",
                "name", "kind", "enabled", "prompt"
            );
            for p in shellcraft_bundled::bundled() {
                println!(
                    "{:<12} {:<9} {:<8} {:<7} {}",
                    p.name(),
                    "bundled",
                    if on(p.name()) { "yes" } else { "no" },
                    if shown(p.name()) { "yes" } else { "-" },
                    p.describe()
                );
            }
            for (name, s) in &cfg.plugin {
                if let Some(exe) = &s.segment {
                    println!(
                        "{:<12} {:<9} {:<8} {:<7} segment: {exe} (ttl {} ms)",
                        name,
                        "external",
                        if s.enabled { "yes" } else { "no" },
                        if shown(name) { "yes" } else { "-" },
                        s.ttl_ms
                    );
                }
            }
            if let Ok((data, lock_file)) = manage::dirs(config_path) {
                let lock = Lock::load(&lock_file);
                let (found, problems) = store::installed(&data, &lock);
                for p in found {
                    let m = &p.manifest;
                    println!(
                        "{:<12} {:<9} {:<8} {:<7} {} {}{}",
                        m.name,
                        "installed",
                        if cfg.plugin.get(&m.name).map_or(true, |s| s.enabled) {
                            "yes"
                        } else {
                            "no"
                        },
                        if m.segment.is_empty() { "-" } else { "auto" },
                        m.version,
                        if m.description.is_empty() { "" } else { "- " },
                        match &p.trust {
                            Ok(()) if lock.get(&m.name).is_some_and(|e| e.dev) => {
                                format!("LINKED (dev) {}", m.description)
                            }
                            Ok(()) => m.description.clone(),
                            Err(e) => format!("NOT LOADED: {e}"),
                        }
                    );
                    for name in m.command.keys() {
                        println!("{:<12} {:<9} command: shellcraft {name}", "", "");
                    }
                    for h in m.hook.values() {
                        println!("{:<12} {:<9} hook: {}", "", "", h.event);
                    }
                }
                for p in problems {
                    eprintln!("warning: plugin {p}");
                }
            }
            for seg in &cfg.prompt.segments {
                let baked_known = cache_dir_baked(seg);
                let known = shellcraft_bundled::find(seg).is_some()
                    || cfg.plugin.get(seg).is_some_and(|p| p.segment.is_some())
                    || baked_known;
                if !known {
                    eprintln!("warning: prompt segment `{seg}` matches no plugin");
                }
            }
            Ok(())
        }
        PluginCmd::Add { source, yes } => manage::add(&source, yes, host),
        PluginCmd::Link { dir } => manage::link(&dir, host),
        PluginCmd::Search { query } => registry::search(query.as_deref().unwrap_or(""), host),
        PluginCmd::Info { name } => registry::info(&name, host),
        PluginCmd::Slots { shell } => {
            let kind = shellcraft_core::Shell::parse(&shell)
                .with_context(|| format!("unknown shell `{shell}`"))?;
            for slot in shellcraft_shell::emitter(kind).slots() {
                println!("{slot}");
            }
            Ok(())
        }
        PluginCmd::Remove { name } => manage::remove(&name, host),
        PluginCmd::Update { name, yes } => manage::update(name.as_deref(), yes, host),
        PluginCmd::New { name, kind } => scaffold::scaffold(&name, &kind),
        PluginCmd::Check { path } => check::check(&path),
        PluginCmd::Refresh { kind, name, dir } => {
            let cache_dir = paths::cache_dir(|k| std::env::var(k).ok())
                .context("cannot locate cache dir: set HOME")?;
            match kind.as_str() {
                "versions" => {
                    shellcraft_bundled::refresh_versions(&name, &dir, &cache_dir, None);
                    Ok(())
                }
                "segment" => {
                    // `[plugin.<name>] segment = ...`, else an installed plugin's `plugin.id`
                    let exe = cfg
                        .plugin
                        .get(&name)
                        .and_then(|p| p.segment.clone())
                        .or_else(|| {
                            shellcraft_plugin::store::load_baked(&cache_dir)
                                .into_iter()
                                .find(|r| format!("{}.{}", r.plugin, r.id) == name)
                                .map(|r| r.command)
                        })
                        .with_context(|| format!("plugin `{name}` has no segment executable"))?;
                    let exe = match exe.strip_prefix("~/") {
                        Some(rest) => std::env::var("HOME")
                            .map(|h| format!("{h}/{rest}"))
                            .unwrap_or(exe),
                        None => exe,
                    };
                    shellcraft_plugin::external::refresh(&name, &exe, &dir, &cache_dir)
                        .with_context(|| format!("running {exe}"))
                }
                other => bail!("unknown refresh kind `{other}`"),
            }
        }
    }
}

/// One non-bundled plugin as a manager screen lists it.
pub struct CatalogEntry {
    pub name: String,
    pub version: String,
    pub description: String,
    pub installed: bool,
    pub note: String,
}

/// Installed plugins plus whatever the configured registries offer (offline: cached registries only).
pub fn catalog(host: &Host) -> Vec<CatalogEntry> {
    let cfg = load_config(host.config_path);
    let mut out: Vec<CatalogEntry> = Vec::new();
    if let Ok((data, lock_file)) = manage::dirs(host.config_path) {
        let (ok, problems) = store::installed(&data, &Lock::load(&lock_file));
        for p in ok {
            let m = &p.manifest;
            out.push(CatalogEntry {
                name: m.name.clone(),
                version: m.version.clone(),
                description: m.description.clone(),
                installed: true,
                note: p.trust.err().unwrap_or_default(),
            });
        }
        for pr in problems {
            out.push(CatalogEntry {
                name: pr.clone(),
                version: String::new(),
                description: String::new(),
                installed: true,
                note: pr,
            });
        }
    }
    for f in registry::all_entries(&cfg, false).unwrap_or_default() {
        if !out.iter().any(|e| e.name == f.entry.name) {
            out.push(CatalogEntry {
                name: f.entry.name,
                version: f.entry.version,
                description: f.entry.description,
                installed: false,
                note: format!("from registry {}", f.registry),
            });
        }
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

/// Install `name` (registry name, `owner/repo`, folder or URL) without asking, as the manager does
/// after the user reviewed the plan.
pub fn add_unattended(name: &str, host: &Host) -> Result<()> {
    let spec = registry::resolve(name, host)?;
    manage::install(&spec, true, host)
}

/// Remove an installed plugin.
pub fn remove_installed(name: &str, host: &Host) -> Result<()> {
    manage::remove(name, host)
}

/// `shellcraft install <names>`: install every name that is a plugin (official, from a configured
/// registry, a folder, `owner/repo` or a URL) without asking, and return the names that are not,
/// so the caller can try them as tools.
pub fn install_names(names: &[String], host: &Host) -> Result<Vec<String>> {
    let cfg = load_config(host.config_path);
    let known: Vec<String> = registry::all_entries(&cfg, false)?
        .into_iter()
        .map(|f| f.entry.name)
        .collect();
    let (_, lock_file) = manage::dirs(host.config_path)?;
    // re-apply once at the end, not after every plugin
    let quiet = |_: &Path| -> Result<()> { Ok(()) };
    let each = Host {
        config_path: host.config_path,
        builtin_commands: host.builtin_commands,
        reapply: &quiet,
    };
    manage::BRIEF.store(true, std::sync::atomic::Ordering::Relaxed);
    let mut rest = Vec::new();
    let mut installed = false;
    for name in names {
        let is_word = registry::looks_like_name(name);
        if is_word && !known.contains(name) {
            rest.push(name.clone());
        } else if is_word && Lock::load(&lock_file).get(name).is_some() {
            println!("{name}: already installed (`shellcraft update` brings it up to date)");
        } else {
            manage::add(name, true, &each)?;
            installed = true;
        }
    }
    manage::BRIEF.store(false, std::sync::atomic::Ordering::Relaxed);
    if installed && host.config_path.exists() {
        match (host.reapply)(host.config_path) {
            Ok(()) => println!("done: restart your shell or run `exec $SHELL` to use it"),
            Err(e) => eprintln!("warning: could not re-apply: {e:#}"),
        }
    }
    Ok(rest)
}

/// `shellcraft update`: refresh the registries, then bring every installed plugin up to date
/// without asking.
pub fn update_all(host: &Host) -> Result<()> {
    let cfg = load_config(host.config_path);
    let available = registry::all_entries(&cfg, true)?.len();
    println!("{available} plugins available");
    manage::update(None, true, host)
}
