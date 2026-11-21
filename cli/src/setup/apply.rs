//! `shellcraft apply`: generate the init script and optionally wire it into the rc file.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use shellcraft_core::{paths, Config, Shell};
use shellcraft_shell::{env, generate, pick_shell, quote_for, rc, rc_path};

static QUIET: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Silence the progress lines of `run` (used when another command re-applies).
pub fn set_quiet(on: bool) {
    QUIET.store(on, std::sync::atomic::Ordering::Relaxed);
}

fn quiet() -> bool {
    QUIET.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn run(config_path: &Path, shell: Option<&str>, print: bool, wire_rc: bool) -> Result<()> {
    let text = fs::read_to_string(config_path).with_context(|| {
        format!(
            "reading {} (run `shellcraft init` first)",
            config_path.display()
        )
    })?;
    let mut cfg =
        Config::from_toml(&text).with_context(|| format!("in {}", config_path.display()))?;
    let shell = pick_shell(shell, &cfg)?;
    let (script, plugins) = generate(&mut cfg, shell, config_path)?;
    if cfg.theme.name != "default" && !cfg.theme_data.contains_key(&cfg.theme.name) {
        eprintln!(
            "warning: theme `{0}` is not installed; using the default colours (`shellcraft install {0}`)",
            cfg.theme.name
        );
    }
    if print {
        print!("{script}");
        return Ok(());
    }

    let dir =
        paths::cache_dir(env).context("cannot locate cache dir: set XDG_CACHE_HOME or HOME")?;
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    let init = dir.join(format!("init.{}", shell.file_ext()));
    fs::write(&init, &script).with_context(|| format!("writing {}", init.display()))?;
    if !quiet() {
        println!("wrote {}", init.display());
    }
    shellcraft_plugin::store::bake(&dir, &plugins)
        .with_context(|| format!("writing {}", dir.join("plugins.tsv").display()))?;
    let cache_file = dir.join("config.bin");
    shellcraft_core::cache::store(&cache_file, config_path, &cfg)
        .with_context(|| format!("writing {}", cache_file.display()))?;

    let init_q = quote_for(shell, &init.display().to_string());
    let line = match shell {
        Shell::Fish => format!("test -r {init_q}; and source {init_q}"),
        Shell::Nushell => format!("source {init_q}"),
        Shell::Pwsh => format!("if (Test-Path {init_q}) {{ . {init_q} }}"),
        _ => format!("[ -r {init_q} ] && . {init_q}"),
    };
    if !wire_rc {
        if !quiet() {
            println!("add this to your rc file, or run `shellcraft apply --rc`:\n{line}");
        }
        return Ok(());
    }

    let rc_file = rc_path(shell)?;
    let existing = match fs::read_to_string(&rc_file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("reading {}", rc_file.display())),
    };
    let updated = rc::upsert(&existing, &line);
    if updated == existing {
        println!("{} already wired", rc_file.display());
        return Ok(());
    }
    if !existing.is_empty() {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let backup = PathBuf::from(format!("{}.pre-shellcraft-{stamp}", rc_file.display()));
        fs::copy(&rc_file, &backup)
            .with_context(|| format!("backing up to {}", backup.display()))?;
        println!("backup {}", backup.display());
    }
    if let Some(parent) = rc_file.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    fs::write(&rc_file, updated).with_context(|| format!("writing {}", rc_file.display()))?;
    println!("wired {}", rc_file.display());
    Ok(())
}
