//! Installed plugins on disk, their lock entries, content hashes, and the baked segment list the
//! prompt reads.
//!
//! Layout: `<data>/plugins/<name>/` holds the plugin; `<config>/shellcraft.lock` records where it
//! came from and a content hash. A plugin whose files no longer match the lock is not loaded.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::manifest::{Manifest, FILE_NAME};

/// Where a plugin's downloaded prebuilt binaries live (`@name` in its manifest).
pub fn bin_dir(data_dir: &Path, plugin: &str) -> PathBuf {
    data_dir.join("plugin-bin").join(plugin)
}

/// Absolute path of a `run` or `command` value of an installed plugin: a file in its folder, or
/// `@name` for one of its downloaded binaries.
pub fn run_path(p: &Installed, run: &str) -> PathBuf {
    match run.strip_prefix('@') {
        Some(name) => {
            let data = p.dir.parent().and_then(Path::parent).unwrap_or(&p.dir);
            bin_dir(data, &p.manifest.name).join(name)
        }
        None => p.dir.join(run),
    }
}

fn file_sha256(path: &Path) -> std::io::Result<String> {
    Ok(Sha256::digest(fs::read(path)?)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

pub fn plugins_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("plugins")
}

/// `shellcraft.lock` next to the config file.
pub fn lock_path(config_file: &Path) -> PathBuf {
    config_file.with_file_name("shellcraft.lock")
}

/// SHA-256 over every file (sorted by path) except `.git`, covering names and contents.
pub fn hash_dir(dir: &Path) -> std::io::Result<String> {
    fn walk(dir: &Path, base: &Path, out: &mut Vec<PathBuf>) -> std::io::Result<()> {
        for e in fs::read_dir(dir)? {
            let e = e?;
            let p = e.path();
            if e.file_name() == ".git" {
                continue;
            }
            if e.file_type()?.is_dir() {
                walk(&p, base, out)?;
            } else {
                out.push(p.strip_prefix(base).unwrap_or(&p).to_path_buf());
            }
        }
        Ok(())
    }
    let mut files = Vec::new();
    walk(dir, dir, &mut files)?;
    files.sort();
    let mut h = Sha256::new();
    for f in files {
        h.update(f.to_string_lossy().as_bytes());
        h.update([0]);
        h.update(fs::read(dir.join(&f))?);
        h.update([0]);
    }
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LockEntry {
    pub name: String,
    pub version: String,
    /// Where it came from: `owner/repo`, a URL or a local path.
    pub source: String,
    /// Git commit, or `local`.
    pub rev: String,
    /// `hash_dir` of the installed files.
    pub hash: String,
    /// Permission lines the user approved.
    #[serde(default)]
    pub approved: Vec<String>,
    /// Linked for development (`plugin link`): used straight from the author's folder, so its
    /// files may change at any time and are not hash-checked.
    #[serde(default)]
    pub dev: bool,
    /// Downloaded prebuilt binaries: name -> SHA-256 of the installed file.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub binaries: BTreeMap<String, String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Lock {
    #[serde(default)]
    pub plugin: Vec<LockEntry>,
}

impl Lock {
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(
            path,
            format!("# written by `shellcraft plugin`; do not edit\n{text}"),
        )
    }

    pub fn get(&self, name: &str) -> Option<&LockEntry> {
        self.plugin.iter().find(|e| e.name == name)
    }

    pub fn set(&mut self, entry: LockEntry) {
        self.plugin.retain(|e| e.name != entry.name);
        self.plugin.push(entry);
        self.plugin.sort_by(|a, b| a.name.cmp(&b.name));
    }

    pub fn remove(&mut self, name: &str) -> bool {
        let n = self.plugin.len();
        self.plugin.retain(|e| e.name != name);
        self.plugin.len() != n
    }
}

/// A plugin found on disk, with the result of checking it against the lock.
#[derive(Debug, Clone)]
pub struct Installed {
    pub dir: PathBuf,
    pub manifest: Manifest,
    /// `Ok(())` when the lock entry exists and the content hash matches.
    pub trust: Result<(), String>,
}

/// Every plugin under `<data>/plugins`, checked against `lock`. Broken manifests are reported in
/// the second list instead of being loaded.
pub fn installed(data_dir: &Path, lock: &Lock) -> (Vec<Installed>, Vec<String>) {
    let mut ok = Vec::new();
    let mut problems = Vec::new();
    let Ok(rd) = fs::read_dir(plugins_dir(data_dir)) else {
        return (ok, problems);
    };
    let mut dirs: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    for dir in dirs {
        let label = dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if label.starts_with('.') {
            continue; // temporary clone directories
        }
        let manifest = match fs::read_to_string(dir.join(FILE_NAME))
            .map_err(|e| e.to_string())
            .and_then(|t| Manifest::parse(&t))
        {
            Ok(m) => m,
            Err(e) => {
                problems.push(format!("{label}: {e}"));
                continue;
            }
        };
        let trust = match lock.get(&manifest.name) {
            None => Err("not in shellcraft.lock (install it with `shellcraft plugin add`)".to_string()),
            Some(entry) if entry.dev => Ok(()),
            Some(entry) => match hash_dir(&dir) {
                Ok(h) if h == entry.hash => verify_binaries(data_dir, &manifest.name, entry),
                Ok(_) => Err("files changed since they were locked (run `shellcraft plugin update` or reinstall)".to_string()),
                Err(e) => Err(e.to_string()),
            },
        };
        ok.push(Installed {
            dir,
            manifest,
            trust,
        });
    }
    (ok, problems)
}

/// Every downloaded binary must still be the file that was installed.
fn verify_binaries(data_dir: &Path, plugin: &str, entry: &LockEntry) -> Result<(), String> {
    for (name, want) in &entry.binaries {
        match file_sha256(&bin_dir(data_dir, plugin).join(name)) {
            Ok(have) if &have == want => {}
            _ => {
                return Err(format!(
                    "binary `{name}` is missing or changed since it was installed (reinstall the plugin)"
                ))
            }
        }
    }
    Ok(())
}

// ---- baked segment list (read by the prompt) -------------------------------------------------

/// One manifest segment, resolved to an absolute command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baked {
    pub plugin: String,
    pub id: String,
    pub command: String,
    pub ttl_ms: u64,
    pub when: Vec<String>,
}

