//! `shellcraft hist`: command history with exit status, duration and directory.

use shellcraft_core::proc::now_secs as now;
use std::path::PathBuf;

use crate::db::{Db, Query, Record};
use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use shellcraft_core::paths;

#[derive(Args)]
pub struct Filter {
    /// Words that must all appear in the command
    pub terms: Vec<String>,
    /// Only commands run in the current directory
    #[arg(long)]
    pub here: bool,
    /// Only commands that succeeded
    #[arg(long)]
    pub ok: bool,
    /// Maximum results (0 = all)
    #[arg(short = 'n', long, default_value_t = 0)]
    pub limit: usize,
    /// Keep repeated commands instead of showing each once
    #[arg(long)]
    pub all: bool,
}

#[derive(Subcommand)]
pub enum HistCmd {
    /// Record a command (called by the shell after each command)
    Add {
        #[arg(long, default_value_t = 0)]
        exit: i32,
        /// Duration in milliseconds
        #[arg(long, default_value_t = 0)]
        duration: u64,
        #[arg(long)]
        cwd: Option<String>,
        /// Which shell session wrote this (its pid): other sessions use it to notice that a
        /// command came from somewhere else
        #[arg(long)]
        session: Option<String>,
        /// The command line
        #[arg(trailing_var_arg = true, required = true, allow_hyphen_values = true)]
        command: Vec<String>,
    },
    /// Print matching commands, newest first (one per line, for fzf)
    Search(Filter),
    /// Like search, with time, exit status and directory
    List(Filter),
    /// Print the best completion of a typed prefix (used for the dim inline suggestion)
    Suggest {
        /// Prefer commands run in this directory
        #[arg(long)]
        cwd: Option<String>,
        /// What has been typed so far
        #[arg(trailing_var_arg = true, required = true, allow_hyphen_values = true)]
        prefix: Vec<String>,
    },
    /// Recent successful commands as `directory<TAB>command`, newest first (the zsh line editor
    /// loads these once to suggest from memory)
    Recent {
        #[arg(short = 'n', long, default_value_t = 3000)]
        limit: usize,
    },
    /// Totals and the most used commands
    Stats,
}

fn file() -> Result<PathBuf> {
    let dir =
        paths::data_dir(|k| std::env::var(k).ok()).context("cannot locate data dir: set HOME")?;
    Ok(dir.join("history.db"))
}

fn run_query(f: &Filter, db: &Db) -> Result<Vec<Record>> {
    let cwd = f
        .here
        .then(|| {
            std::env::current_dir()
                .ok()
                .map(|p| p.to_string_lossy().into_owned())
        })
        .flatten();
    db.search(&Query {
        terms: &f.terms,
        cwd: cwd.as_deref(),
        ok_only: f.ok,
        unique: !f.all,
        limit: f.limit,
    })
}

pub fn run(cmd: HistCmd) -> Result<()> {
    let path = file()?;
    match cmd {
        HistCmd::Add {
            exit,
            duration,
            cwd,
            session,
            command,
        } => {
            let command = command.join(" ");
            // Like HISTCONTROL=ignorespace: a leading space keeps a command out of history.
            if command.trim().is_empty() || command.starts_with(' ') {
                return Ok(());
            }
            let cwd = cwd.unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default()
            });
            Db::open(&path)
                .and_then(|db| {
                    db.add(&Record {
                        when: now(),
                        exit,
                        duration_ms: duration,
                        cwd,
                        command,
                    })
                })
                .with_context(|| format!("writing {}", path.display()))?;
            // tell other sessions (they read this one small file, no process) that history changed
            let stamp = format!(
                "{} {}\n",
                session.unwrap_or_default(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos())
            );
            let _ = std::fs::write(path.with_file_name("history.stamp"), stamp);
            Ok(())
        }
        HistCmd::Search(f) => {
            let db = Db::open(&path)?;
            for r in run_query(&f, &db)? {
                println!("{}", r.command.replace('\n', " "));
            }
            Ok(())
        }
        HistCmd::List(f) => {
            let db = Db::open(&path)?;
            for r in run_query(&f, &db)? {
                println!(
                    "{}  {:>3}  {:>6}ms  {}  {}",
                    r.when,
                    r.exit,
                    r.duration_ms,
                    r.cwd,
                    r.command.replace('\n', " ")
                );
            }
            Ok(())
        }
        HistCmd::Suggest { cwd, prefix } => {
            let db = Db::open(&path)?;
            // Keep the typed text exactly (the shell passes it as one word).
            let typed = prefix.join(" ");
            match db.suggest(&typed, cwd.as_deref())? {
                Some(s) => {
                    println!("{s}");
                    Ok(())
                }
                None => std::process::exit(1),
            }
        }
        HistCmd::Recent { limit } => {
            for (cwd, cmd) in Db::open(&path)?.recent(limit)? {
                println!("{cwd}\t{cmd}");
            }
            Ok(())
        }
        HistCmd::Stats => {
            let (total, failed, top) = Db::open(&path)?.stats()?;
            println!("commands: {total}  failed: {failed}");
            for (cmd, n) in top {
                println!("{n:>6}  {}", cmd.replace('\n', " "));
            }
            Ok(())
        }
    }
}
