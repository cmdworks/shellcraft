//! Registries: git repositories (or plain folders) holding an `index.toml` that lists plugins by
//! name, so `plugin add <name>` and `plugin search` work without knowing repository URLs.
//!
//! ```toml
//! [[plugin]]
//! name = "todos"
//! version = "1.0.0"
//! description = "count TODOs"
//! repo = "owner/shellcraft-todos"   # folder (relative to the registry), owner/repo or git URL
//! rev = "v1.0.0"                    # optional tag or commit
//! hash = "…"                        # optional SHA-256 of the files; verified at install
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use clap::Subcommand;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use shellcraft_core::Config;
use shellcraft_plugin::manifest::KINDS;
use shellcraft_plugin::store;

use super::manage::{fetch_source, git, parse_source, Source, Spec};
use super::runtime::cache_dir;
use super::{load_config, Host};

pub const INDEX_FILE: &str = "index.toml";

/// One plugin in a registry index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "shellcraft_plugin::manifest::default_kind")]
    pub kind: String,
    #[serde(default)]
    pub version: String,
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rev: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    #[serde(default)]
    plugin: Vec<Entry>,
}

/// Parse and validate an `index.toml`.
pub fn parse_index(text: &str) -> Result<Vec<Entry>> {
    let index: Index = toml::from_str(text).context("index.toml")?;
    let word = |s: &str| {
        !s.is_empty()
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
    };
    for e in &index.plugin {
        if !word(&e.name) {
            bail!("index.toml: plugin name `{}` is not valid", e.name);
        }
        if e.repo.trim().is_empty() {
            bail!("index.toml: plugin `{}` has no repo", e.name);
        }
        if !KINDS.contains(&e.kind.as_str()) {
            bail!(
                "index.toml: plugin `{}` has unknown kind `{}`",
                e.name,
                e.kind
            );
        }
    }
    Ok(index.plugin)
}

