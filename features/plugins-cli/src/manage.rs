//! `plugin add`, `update`, `remove`: sources, git, confirmation, lock file.

use std::fs;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};
use shellcraft_core::paths;
use shellcraft_plugin::manifest::{Manifest, Requirement, FILE_NAME};
use shellcraft_plugin::store::{self, Lock, LockEntry};

use super::runtime::{cache_dir, command_conflicts};
use super::Host;

/// `shellcraft install <name>` prints one line per plugin instead of the full summary.
pub(super) static BRIEF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub enum Source {
    Local(PathBuf),
    Git {
        url: String,
        rev: Option<String>,
        /// A folder inside the repository that holds the plugin (`owner/repo/sub/dir`, `url#sub/dir`).
        subdir: Option<String>,
    },
}

/// A folder path inside a repository: relative, no `..`.
fn safe_subdir(s: &str) -> bool {
    !s.is_empty()
        && !s.starts_with('/')
        && s.split('/')
            .all(|p| !p.is_empty() && p != "." && p != ".." && !p.contains('\\'))
}

pub fn parse_source(s: &str) -> Result<Source> {
    if let Some(name) = s.strip_prefix("official:") {
        let dir = super::registry::official_dir()?.join(name);
        if !dir.is_dir() {
            bail!("`{name}` is not an official plugin");
        }
        return Ok(Source::Local(dir.canonicalize()?));
    }
    if Path::new(s).is_dir() {
        return Ok(Source::Local(Path::new(s).canonicalize()?));
    }
    let (main, hash_sub) = match s.split_once('#') {
        Some((a, b)) if !b.is_empty() => (a, Some(b.to_string())),
        _ => (s, None),
    };
    if hash_sub.as_deref().is_some_and(|d| !safe_subdir(d)) {
        bail!("`{s}`: the folder after `#` must be a relative path inside the repository");
    }
    let (repo, rev) = match main.rsplit_once('@') {
        Some((r, v)) if r.contains('/') && !r.contains(':') && !v.is_empty() => {
            (r, Some(v.to_string()))
        }
        _ => (main, None),
    };
    let word = |p: &str| {
        !p.is_empty()
            && p.chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
    };
    // owner/repo, or owner/repo/folder/in/the/repo
    let mut parts = repo.split('/');
    if let (Some(owner), Some(name)) = (parts.next(), parts.next()) {
        let rest: Vec<&str> = parts.collect();
        let sub = rest.join("/");
        if word(owner)
            && word(name)
            && (rest.is_empty() || (safe_subdir(&sub) && hash_sub.is_none()))
            && !repo.contains(':')
        {
            return Ok(Source::Git {
                url: format!("https://github.com/{owner}/{name}.git"),
                rev,
                subdir: (!rest.is_empty()).then_some(sub).or(hash_sub),
            });
        }
    }
    if ["https://", "ssh://", "file://", "git@"]
        .iter()
        .any(|p| main.starts_with(p))
    {
        return Ok(Source::Git {
            url: main.to_string(),
            rev: None,
            subdir: hash_sub,
        });
    }
    bail!("`{s}` is not a directory, `owner/repo[/folder][@rev]` or a git URL (https, ssh, file; `#folder` after it)")
}

