//! `shellcraft install`: install tools through the system package manager (never `curl | sh`).

use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};
use shellcraft_core::Config;
use shellcraft_platform::{self as platform, Manager, Os};

pub mod download;
pub mod selfupdate;

/// name, binary, brew, apt, pacman (`""` = not packaged there)
const TOOLS: &[(&str, &str, &str, &str, &str)] = &[
    ("starship", "starship", "starship", "", "starship"),
    ("zoxide", "zoxide", "zoxide", "zoxide", "zoxide"),
    ("fzf", "fzf", "fzf", "fzf", "fzf"),
    ("mise", "mise", "mise", "", "mise"),
    ("atuin", "atuin", "atuin", "", "atuin"),
    ("eza", "eza", "eza", "eza", "eza"),
    ("bat", "bat", "bat", "bat", "bat"),
    ("fd", "fd", "fd", "fd-find", "fd"),
    ("ripgrep", "rg", "ripgrep", "ripgrep", "ripgrep"),
];

fn in_path(bin: &str, path: &str) -> bool {
    path.split(':')
        .any(|d| !d.is_empty() && Path::new(d).join(bin).is_file())
}

/// First package manager of `os` whose program is on `path`.
fn detect(path: &str, os: Os) -> Option<Manager> {
    platform::package_managers_for(os)
        .iter()
        .copied()
        .find(|m| in_path(m.binary(), path))
}

/// Command line that installs `tool` with `m`, or `None` if `m` does not package it.
fn command(m: Manager, tool: &(&str, &str, &str, &str, &str)) -> Option<Vec<String>> {
    let pkg = match m {
        Manager::Brew => tool.2,
        Manager::Apt => tool.3,
        Manager::Pacman => tool.4,
    };
    (!pkg.is_empty()).then(|| {
        m.install_command()
            .iter()
            .map(|s| s.to_string())
            .chain([pkg.to_string()])
            .collect()
    })
}

/// Arguments of `shellcraft install`.
#[derive(clap::Args)]
pub struct InstallArgs {
    pub tools: Vec<String>,
    /// Print the commands without running them
    #[arg(long)]
    pub dry_run: bool,
    /// Download a checksummed release binary into ~/.local/bin instead of using a package manager
    #[arg(long)]
    pub native: bool,
}

pub fn run(args: InstallArgs, config_path: &Path) -> Result<()> {
    let InstallArgs {
        tools: names,
        dry_run,
        native,
    } = args;
    let path = std::env::var("PATH").unwrap_or_default();
    let wanted: Vec<String> = if names.is_empty() {
        // Tools enabled in the config.
        let text = std::fs::read_to_string(config_path).unwrap_or_default();
        let cfg = Config::from_toml(&text).context("reading config")?;
        cfg.tools
            .into_iter()
            .filter(|(_, on)| *on)
            .map(|(n, _)| n)
            .collect()
    } else {
        names
    };
    let m = detect(&path, platform::current());
    if m.is_none() && !native {
        bail!("no supported package manager found (brew, apt-get or pacman); try --native");
    }
    let mut failed = 0;
    for name in &wanted {
        let Some(tool) = TOOLS.iter().find(|t| t.0 == name) else {
            eprintln!(
                "{name}: not a plugin (`shellcraft search`) or a known tool (tools: {})",
                TOOLS.iter().map(|t| t.0).collect::<Vec<_>>().join(", ")
            );
            failed += 1;
            continue;
        };
        if in_path(tool.1, &path) {
            println!("{name}: already installed");
            continue;
        }
        // Package manager first; the native download covers `--native` and tools it lacks.
        let argv = m.filter(|_| !native).and_then(|m| command(m, tool));
        let Some(argv) = argv else {
            match native_install(name, dry_run) {
                Ok(()) => {}
                Err(e) => {
                    eprintln!("{name}: {e:#}");
                    failed += 1;
                }
            }
            continue;
        };
        println!("{name}: {}", argv.join(" "));
        if !dry_run {
            let ok = Command::new(&argv[0])
                .args(&argv[1..])
                .status()
                .is_ok_and(|s| s.success());
            if !ok {
                eprintln!("{name}: install failed");
                failed += 1;
            }
        }
    }
    if failed > 0 {
        bail!("{failed} tool(s) not installed");
    }
    Ok(())
}

/// Download the tool's checksummed release into `~/.local/bin`.
fn native_install(name: &str, dry_run: bool) -> Result<()> {
    let Some((_, bin, base, pattern)) = download::SOURCES.iter().find(|s| s.0 == name) else {
        bail!("no package for this manager and no native download; install it from its website");
    };
    let triple =
        shellcraft_platform::release_triple().context("native download: unsupported OS or CPU")?;
    let asset = pattern.replace("{triple}", triple);
    if dry_run {
        println!("{name}: download {base}/{asset} (+ .sha256) into ~/.local/bin");
        return Ok(());
    }
    let home = std::env::var("HOME").context("HOME is not set")?;
    let dest = Path::new(&home).join(".local").join("bin");
    let target = download::install(base, &asset, bin, &dest)?;
    println!("{name}: installed {} (sha256 verified)", target.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_commands_per_manager() {
        let fd = TOOLS.iter().find(|t| t.0 == "fd").expect("fd");
        assert_eq!(
            command(Manager::Brew, fd).expect("brew").join(" "),
            "brew install fd"
        );
        assert_eq!(
            command(Manager::Apt, fd)
                .expect("apt")
                .last()
                .map(String::as_str),
            Some("fd-find")
        );
        let starship = TOOLS.iter().find(|t| t.0 == "starship").expect("starship");
        assert!(command(Manager::Apt, starship).is_none());
    }

    #[test]
    fn detects_manager_from_path() {
        let dir = std::env::temp_dir().join(format!("sc-install-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let p = dir.to_str().expect("utf8");
        assert_eq!(detect(p, Os::Linux), None);
        std::fs::write(dir.join("pacman"), "").expect("write");
        assert_eq!(detect(p, Os::Linux), Some(Manager::Pacman));
        assert_eq!(detect(p, Os::MacOs), None, "pacman is not tried on macOS");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