fn short_hash(s: &str) -> String {
    Sha256::digest(s.as_bytes())
        .iter()
        .take(6)
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A local copy of the registry at `source`: the folder itself, or a clone kept in the cache
/// (fetched again when `refresh`).
fn checkout(source: &str, cache: &Path, refresh: bool) -> Result<(PathBuf, Option<GitInfo>)> {
    match parse_source(source)? {
        Source::Local(p) => Ok((p, None)),
        Source::Git { url, rev, subdir } => {
            let dir = cache.join("registries").join(short_hash(&url));
            if !dir.join(".git").exists() {
                fs::create_dir_all(dir.parent().unwrap_or(cache))?;
                let _ = fs::remove_dir_all(&dir);
                git(None, &["clone", "--quiet", &url, &dir.to_string_lossy()])?;
            } else if refresh {
                git(Some(&dir), &["pull", "--quiet", "--ff-only"])?;
            }
            if let Some(r) = rev {
                git(Some(&dir), &["checkout", "--quiet", &r])?;
            }
            let head = git(Some(&dir), &["rev-parse", "HEAD"])?;
            // a registry may live in a folder of the repository (`owner/repo/community`)
            let prefix = subdir.unwrap_or_default();
            let root = if prefix.is_empty() {
                dir
            } else {
                dir.join(&prefix)
            };
            if !root.join(INDEX_FILE).is_file() {
                bail!("no {INDEX_FILE} in {source}");
            }
            Ok((root, Some((url, head, prefix))))
        }
    }
}

/// The official registry that ships inside the binary, written to the cache and returned.
pub(super) fn official_dir() -> Result<PathBuf> {
    let dir = cache_dir()?.join("official");
    shellcraft_bundled_official::extract(&dir).context("preparing the official plugins")?;
    Ok(dir)
}

/// A plugin entry with the registry it came from.
pub struct Found {
    pub registry: String,
    pub dir: PathBuf,
    pub entry: Entry,
    /// For a registry that is a git repository: its URL and the commit that was read, so an
    /// installed plugin can be fetched again from the same place (`url#folder`).
    pub git: Option<GitInfo>,
}

/// Where a git registry was read: its URL, the commit, and the folder of the repository that holds it.
pub type GitInfo = (String, String, String);

/// The official plugins repository. Its index is read from a cached shallow clone, refreshed at
/// most once a day and never when offline; the copy embedded in the binary is the fallback.
pub const OFFICIAL_REMOTE: &str = "https://github.com/cmdworks/shellcraft-plugins.git";

fn stamp_age_secs(stamp: &Path) -> Option<u64> {
    let modified = fs::metadata(stamp).and_then(|m| m.modified()).ok()?;
    std::time::SystemTime::now()
        .duration_since(modified)
        .ok()
        .map(|d| d.as_secs())
}

/// The cached clone of [`OFFICIAL_REMOTE`], created or refreshed when it is due (`force`: now).
/// Failures (no network, no git) are silent: the embedded copy still works.
fn official_remote(cache: &Path, force: bool) -> Option<PathBuf> {
    let key = short_hash(OFFICIAL_REMOTE);
    let root = cache.join("registries");
    let dir = root.join(&key);
    let stamp = root.join(format!("{key}.stamp"));
    let cloned = dir.join(".git").exists();
    let age = stamp_age_secs(&stamp);
    // a clone is refreshed daily; a failed first clone is retried after an hour
    let limit = if cloned { 24 * 3600 } else { 3600 };
    let due = !shellcraft_core::proc::offline() && (force || age.map_or(true, |a| a >= limit));
    if due {
        let _ = fs::create_dir_all(&root);
        let result = if cloned {
            git_timeout(Some(&dir), &["pull", "--quiet", "--ff-only"])
        } else {
            let _ = fs::remove_dir_all(&dir);
            git_timeout(
                None,
                &[
                    "clone",
                    "--quiet",
                    "--depth",
                    "1",
                    OFFICIAL_REMOTE,
                    &dir.to_string_lossy(),
                ],
            )
        };
        let _ = fs::write(&stamp, if result.is_ok() { "ok" } else { "failed" });
    }
    dir.join(INDEX_FILE).is_file().then_some(dir)
}

/// `git` with a time limit, so a bad network never stalls a command for long.
fn git_timeout(dir: Option<&Path>, args: &[&str]) -> Result<()> {
    let mut cmd = std::process::Command::new("git");
    cmd.env("GIT_TERMINAL_PROMPT", "0")
        .args(["-c", "protocol.ext.allow=never"]);
    if let Some(d) = dir {
        cmd.arg("-C").arg(d);
    }
    let mut child = cmd
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("running git")?;
    let done = shellcraft_core::proc::wait_timeout(
        &mut child,
        std::time::Duration::from_secs(10),
        std::time::Duration::from_millis(20),
    )?;
    if done && child.wait()?.success() {
        Ok(())
    } else {
        bail!(
            "git {} failed or timed out",
            args.first().copied().unwrap_or("")
        )
    }
}

/// Every entry of every configured registry, in registry order.
pub fn all_entries(cfg: &Config, refresh: bool) -> Result<Vec<Found>> {
    let cache = cache_dir()?;
    // the official plugins come first and need no configuration: the copy embedded in the binary,
    // and the newer versions found in the official repository, when it could be read
    let mut out: Vec<Found> = Vec::new();
    if let Ok(dir) = official_dir() {
        if let Ok(entries) = fs::read_to_string(dir.join(INDEX_FILE))
            .map_err(anyhow::Error::from)
            .and_then(|t| parse_index(&t))
        {
            out.extend(entries.into_iter().map(|entry| Found {
                registry: "official".to_string(),
                dir: dir.clone(),
                entry,
                git: None,
            }));
        }
    }
    if let Some(dir) = official_remote(&cache, refresh) {
        let head = git(Some(&dir), &["rev-parse", "HEAD"]).ok();
        let parsed = fs::read_to_string(dir.join(INDEX_FILE))
            .map_err(anyhow::Error::from)
            .and_then(|t| parse_index(&t));
        if let (Ok(entries), Some(head)) = (parsed, head) {
            for entry in entries {
                match out.iter().position(|f| f.entry.name == entry.name) {
                    Some(i)
                        if !shellcraft_installer::selfupdate::newer(
                            &entry.version,
                            &out[i].entry.version,
                        ) => {}
                    found => {
                        let new = Found {
                            registry: "official".to_string(),
                            dir: dir.clone(),
                            entry,
                            git: Some((OFFICIAL_REMOTE.to_string(), head.clone(), String::new())),
                        };
                        match found {
                            Some(i) => out[i] = new,
                            None => out.push(new),
                        }
                    }
                }
            }
        }
    }
    for source in &cfg.registry.sources {
        let (dir, git_info) = match checkout(source, &cache, refresh) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("warning: registry {source}: {e:#}");
                continue;
            }
        };
        let text = match fs::read_to_string(dir.join(INDEX_FILE)) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("warning: registry {source}: cannot read {INDEX_FILE}: {e}");
                continue;
            }
        };
        if !cfg.registry.keys.is_empty() {
            if let Err(e) = super::sign::verify(&dir, &text, &cfg.registry.keys) {
                eprintln!("warning: registry {source} ignored: {e:#}");
                continue;
            }
        }
        match parse_index(&text) {
            Ok(entries) => out.extend(entries.into_iter().map(|entry| Found {
                registry: source.clone(),
                dir: dir.clone(),
                entry,
                git: git_info.clone(),
            })),
            Err(e) => eprintln!("warning: registry {source}: {e:#}"),
        }
    }
    Ok(out)
}

