//! `shellcraft init`: write the commented default config.

use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use shellcraft_core::DEFAULT_CONFIG;

pub fn run(path: &Path, force: bool) -> Result<()> {
    if path.exists() && !force {
        bail!(
            "{} already exists (use --force to overwrite)",
            path.display()
        );
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    fs::write(path, DEFAULT_CONFIG).with_context(|| format!("writing {}", path.display()))?;
    println!("wrote {}", path.display());
    Ok(())
}
