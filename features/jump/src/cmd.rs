//! `shellcraft jump`: frecency directory database.

use shellcraft_core::proc::now_secs as now;
use std::path::PathBuf;

use crate::db::{load, save};
use anyhow::{bail, Context, Result};
use clap::Subcommand;
use shellcraft_core::paths;

#[derive(Subcommand)]
pub enum JumpCmd {
    /// Record a visit to a directory (called by the shell on every cd)
    Add { path: PathBuf },
    /// Print the best match for the given words (exit 1 if none)
    Query {
        /// Words that must appear in the path, in order
        terms: Vec<String>,
    },
    /// List matches with scores, best first
    List { terms: Vec<String> },
    /// Forget a directory
    Remove { path: PathBuf },
    /// Drop entries whose directory no longer exists
    Prune,
}

fn db_path() -> Result<PathBuf> {
    let dir =
        paths::data_dir(|k| std::env::var(k).ok()).context("cannot locate data dir: set HOME")?;
    Ok(dir.join("jump.tsv"))
}

fn exists(p: &str) -> bool {
    std::path::Path::new(p).is_dir()
}

pub fn run(cmd: JumpCmd) -> Result<()> {
    let file = db_path()?;
    let mut db = load(&file).with_context(|| format!("reading {}", file.display()))?;
    match cmd {
        JumpCmd::Add { path } => {
            let abs = path.canonicalize().unwrap_or(path);
            let Some(s) = abs.to_str() else { return Ok(()) };
            if std::env::var("HOME").is_ok_and(|h| h == s) || s == "/" {
                return Ok(());
            }
            db.add(s, now());
            save(&file, &db).with_context(|| format!("writing {}", file.display()))
        }
        JumpCmd::Query { terms } => {
            let t = now();
            match db
                .query(&terms, t)
                .into_iter()
                .find(|(e, _)| exists(&e.path))
            {
                Some((e, _)) => {
                    println!("{}", e.path);
                    Ok(())
                }
                None => bail!("no match"),
            }
        }
        JumpCmd::List { terms } => {
            for (e, score) in db.query(&terms, now()) {
                println!("{score:8.1}  {}", e.path);
            }
            Ok(())
        }
        JumpCmd::Remove { path } => {
            let abs = path.canonicalize().unwrap_or(path);
            let s = abs.to_string_lossy();
            if !db.remove(&s) {
                bail!("{s} is not in the database");
            }
            save(&file, &db).with_context(|| format!("writing {}", file.display()))
        }
        JumpCmd::Prune => {
            let n = db.prune(exists);
            save(&file, &db).with_context(|| format!("writing {}", file.display()))?;
            println!("removed {n} stale entries");
            Ok(())
        }
    }
}