/// Where the plugin of `f` can be fetched from; a relative `repo` is a folder in the registry.
fn spec_for(f: &Found) -> Spec {
    if f.registry == "official" && f.git.is_none() {
        // recorded as `official:<folder>` so another machine (profile pull) and `update` can find it
        return Spec {
            source: format!("official:{}", f.entry.repo),
            rev: None,
            expect_hash: f.entry.hash.clone(),
        };
    }
    let relative = !Path::new(&f.entry.repo).is_absolute() && f.dir.join(&f.entry.repo).is_dir();
    // a plugin that lives in a git registry is fetched from that repository again later
    // (`<url>#<folder>`, pinned to the commit that was read), not from the local cache folder
    if let (true, Some((url, head, prefix))) = (relative, &f.git) {
        let folder = if prefix.is_empty() {
            f.entry.repo.clone()
        } else {
            format!("{prefix}/{}", f.entry.repo)
        };
        return Spec {
            source: format!("{url}#{folder}"),
            rev: Some(head.clone()),
            expect_hash: f.entry.hash.clone(),
        };
    }
    let source = if relative {
        f.dir.join(&f.entry.repo).display().to_string()
    } else {
        f.entry.repo.clone()
    };
    Spec {
        source,
        rev: f.entry.rev.clone(),
        expect_hash: f.entry.hash.clone(),
    }
}

/// A registry name is letters, digits, `-` and `_` only, and not an existing folder.
pub(super) fn looks_like_name(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        && !Path::new(s).exists()
}

/// What `plugin add <source>` should install: a registry lookup for a plain name, otherwise the
/// source as given.
pub fn resolve(source: &str, host: &Host) -> Result<Spec> {
    if !looks_like_name(source) {
        return Ok(Spec {
            source: source.to_string(),
            rev: None,
            expect_hash: None,
        });
    }
    let cfg = load_config(host.config_path);
    let found = all_entries(&cfg, false)?;
    let Some(f) = found.iter().find(|f| f.entry.name == source) else {
        bail!("no plugin named `{source}` (try `shellcraft search`)");
    };
    if f.registry != "official" {
        println!("{source}: from registry {}", f.registry);
    }
    Ok(spec_for(f))
}

/// `plugin search <query>`.
pub fn search(query: &str, host: &Host) -> Result<()> {
    let cfg = load_config(host.config_path);
    let q = query.to_lowercase();
    let mut shown = 0;
    for f in all_entries(&cfg, false)? {
        let e = &f.entry;
        if q.is_empty()
            || e.name.to_lowercase().contains(&q)
            || e.description.to_lowercase().contains(&q)
        {
            println!(
                "{:<16} {:<8} {:<8} {}",
                e.name, e.version, e.kind, e.description
            );
            shown += 1;
        }
    }
    if shown == 0 {
        println!("no plugin matches `{query}`");
    }
    Ok(())
}

/// `plugin info <name>`.
pub fn info(name: &str, host: &Host) -> Result<()> {
    let cfg = load_config(host.config_path);
    let found = all_entries(&cfg, false)?;
    let Some(f) = found.iter().find(|f| f.entry.name == name) else {
        bail!("no plugin named `{name}` in the configured registries");
    };
    let e = &f.entry;
    println!("{} {}  ({})", e.name, e.version, e.kind);
    if !e.description.is_empty() {
        println!("  {}", e.description);
    }
    println!("  registry: {}", f.registry);
    println!("  repo:     {}", e.repo);
    println!(
        "  rev:      {}",
        e.rev.as_deref().unwrap_or("default branch")
    );
    println!("  hash:     {}", e.hash.as_deref().unwrap_or("not pinned"));
    Ok(())
}

