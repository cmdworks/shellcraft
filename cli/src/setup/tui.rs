//! `shellcraft tui`: gather state, run the full-screen manager, carry out the plan.

use std::io::IsTerminal;
use std::path::Path;

use anyhow::{bail, Result};
use shellcraft_plugins_cli::Host;
use shellcraft_tui::{Outcome, PluginInfo, Snapshot};

use super::apply;

pub fn run(config_path: &Path, host: &Host) -> Result<()> {
    if !std::io::stdin().is_terminal() || !std::io::stdout().is_terminal() {
        bail!("the manager needs a terminal (use `shellcraft setup` or edit the config)");
    }
    let config_text = std::fs::read_to_string(config_path).unwrap_or_default();
    let mut plugins: Vec<PluginInfo> = shellcraft_bundled::bundled()
        .iter()
        .map(|p| PluginInfo {
            name: p.name().to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            description: p.describe().to_string(),
            bundled: true,
            installed: true,
            note: String::new(),
        })
        .collect();
    plugins.extend(
        shellcraft_plugins_cli::catalog(host)
            .into_iter()
            .map(|e| PluginInfo {
                name: e.name,
                version: e.version,
                description: e.description,
                bundled: false,
                installed: e.installed,
                note: e.note,
            }),
    );
    let snapshot = Snapshot {
        config_path: config_path.display().to_string(),
        config_text,
        shell: std::env::var("SHELL")
            .unwrap_or_default()
            .rsplit('/')
            .next()
            .unwrap_or("")
            .to_string(),
        themes: shellcraft_shell::available_themes(config_path),
        plugins,
    };
    let Outcome::Apply(plan) = shellcraft_tui::run(snapshot)? else {
        println!("No changes made.");
        return Ok(());
    };
    // validated before anything is changed, so a bad edit cannot leave a broken config
    shellcraft_core::Config::from_toml(&plan.config_text)?;
    for name in &plan.remove {
        shellcraft_plugins_cli::remove_installed(name, host)?;
    }
    for name in &plan.install {
        shellcraft_plugins_cli::add_unattended(name, host)?;
    }
    if let Some(dir) = config_path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    std::fs::write(config_path, &plan.config_text)?;
    apply::run(config_path, None, false, false)
}