fn baked_file(cache_dir: &Path) -> PathBuf {
    cache_dir.join("plugins.tsv")
}

/// Write the segments of trusted plugins for the prompt (one line each).
pub fn bake(cache_dir: &Path, plugins: &[Installed]) -> std::io::Result<()> {
    let mut text = String::new();
    for p in plugins.iter().filter(|p| p.trust.is_ok()) {
        for (id, seg) in &p.manifest.segment {
            text.push_str(&format!(
                "{}\t{}\t{}\t{}\t{}\n",
                p.manifest.name,
                id,
                run_path(p, &seg.command).display(),
                seg.ttl_ms,
                seg.when.files.join(",")
            ));
        }
    }
    fs::create_dir_all(cache_dir)?;
    fs::write(baked_file(cache_dir), text)?;
    bake_extras(cache_dir, plugins)
}

/// A plugin command, resolved to an absolute path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BakedCommand {
    pub plugin: String,
    pub name: String,
    pub path: String,
    pub about: String,
}

/// A plugin hook, resolved to an absolute path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BakedHook {
    pub plugin: String,
    pub id: String,
    pub event: String,
    pub path: String,
}

/// Commands and hooks of trusted plugins (written next to `plugins.tsv`).
pub fn bake_extras(cache_dir: &Path, plugins: &[Installed]) -> std::io::Result<()> {
    let (mut cmds, mut hooks) = (String::new(), String::new());
    for p in plugins.iter().filter(|p| p.trust.is_ok()) {
        for (name, c) in &p.manifest.command {
            cmds.push_str(&format!(
                "{}\t{}\t{}\t{}\n",
                p.manifest.name,
                name,
                run_path(p, &c.run).display(),
                c.about.replace(['\t', '\n'], " ")
            ));
        }
        for (id, h) in &p.manifest.hook {
            hooks.push_str(&format!(
                "{}\t{}\t{}\t{}\n",
                p.manifest.name,
                id,
                h.event,
                run_path(p, &h.run).display()
            ));
        }
    }
    fs::create_dir_all(cache_dir)?;
    fs::write(cache_dir.join("commands.tsv"), cmds)?;
    let mut cheats = String::new();
    for p in plugins.iter().filter(|p| p.trust.is_ok()) {
        for (topic, c) in &p.manifest.cheat {
            cheats.push_str(&format!("{topic}\t{}\n", p.dir.join(&c.file).display()));
        }
    }
    fs::write(cache_dir.join("cheats.tsv"), cheats)?;
    fs::write(cache_dir.join("hooks.tsv"), hooks)
}

pub fn load_commands(cache_dir: &Path) -> Vec<BakedCommand> {
    let Ok(text) = fs::read_to_string(cache_dir.join("commands.tsv")) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            Some(BakedCommand {
                plugin: f.next()?.into(),
                name: f.next()?.into(),
                path: f.next()?.into(),
                about: f.next()?.into(),
            })
        })
        .collect()
}

pub fn load_hooks(cache_dir: &Path) -> Vec<BakedHook> {
    let Ok(text) = fs::read_to_string(cache_dir.join("hooks.tsv")) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            Some(BakedHook {
                plugin: f.next()?.into(),
                id: f.next()?.into(),
                event: f.next()?.into(),
                path: f.next()?.into(),
            })
        })
        .collect()
}