#[derive(Subcommand)]
pub enum RegistryCmd {
    /// List configured registries and how many plugins each offers
    List,
    /// Add a registry (a folder, `owner/repo` or a git URL) to the config
    Add { source: String },
    /// Remove a registry from the config
    Remove { source: String },
    /// Fetch the latest index of every registry
    Update,
    /// Create an empty registry in a folder (for development or your own plugins)
    Init { dir: PathBuf },
    /// Create a signing key pair: the secret key is written to FILE, the public key is printed
    Keygen { file: PathBuf },
    /// Sign a registry folder's index.toml with a secret key (writes index.toml.sig)
    Sign {
        registry: PathBuf,
        #[arg(long)]
        key: PathBuf,
    },
    /// Trust a registry public key (adds it to `[registry] keys`; registries must then be signed)
    Trust { key: String },
    /// Add or replace a plugin in a registry folder: records its version, commit and content hash
    Publish {
        /// The registry folder (see `registry init`)
        registry: PathBuf,
        /// The plugin: a folder, `owner/repo[@rev]` or a git URL
        source: String,
    },
}

/// Write `[registry] <key>` in the config text, keeping everything else.
fn set_list(text: &str, key: &str, values: &[String]) -> String {
    let value = format!(
        "{key} = [{}]",
        values
            .iter()
            .map(|s| toml::Value::String(s.clone()).to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    let Some(h) = lines.iter().position(|l| l.trim() == "[registry]") else {
        lines.push(String::new());
        lines.push("[registry]".to_string());
        lines.push(value);
        return lines.join("\n") + "\n";
    };
    let end = lines[h + 1..]
        .iter()
        .position(|l| l.trim_start().starts_with('['))
        .map_or(lines.len(), |i| h + 1 + i);
    if let Some(i) = lines[h + 1..end]
        .iter()
        .position(|l| l.trim_start().starts_with(key))
    {
        let start = h + 1 + i;
        let mut stop = start;
        while !lines[stop].contains(']') && stop + 1 < end {
            stop += 1;
        }
        lines.splice(start..=stop, [value]);
    } else {
        lines.insert(h + 1, value);
    }
    lines.join("\n") + "\n"
}

/// Edit a list in `[registry]` (`sources` or `keys`) of the config file.
fn edit_list(host: &Host, key: &str, f: impl FnOnce(&mut Vec<String>) -> Result<()>) -> Result<()> {
    let path = host.config_path;
    let text = fs::read_to_string(path).unwrap_or_default();
    let cfg = Config::from_toml(&text).unwrap_or_default();
    let mut list = if key == "keys" {
        cfg.registry.keys
    } else {
        cfg.registry.sources
    };
    f(&mut list)?;
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(path, set_list(&text, key, &list))?;
    Ok(())
}

pub fn run_registry(cmd: RegistryCmd, host: &Host) -> Result<()> {
    match cmd {
        RegistryCmd::List => {
            let cfg = load_config(host.config_path);
            if cfg.registry.sources.is_empty() {
                println!("no registries configured (`shellcraft registry add <source>`)");
            }
            let found = all_entries(&cfg, false)?;
            for s in &cfg.registry.sources {
                let n = found.iter().filter(|f| &f.registry == s).count();
                println!("{s}  ({n} plugins)");
            }
            Ok(())
        }
        RegistryCmd::Add { source } => {
            parse_source(&source)?; // must be a folder, owner/repo or a git URL
            edit_list(host, "sources", |s| {
                if s.contains(&source) {
                    bail!("{source} is already a registry");
                }
                s.push(source.clone());
                Ok(())
            })?;
            println!("added registry {source}");
            Ok(())
        }
        RegistryCmd::Remove { source } => {
            edit_list(host, "sources", |s| {
                let n = s.len();
                s.retain(|x| *x != source);
                if s.len() == n {
                    bail!("{source} is not a configured registry");
                }
                Ok(())
            })?;
            println!("removed registry {source}");
            Ok(())
        }
        RegistryCmd::Update => {
            let cfg = load_config(host.config_path);
            let n = all_entries(&cfg, true)?.len();
            println!("{n} plugins available");
            Ok(())
        }
        RegistryCmd::Init { dir } => {
            fs::create_dir_all(&dir)?;
            let index = dir.join(INDEX_FILE);
            if index.exists() {
                bail!("{} already exists", index.display());
            }
            fs::write(
                &index,
                "# shellcraft plugin registry. Add entries with `shellcraft registry publish`.\n",
            )?;
            let _ = git(Some(&dir), &["init", "--quiet"]);
            println!("created registry {}", dir.display());
            Ok(())
        }
        RegistryCmd::Keygen { file } => {
            let public = super::sign::keygen(&file)?;
            println!(
                "secret key written to {} (keep it private)\npublic key: {public}",
                file.display()
            );
            Ok(())
        }
        RegistryCmd::Sign { registry, key } => {
            let public = super::sign::sign(&registry, &key)?;
            println!(
                "signed {}/index.toml (public key {public})",
                registry.display()
            );
            Ok(())
        }
        RegistryCmd::Trust { key } => {
            let mut cfg = load_config(host.config_path);
            cfg.registry.keys.push(key.clone());
            cfg.validate()?; // rejects anything that is not a 64-digit hex key
            edit_list(host, "keys", |k| {
                if !k.contains(&key) {
                    k.push(key.clone());
                }
                Ok(())
            })?;
            println!("trusting {key}; registries must now be signed with it");
            Ok(())
        }
        RegistryCmd::Publish { registry, source } => publish(&registry, &source),
    }
}

fn publish(registry: &Path, source: &str) -> Result<()> {
    let index_path = registry.join(INDEX_FILE);
    let text = fs::read_to_string(&index_path)
        .with_context(|| format!("{} (run `registry init` first)", index_path.display()))?;
    let mut entries = parse_index(&text)?;
    let tmp = std::env::temp_dir().join(format!("sc-publish-{}", std::process::id()));
    let _ = fs::remove_dir_all(&tmp);
    let result = (|| -> Result<Entry> {
        let rev = fetch_source(source, None, &tmp)?;
        let m = super::manage::read_manifest(&tmp)?;
        let repo = match parse_source(source)? {
            // a plugin folder inside the registry is recorded relative to it
            Source::Local(p) => match registry
                .canonicalize()
                .ok()
                .and_then(|r| p.strip_prefix(r).ok().map(Path::to_path_buf))
            {
                Some(rel) => rel.display().to_string(),
                None => p.display().to_string(),
            },
            Source::Git { .. } => source.to_string(),
        };
        Ok(Entry {
            name: m.name.clone(),
            description: m.description.clone(),
            kind: m.kind.clone(),
            version: m.version.clone(),
            repo,
            rev: (rev != "local").then_some(rev),
            hash: Some(store::hash_dir(&tmp)?),
        })
    })();
    let _ = fs::remove_dir_all(&tmp);
    let entry = result?;
    entries.retain(|e| e.name != entry.name);
    println!("published {} {}", entry.name, entry.version);
    entries.push(entry);
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    let body = toml::to_string_pretty(&Index { plugin: entries })?;
    fs::write(index_path, format!("# shellcraft plugin registry\n{body}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_parses_and_validates() {
        let ok = "[[plugin]]\nname = \"todos\"\nrepo = \"o/r\"\nrev = \"v1\"\nversion = \"1.0\"\n";
        let e = parse_index(ok).expect("valid");
        assert_eq!(
            (e[0].name.as_str(), e[0].kind.as_str(), e[0].rev.as_deref()),
            ("todos", "feature", Some("v1"))
        );
        assert!(parse_index("[[plugin]]\nname = \"a b\"\nrepo = \"x\"\n").is_err());
        assert!(parse_index("[[plugin]]\nname = \"a\"\nrepo = \"\"\n").is_err());
        assert!(parse_index("[[plugin]]\nname = \"a\"\nrepo = \"x\"\nkind = \"weird\"\n").is_err());
        assert!(parse_index("[[plugin]]\nname = \"a\"\nrepo = \"x\"\nsurprise = 1\n").is_err());
    }

    #[test]
    fn names_versus_sources() {
        assert!(looks_like_name("todos") && looks_like_name("my-plugin_2"));
        assert!(
            !looks_like_name("owner/repo") && !looks_like_name("a@v1") && !looks_like_name("./x")
        );
        assert!(!looks_like_name("/tmp"), "an existing folder is a source");
    }

    #[test]
    fn sources_line_is_added_replaced_and_removed() {
        let added = set_list(
            "[shell]\nhistory_size = 5\n",
            "sources",
            &["a".into(), "b/c".into()],
        );
        assert!(
            added.ends_with("[registry]\nsources = [\"a\", \"b/c\"]\n"),
            "{added}"
        );
        let replaced = set_list(&added, "sources", &["z".into()]);
        assert_eq!(replaced.matches("[registry]").count(), 1);
        assert!(replaced.contains("sources = [\"z\"]") && !replaced.contains("b/c"));
        let multi = set_list(
            "[registry]\nsources = [\n  \"a\",\n]\n[env]\nA = \"1\"\n",
            "sources",
            &["n".into()],
        );
        assert_eq!(multi, "[registry]\nsources = [\"n\"]\n[env]\nA = \"1\"\n");
        assert!(Config::from_toml(&replaced).is_ok());
    }
}
