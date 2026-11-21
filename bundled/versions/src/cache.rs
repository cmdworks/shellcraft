//! The versions cache (`versions.tsv`) and the background refresh that fills it.

use shellcraft_core::proc::now_secs as now;
use shellcraft_core::proc::wait_timeout;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use super::ecosystem::{eco, parse_version, Eco};

/// Seconds before a cached version is asked for again.
pub const TTL_SECS: u64 = 600;
/// Retry delay while a refresh is in flight.
pub(super) const RETRY_SECS: u64 = 30;

pub(super) type Cache = HashMap<String, (String, u64)>;

pub(super) fn cache_file(dir: &Path) -> PathBuf {
    dir.join("versions.tsv")
}

pub(super) fn load_cache(dir: &Path) -> Cache {
    let Ok(text) = fs::read_to_string(cache_file(dir)) else {
        return Cache::new();
    };
    text.lines()
        .filter_map(|l| {
            let mut f = l.split('\t');
            Some((
                f.next()?.to_string(),
                (f.next()?.to_string(), f.next()?.parse().ok()?),
            ))
        })
        .collect()
}

pub(super) fn store_cache(dir: &Path, cache: &Cache) {
    let _ = fs::create_dir_all(dir);
    let mut keys: Vec<_> = cache.keys().collect();
    keys.sort();
    let text: String = keys
        .into_iter()
        .map(|k| format!("{k}\t{}\t{}\n", cache[k].0, cache[k].1))
        .collect();
    let tmp = dir.join(format!("versions.tmp{}", std::process::id()));
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(&tmp, cache_file(dir));
    }
}

pub(super) fn key(e: &Eco, dir: &Path) -> String {
    format!("{}@{}", e.name, dir.display())
}

/// Ask the tool for its version (3 s limit) and record it. Returns the version, or `None` when
/// the tool is missing (recorded as `-` so the prompt hides the segment).
pub fn refresh(
    name: &str,
    dir: &Path,
    cache_dir: &Path,
    path_override: Option<&str>,
) -> Option<String> {
    let e = eco(name)?;
    let mut cmd = Command::new(e.program);
    cmd.args(e.args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(p) = path_override {
        cmd.env("PATH", p);
    }
    let version = cmd.spawn().ok().and_then(|mut child| {
        if !wait_timeout(
            &mut child,
            Duration::from_secs(3),
            Duration::from_millis(10),
        )
        .ok()?
        {
            return None;
        }
        let mut out = String::new();
        std::io::Read::read_to_string(&mut child.stdout.take()?, &mut out).ok()?;
        parse_version(e, &out)
    });
    let mut cache = load_cache(cache_dir);
    cache.insert(
        key(e, dir),
        (version.clone().unwrap_or_else(|| "-".into()), now()),
    );
    store_cache(cache_dir, &cache);
    version
}

pub(super) fn spawn_refresh(name: &str, dir: &Path) {
    if let Ok(exe) = std::env::current_exe() {
        let _ = Command::new(exe)
            .args(["plugin", "refresh", "versions", name])
            .arg(dir)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sc-ver-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).expect("mkdir");
        d
    }

    #[test]
    fn refresh_records_version_and_missing_tools() {
        let proj = temp("refresh-proj");
        let cache = temp("refresh-cache");
        let bin = temp("refresh-bin");
        let fake = bin.join("node");
        fs::write(&fake, "#!/bin/sh\necho v20.1.0\n").expect("write");
        fs::set_permissions(&fake, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .expect("chmod");
        let path = bin.to_str().expect("utf8");
        assert_eq!(
            refresh("node", &proj, &cache, Some(path)).as_deref(),
            Some("20.1.0")
        );
        assert_eq!(
            refresh("bun", &proj, &cache, Some(path)),
            None,
            "bun is not on that PATH"
        );
        let c = load_cache(&cache);
        assert_eq!(c[&format!("node@{}", proj.display())].0, "20.1.0");
        assert_eq!(c[&format!("bun@{}", proj.display())].0, "-");
        for d in [proj, cache, bin] {
            let _ = fs::remove_dir_all(d);
        }
    }
}