/// Segments written by the last `bake`.
pub fn load_baked(cache_dir: &Path) -> Vec<Baked> {
    let Ok(text) = fs::read_to_string(baked_file(cache_dir)) else {
        return Vec::new();
    };
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            Some(Baked {
                plugin: f.next()?.to_string(),
                id: f.next()?.to_string(),
                command: f.next()?.to_string(),
                ttl_ms: f.next()?.parse().ok()?,
                when: f
                    .next()?
                    .split(',')
                    .filter(|s| !s.is_empty())
                    .map(String::from)
                    .collect(),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sc-store-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).expect("mkdir");
        d
    }

    fn write_plugin(data: &Path, name: &str) -> PathBuf {
        let dir = plugins_dir(data).join(name);
        fs::create_dir_all(dir.join("bin")).expect("mkdir");
        fs::write(
            dir.join(FILE_NAME),
            format!("name = \"{name}\"\nversion = \"1.0.0\"\n[segment.s]\ncommand = \"bin/run\"\nwhen = {{ files = [\"go.mod\"] }}\n[command.hi]\nrun = \"bin/hi\"\nabout = \"say hi\"\n[hook.h]\nevent = \"cd\"\nrun = \"bin/run\"\n"),
        )
        .expect("write");
        fs::write(dir.join("bin/run"), "#!/bin/sh\necho hi\n").expect("write");
        dir
    }

    #[test]
    fn hash_covers_content_and_ignores_git() {
        let d = temp("hash");
        fs::write(d.join("a"), "one").expect("write");
        let h1 = hash_dir(&d).expect("hash");
        fs::create_dir_all(d.join(".git")).expect("mkdir");
        fs::write(d.join(".git/HEAD"), "x").expect("write");
        assert_eq!(hash_dir(&d).expect("hash"), h1, ".git is ignored");
        fs::write(d.join("a"), "two").expect("write");
        assert_ne!(hash_dir(&d).expect("hash"), h1);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn lock_roundtrip_and_trust_check() {
        let data = temp("lock");
        let dir = write_plugin(&data, "demo");
        let lockfile = data.join("shellcraft.lock");
        let mut lock = Lock::default();
        let (found, _) = installed(&data, &lock);
        assert!(found[0]
            .trust
            .as_ref()
            .expect_err("unlocked")
            .contains("not in shellcraft.lock"));
        lock.set(LockEntry {
            name: "demo".into(),
            version: "1.0.0".into(),
            source: "local".into(),
            rev: "local".into(),
            hash: hash_dir(&dir).expect("hash"),
            approved: vec!["network: no".into()],
            dev: false,
            binaries: BTreeMap::new(),
        });
        lock.save(&lockfile).expect("save");
        let lock = Lock::load(&lockfile);
        assert_eq!(
            lock.get("demo").map(|e| e.approved.clone()),
            Some(vec!["network: no".to_string()])
        );
        assert!(installed(&data, &lock).0[0].trust.is_ok());
        fs::write(dir.join("bin/run"), "#!/bin/sh\necho tampered\n").expect("write");
        let err = installed(&data, &lock).0[0]
            .trust
            .clone()
            .expect_err("changed");
        assert!(err.contains("changed"), "{err}");
        let mut lock = lock;
        assert!(lock.remove("demo") && !lock.remove("demo"));
        let _ = fs::remove_dir_all(&data);
    }

    #[test]
    fn bake_only_trusted_plugins_and_read_back() {
        let data = temp("bake");
        let cache = temp("bake-cache");
        let dir = write_plugin(&data, "demo");
        let mut lock = Lock::default();
        lock.set(LockEntry {
            name: "demo".into(),
            version: "1.0.0".into(),
            source: "local".into(),
            rev: "local".into(),
            hash: hash_dir(&dir).expect("hash"),
            approved: vec![],
            dev: false,
            binaries: BTreeMap::new(),
        });
        let (trusted, _) = installed(&data, &lock);
        bake(&cache, &trusted).expect("bake");
        let rows = load_baked(&cache);
        assert_eq!(rows.len(), 1);
        let cmds = load_commands(&cache);
        assert_eq!(
            (
                cmds[0].plugin.as_str(),
                cmds[0].name.as_str(),
                cmds[0].about.as_str()
            ),
            ("demo", "hi", "say hi")
        );
        assert!(cmds[0].path.ends_with("plugins/demo/bin/hi"));
        let hooks = load_hooks(&cache);
        assert_eq!((hooks[0].event.as_str(), hooks[0].id.as_str()), ("cd", "h"));
        assert_eq!(rows[0].plugin, "demo");
        assert_eq!(rows[0].when, vec!["go.mod"]);
        assert!(rows[0].command.ends_with("plugins/demo/bin/run"));
        let (untrusted, _) = installed(&data, &Lock::default());
        bake(&cache, &untrusted).expect("bake");
        assert!(
            load_baked(&cache).is_empty(),
            "untrusted plugins are not baked"
        );
        // a broken manifest is reported, not loaded
        let bad = plugins_dir(&data).join("broken");
        fs::create_dir_all(&bad).expect("mkdir");
        fs::write(bad.join(FILE_NAME), "name = ").expect("write");
        assert_eq!(installed(&data, &lock).1.len(), 1);
        for d in [data, cache] {
            let _ = fs::remove_dir_all(d);
        }
    }
}
