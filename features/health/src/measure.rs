//! `stats`, `bench` and `profile`: cold-path measurement commands.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;

use anyhow::{bail, Context, Result};
use shellcraft_core::{paths, Config, Shell};

use shellcraft_shell::{env, pick_shell, rc_path};

/// Sorted per-run timings in milliseconds.
pub struct Timings(Vec<f64>);

impl Timings {
    fn from(mut v: Vec<f64>) -> Self {
        v.sort_by(f64::total_cmp);
        Self(v)
    }
    pub fn min(&self) -> f64 {
        self.0.first().copied().unwrap_or(0.0)
    }
    pub fn max(&self) -> f64 {
        self.0.last().copied().unwrap_or(0.0)
    }
    pub fn median(&self) -> f64 {
        let n = self.0.len();
        match n {
            0 => 0.0,
            _ if n % 2 == 1 => self.0[n / 2],
            _ => (self.0[n / 2 - 1] + self.0[n / 2]) / 2.0,
        }
    }
}

fn run_once(cmd: &mut Command) -> Result<f64> {
    let start = Instant::now();
    let status = cmd.stdout(Stdio::null()).stderr(Stdio::null()).status()?;
    let ms = start.elapsed().as_secs_f64() * 1000.0;
    if !status.success() {
        bail!("shell exited with {status}");
    }
    Ok(ms)
}

fn time_runs(make: impl Fn() -> Command, runs: u32) -> Result<Timings> {
    run_once(&mut make())?; // warm-up, not counted
    let mut v = Vec::new();
    for _ in 0..runs.max(1) {
        v.push(run_once(&mut make())?);
    }
    Ok(Timings::from(v))
}

fn load(config_path: &Path) -> Result<Config> {
    let text = fs::read_to_string(config_path)
        .with_context(|| format!("reading {}", config_path.display()))?;
    Ok(Config::from_toml(&text)?)
}

/// Interactive shell with the user's real rc files.
fn wired(shell: Shell) -> Command {
    let mut c = Command::new(shell.name());
    match shell {
        Shell::Nushell => c.args(["--commands", "exit"]),
        Shell::Pwsh => c.args(["-Command", "exit"]),
        _ => c.args(["-i", "-c", "exit"]),
    };
    c
}

/// Interactive shell without the user's rc files. Global files (`/etc/zshrc`) still run, so the
/// difference to [`wired`] is only the user's own setup.
fn baseline(shell: Shell) -> Command {
    let mut c = Command::new(shell.name());
    match shell {
        Shell::Zsh => {
            let empty = std::env::temp_dir().join("shellcraft-empty-zdotdir");
            let _ = fs::create_dir_all(&empty);
            // An empty .zshrc stops zsh from starting its new-user setup wizard.
            let _ = fs::write(empty.join(".zshrc"), "");
            c.args(["-i", "-c", "exit"]).env("ZDOTDIR", empty)
        }
        Shell::Fish => {
            // `--no-config` would also skip the system conf.d files (direnv, ...) that the real
            // run loads; an empty config dir skips only the user's own config.
            let empty = std::env::temp_dir().join("shellcraft-empty-fish-config");
            let _ = fs::create_dir_all(&empty);
            c.args(["-i", "-c", "exit"]).env("XDG_CONFIG_HOME", empty)
        }
        Shell::Nushell => c.args(["--no-config-file", "--commands", "exit"]),
        Shell::Pwsh => c.args(["-NoProfile", "-Command", "exit"]),
        _ => c.args(["--norc", "--noprofile", "-i", "-c", "exit"]),
    };
    c
}

fn supported(shell: Shell) -> Result<Shell> {
    Ok(shell)
}

/// Arguments of `shellcraft bench`.
#[derive(clap::Args)]
pub struct BenchArgs {
    #[arg(default_value_t = 20)]
    pub runs: u32,
    #[arg(long)]
    pub shell: Option<String>,
}

/// Arguments of `shellcraft stats`.
#[derive(clap::Args)]
pub struct StatsArgs {
    #[arg(long)]
    pub shell: Option<String>,
}