pub fn git(dir: Option<&Path>, args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .args(["-c", "protocol.ext.allow=never"]);
    if let Some(d) = dir {
        cmd.arg("-C").arg(d);
    }
    let out = cmd
        .args(args)
        .stdin(Stdio::null())
        .output()
        .context("running git (is it installed?)")?;
    if !out.status.success() {
        bail!(
            "git {}: {}",
            args.first().copied().unwrap_or(""),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn copy_dir(from: &Path, to: &Path) -> Result<()> {
    fs::create_dir_all(to)?;
    for e in fs::read_dir(from)? {
        let e = e?;
        if e.file_name() == ".git" {
            continue;
        }
        let dest = to.join(e.file_name());
        if e.file_type()?.is_dir() {
            copy_dir(&e.path(), &dest)?;
        } else {
            fs::copy(e.path(), &dest)?; // keeps the executable bit
        }
    }
    Ok(())
}

pub fn confirm(question: &str) -> bool {
    print!("{question} [y/N] ");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line).is_ok()
        && matches!(line.trim().to_lowercase().as_str(), "y" | "yes")
}

pub(crate) fn read_manifest(dir: &Path) -> Result<Manifest> {
    let text = fs::read_to_string(dir.join(FILE_NAME))
        .with_context(|| format!("{} not found in {}", FILE_NAME, dir.display()))?;
    Manifest::parse(&text).map_err(|e| anyhow::anyhow!("{FILE_NAME}: {e}"))
}

fn summary(m: &Manifest, dir: &Path, shown_dir: &Path, source: &str, rev: &str) -> String {
    let mut s = format!("plugin {} {}", m.name, m.version);
    if !m.description.is_empty() {
        s += &format!(" - {}", m.description);
    }
    s += &format!("\n  source:   {source} @ {}", &rev[..rev.len().min(12)]);
    s += &format!(
        "\n  license:  {}",
        m.license.as_deref().unwrap_or("not stated")
    );
    s += &format!(
        "\n  shells:   {}",
        if m.shells.is_empty() {
            "all".to_string()
        } else {
            m.shells.join(", ")
        }
    );
    for (id, seg) in &m.segment {
        s += &format!(
            "\n  segment:  {id} runs {} (when: {})",
            seg.command,
            if seg.when.files.is_empty() {
                "always".into()
            } else {
                seg.when.files.join(", ")
            }
        );
    }
    for (name, c) in &m.command {
        s += &format!(
            "\n  command:  shellcraft {name} runs {} {}",
            c.run,
            if c.about.is_empty() {
                String::new()
            } else {
                format!("({})", c.about)
            }
        );
    }
    for line in super::binaries::summary_lines(m) {
        s += &format!("\n  {line}");
    }
    for (id, h) in &m.hook {
        s += &format!(
            "\n  hook:     {id} runs {} on every {} event",
            h.run, h.event
        );
    }
    for (sh, file) in &m.snippets {
        let size = fs::metadata(dir.join(file)).map_or(0, |m| m.len());
        s += &format!("\n  snippet:  {sh} -> {file} ({size} bytes, runs inside your shell)");
    }
    for (label, n) in [
        ("aliases", m.aliases.len()),
        ("abbreviations", m.abbr.len()),
        ("env vars", m.env.len()),
    ] {
        if n > 0 {
            s += &format!("\n  adds:     {n} {label}");
        }
    }
    for l in m.permission_lines() {
        s += &format!("\n  asks for: {l}");
    }
    s += &format!(
        "\n  Plugins run as you and are not sandboxed; the permissions are what the author says. Read the files in {} first.",
        shown_dir.display()
    );
    s
}

/// Re-apply (through the host) so the init script and the prompt's segment list see the change.
/// Is `r` met by a bundled plugin or an installed one of a high enough version?
fn requirement_met(r: &Requirement, data: &Path, lock: &Lock) -> bool {
    shellcraft_bundled::find(&r.name).is_some()
        || store::installed(data, lock)
            .0
            .iter()
            .any(|i| i.manifest.name == r.name && r.satisfied_by(&i.manifest.version))
}

/// Refuse a plugin whose `requires` are not met or whose `conflicts` are present. Bundled plugins
/// count as installed (any version).
fn check_requirements(m: &Manifest, data: &Path, lock: &Lock) -> Result<()> {
    let (found, _) = store::installed(data, lock);
    let bundled: Vec<&str> = shellcraft_bundled::bundled()
        .iter()
        .map(|p| p.name())
        .collect();
    let mut problems = Vec::new();
    for text in &m.requires {
        let r = Requirement::parse(text).map_err(|e| anyhow::anyhow!(e))?;
        if !requirement_met(&r, data, lock) {
            problems.push(format!("requires `{text}` (install it first)"));
        }
    }
    for c in &m.conflicts {
        if bundled.contains(&c.as_str()) || found.iter().any(|i| i.manifest.name == *c) {
            problems.push(format!("conflicts with `{c}`, which is installed"));
        }
    }
    if problems.is_empty() {
        Ok(())
    } else {
        bail!("cannot install {}: {}", m.name, problems.join("; "))
    }
}

fn reapply(host: &Host) {
    if host.config_path.exists() {
        match (host.reapply)(host.config_path) {
            Ok(()) => println!("re-applied: restart your shell or run `exec $SHELL` to load it"),
            Err(e) => eprintln!("warning: could not re-apply: {e:#}"),
        }
    }
}

pub(super) fn dirs(config_path: &Path) -> Result<(PathBuf, PathBuf)> {
    let data =
        paths::data_dir(|k| std::env::var(k).ok()).context("cannot locate data dir: set HOME")?;
    Ok((data, store::lock_path(config_path)))
}

/// What to install: where from, an exact revision, and the content hash it must have.
pub struct Spec {
    /// As recorded in the lock file: a folder, `owner/repo[@rev]` or a git URL.
    pub source: String,
    /// Commit, tag or branch to check out (overrides an `@rev` in `source`).
    pub rev: Option<String>,
    /// Required SHA-256 of the plugin files (see `store::hash_dir`).
    pub expect_hash: Option<String>,
}

/// Fetch `source` into `dest` (a git clone, or a copy of a folder). Returns the commit, or
/// `"local"` for a folder.
pub fn fetch_source(source: &str, rev: Option<&str>, dest: &Path) -> Result<String> {
    match parse_source(source)? {
        Source::Local(p) => {
            copy_dir(&p, dest)?;
            Ok(if source.starts_with("official:") {
                "official"
            } else {
                "local"
            }
            .to_string())
        }
        Source::Git {
            url,
            rev: from_source,
            subdir,
        } => {
            let rev = rev.map(str::to_string).or(from_source);
            let checkout = |dir: &Path| -> Result<String> {
                git(None, &["clone", "--quiet", &url, &dir.to_string_lossy()])?;
                if let Some(r) = &rev {
                    git(Some(dir), &["checkout", "--quiet", r])?;
                }
                git(Some(dir), &["rev-parse", "HEAD"])
            };
            match subdir {
                None => checkout(dest),
                Some(sub) => {
                    // clone beside the destination, keep only the plugin's folder
                    let name = dest
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
                    let clone = dest.with_file_name(format!("{name}.clone"));
                    let _ = fs::remove_dir_all(&clone);
                    let result = (|| -> Result<String> {
                        let head = checkout(&clone)?;
                        let from = clone.join(&sub);
                        if !from.is_dir() {
                            bail!("the repository has no folder `{sub}`");
                        }
                        copy_dir(&from, dest)?;
                        Ok(head)
                    })();
                    let _ = fs::remove_dir_all(&clone);
                    result
                }
            }
        }
    }
}

/// `plugin add <source>`: a folder, `owner/repo[@rev]`, a git URL, or a registry name.
pub(super) fn add(source: &str, yes: bool, host: &Host) -> Result<()> {
    let spec = super::registry::resolve(source, host)?;
    install(&spec, yes, host)
}

/// Install one plugin: show what it asks for, ask (unless `yes`), copy, lock, re-apply.
pub fn install(spec: &Spec, yes: bool, host: &Host) -> Result<()> {
    install_at(spec, yes, host, 0)
}

/// `install`, first installing missing requirements that a registry can provide (`depth` stops
/// requirement cycles).
fn install_at(spec: &Spec, yes: bool, host: &Host, depth: u32) -> Result<()> {
    let config_path = host.config_path;
    let (data, lock_file) = dirs(config_path)?;
    let root = store::plugins_dir(&data);
    fs::create_dir_all(&root)?;
    let tmp = root.join(format!(".tmp-{}-{depth}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    let result = (|| -> Result<()> {
        let rev = fetch_source(&spec.source, spec.rev.as_deref(), &tmp)?;
        if let Some(want) = &spec.expect_hash {
            let have = store::hash_dir(&tmp)?;
            if &have != want {
                bail!(
                    "{}: the files do not match the expected hash (expected {want}, got {have}); not installed",
                    spec.source
                );
            }
        }
        let m = read_manifest(&tmp)?;
        let conflicts = command_conflicts(&m, &cache_dir()?, host.builtin_commands);
        if !conflicts.is_empty() {
            bail!("cannot install {}: {}", m.name, conflicts.join("; "));
        }
        for text in &m.requires {
            let Ok(r) = Requirement::parse(text) else {
                continue;
            };
            if depth < 4 && !requirement_met(&r, &data, &Lock::load(&lock_file)) {
                if let Ok(dep) = super::registry::resolve(&r.name, host) {
                    println!("{}: needs {}, installing it first", m.name, r.name);
                    install_at(&dep, yes, host, depth + 1)?;
                }
            }
        }
        check_requirements(&m, &data, &Lock::load(&lock_file))?;
        let target = root.join(&m.name);
        if target.exists() {
            bail!(
                "`{}` is already installed (use `plugin update` or `plugin remove {}`)",
                m.name,
                m.name
            );
        }
        if BRIEF.load(std::sync::atomic::Ordering::Relaxed) {
            println!("installing {} {}: {}", m.name, m.version, m.description);
        } else {
            println!("{}", summary(&m, &tmp, &target, &spec.source, &rev));
            if !yes && !confirm("Install?") {
                bail!("not installed");
            }
        }
        fs::rename(&tmp, &target)?;
        let binaries = match super::binaries::fetch_for(&m, &data) {
            Ok(b) => b,
            Err(e) => {
                let _ = fs::remove_dir_all(&target);
                return Err(e);
            }
        };
        let mut lock = Lock::load(&lock_file);
        lock.set(LockEntry {
            name: m.name.clone(),
            version: m.version.clone(),
            source: spec.source.clone(),
            rev,
            hash: store::hash_dir(&target)?,
            approved: m.permission_lines(),
            dev: false,
            binaries,
        });
        lock.save(&lock_file)?;
        println!("installed {} {}", m.name, m.version);
        Ok(())
    })();
    let _ = fs::remove_dir_all(&tmp);
    result?;
    reapply(host);
    Ok(())
}

pub(super) fn remove(name: &str, host: &Host) -> Result<()> {
    let config_path = host.config_path;
    let (data, lock_file) = dirs(config_path)?;
    let dir = store::plugins_dir(&data).join(name);
    let mut lock = Lock::load(&lock_file);
    if !dir.is_dir() && !lock.remove(name) {
        bail!("no plugin named `{name}`");
    }
    lock.remove(name);
    lock.save(&lock_file)?;
    if dir
        .symlink_metadata()
        .is_ok_and(|m| m.file_type().is_symlink())
    {
        fs::remove_file(&dir)?; // a linked plugin: remove the link, never the author's folder
    } else if dir.is_dir() {
        fs::remove_dir_all(&dir)?;
    }
    super::binaries::remove_for(&data, name);
    println!("removed {name}");
    reapply(host);
    Ok(())
}

pub(super) fn update(name: Option<&str>, yes: bool, host: &Host) -> Result<()> {
    let config_path = host.config_path;
    let (data, lock_file) = dirs(config_path)?;
    let mut lock = Lock::load(&lock_file);
    let names: Vec<String> = match name {
        Some(n) => vec![n.to_string()],
        None => lock.plugin.iter().map(|e| e.name.clone()).collect(),
    };
    if names.is_empty() {
        println!("no plugins installed");
    }
    let mut changed = false;
    for n in names {
        let Some(entry) = lock.get(&n).cloned() else {
            bail!("no plugin named `{n}`")
        };
        if entry.dev {
            println!("{n}: linked for development, not updated");
            continue;
        }
        let dir = store::plugins_dir(&data).join(&n);
        let old = read_manifest(&dir)?;
        let local = matches!(entry.rev.as_str(), "local" | "official");
        let source = parse_source(&entry.source);
        let in_folder = matches!(
            source,
            Ok(Source::Git {
                subdir: Some(_),
                ..
            })
        );
        if !local
            && entry
                .source
                .rsplit_once('@')
                .is_some_and(|(r, v)| r.contains('/') && !r.contains(':') && !v.is_empty())
        {
            println!("{n}: pinned ({}), not updated", entry.source);
            continue;
        }
        if local || in_folder {
            // no clone is kept: fetch the source again and replace the plugin when its files differ
            let tmp = store::plugins_dir(&data).join(format!(".tmp-{}", std::process::id()));
            let _ = fs::remove_dir_all(&tmp);
            let rev = fetch_source(&entry.source, None, &tmp)?;
            if store::hash_dir(&tmp)? == entry.hash {
                println!("{n}: up to date");
                let _ = fs::remove_dir_all(&tmp);
                continue;
            }
            let new = read_manifest(&tmp)?;
            if new.name != n {
                let _ = fs::remove_dir_all(&tmp);
                bail!("{n}: the new manifest is named `{}`", new.name);
            }
            if !apply_update(&n, &old, &new, yes, &tmp) {
                let _ = fs::remove_dir_all(&tmp);
                continue;
            }
            fs::remove_dir_all(&dir)?;
            fs::rename(&tmp, &dir)?;
            let binaries = super::binaries::fetch_for(&new, &data)?;
            lock.set(LockEntry {
                version: new.version.clone(),
                rev: if local { entry.rev.clone() } else { rev },
                hash: store::hash_dir(&dir)?,
                approved: new.permission_lines(),
                binaries,
                ..entry
            });
            changed = true;
            continue;
        }
        git(Some(&dir), &["fetch", "--quiet", "origin"])?;
        let head = git(Some(&dir), &["rev-parse", "HEAD"])?;
        let upstream = git(Some(&dir), &["rev-parse", "FETCH_HEAD"])?;
        if head == upstream {
            println!("{n}: up to date");
            continue;
        }
        println!(
            "{}",
            git(Some(&dir), &["diff", "--stat", "HEAD", "FETCH_HEAD"])?
        );
        let new_text = git(Some(&dir), &["show", &format!("FETCH_HEAD:{FILE_NAME}")])?;
        let new =
            Manifest::parse(&new_text).map_err(|e| anyhow::anyhow!("new {FILE_NAME}: {e}"))?;
        if new.name != n {
            bail!("{n}: the new manifest is named `{}`", new.name);
        }
        if !apply_update(&n, &old, &new, yes, &dir) {
            continue;
        }
        git(Some(&dir), &["reset", "--quiet", "--hard", "FETCH_HEAD"])?;
        let binaries = super::binaries::fetch_for(&new, &data)?;
        lock.set(LockEntry {
            version: new.version.clone(),
            rev: upstream,
            hash: store::hash_dir(&dir)?,
            approved: new.permission_lines(),
            binaries,
            ..entry
        });
        changed = true;
    }
    lock.save(&lock_file)?;
    if changed {
        reapply(host);
    }
    Ok(())
}

/// Ask before applying an update. Changed permissions are asked even with `--yes`.
fn apply_update(name: &str, old: &Manifest, new: &Manifest, yes: bool, _dir: &Path) -> bool {
    let perms_changed = old.permission_lines() != new.permission_lines();
    println!("{name}: {} -> {}", old.version, new.version);
    if perms_changed {
        println!("  permissions it declares changed:");
        for l in new.permission_lines() {
            println!("    {l}");
        }
    }
    if yes || confirm("Update?") {
        true
    } else {
        println!("{name}: kept the old version");
        false
    }
}

/// `plugin link <dir>`: use a plugin straight from its folder while you develop it. Edits to its
/// scripts apply at once; edits to the manifest or snippets apply on the next `shellcraft apply`.
/// The files are not hash-checked, so a linked plugin is for your own code only.
pub(super) fn link(dir: &Path, host: &Host) -> Result<()> {
    let config_path = host.config_path;
    let (data, lock_file) = dirs(config_path)?;
    let src = dir
        .canonicalize()
        .with_context(|| format!("{} not found", dir.display()))?;
    let m = read_manifest(&src)?;
    let conflicts = command_conflicts(&m, &cache_dir()?, host.builtin_commands);
    if !conflicts.is_empty() {
        bail!("cannot link {}: {}", m.name, conflicts.join("; "));
    }
    check_requirements(&m, &data, &Lock::load(&lock_file))?;
    let root = store::plugins_dir(&data);
    fs::create_dir_all(&root)?;
    let target = root.join(&m.name);
    if target.symlink_metadata().is_ok() {
        bail!(
            "`{}` is already installed (run `plugin remove {}` first)",
            m.name,
            m.name
        );
    }
    std::os::unix::fs::symlink(&src, &target)?;
    let binaries = match super::binaries::fetch_for(&m, &data) {
        Ok(b) => b,
        Err(e) => {
            let _ = fs::remove_file(&target);
            return Err(e);
        }
    };
    let mut lock = Lock::load(&lock_file);
    lock.set(LockEntry {
        name: m.name.clone(),
        version: m.version.clone(),
        source: src.display().to_string(),
        rev: "link".to_string(),
        hash: String::new(),
        approved: m.permission_lines(),
        dev: true,
        binaries,
    });
    lock.save(&lock_file)?;
    println!(
        "linked {} {} -> {} (development mode: files are not hash-checked)",
        m.name,
        m.version,
        src.display()
    );
    reapply(host);
    Ok(())
}
