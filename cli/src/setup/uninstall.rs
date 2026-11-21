//! `shellcraft uninstall`: undo what `apply --rc` did. Never touches `shellcraft.toml`.

use std::fs;

use anyhow::{bail, Context, Result};
use shellcraft_core::{paths, Shell};
use shellcraft_shell::rc;

use shellcraft_shell::{env, rc_path};

pub fn run(purge: bool) -> Result<()> {
    for shell in [
        Shell::Zsh,
        Shell::Bash,
        Shell::Fish,
        Shell::Nushell,
        Shell::Pwsh,
    ] {
        let Ok(path) = rc_path(shell) else { continue };
        let Ok(text) = fs::read_to_string(&path) else {
            continue;
        };
        match rc::remove(&text) {
            Some(updated) => {
                fs::write(&path, updated).with_context(|| format!("writing {}", path.display()))?;
                println!("removed block from {}", path.display());
            }
            None => println!("no shellcraft block in {}", path.display()),
        }
    }

    let dir = paths::cache_dir(env).context("cannot locate cache dir")?;
    if purge {
        if dir.file_name().and_then(|n| n.to_str()) != Some("shellcraft") {
            bail!("refusing to delete unexpected cache path {}", dir.display());
        }
        match fs::remove_dir_all(&dir) {
            Ok(()) => println!("deleted {}", dir.display()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e).with_context(|| format!("deleting {}", dir.display())),
        }
    } else {
        let bin = dir.join("config.bin");
        if bin.exists() {
            fs::remove_file(&bin).with_context(|| format!("deleting {}", bin.display()))?;
            println!("deleted {}", bin.display());
        }
        for shell in [
            Shell::Zsh,
            Shell::Bash,
            Shell::Fish,
            Shell::Nushell,
            Shell::Pwsh,
        ] {
            let f = dir.join(format!("init.{}", shell.file_ext()));
            if f.exists() {
                fs::remove_file(&f).with_context(|| format!("deleting {}", f.display()))?;
                println!("deleted {}", f.display());
            }
        }
    }
    println!("config kept (delete it yourself if you want it gone): run `shellcraft path`");
    Ok(())
}