pub fn bench(config_path: &Path, args: BenchArgs) -> Result<()> {
    let (runs, shell) = (args.runs, args.shell.as_deref());
    let cfg = load(config_path)?;
    let shell = supported(pick_shell(shell, &cfg)?)?;
    let w = time_runs(|| wired(shell), runs)?;
    let b = time_runs(|| baseline(shell), runs)?;
    println!(
        "{} interactive startup, {} runs (after 1 warm-up), ms",
        shell.name(),
        runs.max(1)
    );
    println!(
        "  with your rc   min {:.1}  median {:.1}  max {:.1}",
        w.min(),
        w.median(),
        w.max()
    );
    println!(
        "  no rc          min {:.1}  median {:.1}  max {:.1}",
        b.min(),
        b.median(),
        b.max()
    );
    println!(
        "  overhead       median {:.1} (budget 10)",
        w.median() - b.median()
    );
    Ok(())
}

fn dir_size(p: &Path) -> u64 {
    let Ok(meta) = fs::symlink_metadata(p) else {
        return 0;
    };
    if meta.is_dir() {
        fs::read_dir(p)
            .map(|rd| rd.filter_map(Result::ok).map(|e| dir_size(&e.path())).sum())
            .unwrap_or(0)
    } else {
        meta.len()
    }
}

fn kb(bytes: u64) -> String {
    format!("{:.1} KB", bytes as f64 / 1024.0)
}

fn version_of(bin: &str) -> Option<String> {
    let out = Command::new(bin).arg("--version").output().ok()?;
    let text = String::from_utf8_lossy(&out.stdout);
    text.lines()
        .next()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
}

pub fn stats(config_path: &Path, args: StatsArgs) -> Result<()> {
    let shell = args.shell.as_deref();
    let cfg = load(config_path)?;
    let shell = supported(pick_shell(shell, &cfg)?)?;
    println!("shellcraft {} stats", env!("CARGO_PKG_VERSION"));
    println!(
        "  host   {} {}  {}",
        std::env::consts::OS,
        std::env::consts::ARCH,
        shell.name()
    );

    println!("\nSpeed (10 runs, ms)");
    let w = time_runs(|| wired(shell), 10)?;
    let b = time_runs(|| baseline(shell), 10)?;
    println!(
        "  startup   median {:.1}  (min {:.1}, max {:.1})",
        w.median(),
        w.min(),
        w.max()
    );
    println!("  baseline  median {:.1}", b.median());
    println!("  overhead  {:.1}  (budget 10)", w.median() - b.median());

    println!("\nDisk");
    println!("  config        {}", kb(dir_size(config_path)));
    if let Some(cache) = paths::cache_dir(env) {
        println!(
            "  cache         {}  {}",
            kb(dir_size(&cache)),
            cache.display()
        );
    }
    if let Ok(exe) = std::env::current_exe() {
        println!("  binary        {}", kb(dir_size(&exe)));
    }

    println!("\nHistory");
    let hist = match shell {
        Shell::Zsh => env("HISTFILE")
            .map(PathBuf::from)
            .or_else(|| env("HOME").map(|h| Path::new(&h).join(".zsh_history"))),
        Shell::Fish => env("HOME").map(|h| Path::new(&h).join(".local/share/fish/fish_history")),
        Shell::Bash => env("HOME").map(|h| Path::new(&h).join(".bash_history")),
        Shell::Nushell | Shell::Pwsh => None,
    };
    match hist.and_then(|p| fs::read(&p).ok().map(|b| (p, b))) {
        Some((p, bytes)) => {
            let lines = bytes.iter().filter(|c| **c == b'\n').count();
            println!(
                "  {lines} lines, {}  {}",
                kb(bytes.len() as u64),
                p.display()
            );
        }
        None => println!("  no history file found"),
    }

    println!("\nTools");
    for (name, on) in &cfg.tools {
        let state = if *on { "enabled" } else { "disabled" };
        match version_of(name) {
            Some(v) => println!("  {name:<10} {state:<9} {v}"),
            None => println!("  {name:<10} {state:<9} not installed"),
        }
    }
    println!("\nNot measured: memory use.");
    Ok(())
}

pub fn profile() -> Result<()> {
    let rc = rc_path(Shell::Zsh)?;
    let tmp: PathBuf =
        std::env::temp_dir().join(format!("shellcraft-profile-{}", std::process::id()));
    fs::create_dir_all(&tmp)?;
    let script = format!(
        "zmodload zsh/zprof\n[[ -r '{}' ]] && source '{}'\nzprof | head -25\n",
        rc.display(),
        rc.display()
    );
    fs::write(tmp.join(".zshrc"), script)?;
    let out = Command::new("zsh")
        .args(["-i", "-c", "exit"])
        .env("ZDOTDIR", &tmp)
        .output()
        .context("running zsh")?;
    let _ = fs::remove_dir_all(&tmp);
    print!("{}", String::from_utf8_lossy(&out.stdout));
    Ok(())
}
