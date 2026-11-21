//! `shellcraft update` for shellcraft itself: look at `<base>/version.txt`, and when it is newer
//! download `<base>/shellcraft-<triple>.tar.gz`, check its `.sha256`, and replace the running
//! binary. The release layout is produced by `scripts/release.sh`.

use std::path::Path;

use anyhow::{bail, Context, Result};
use shellcraft_platform as platform;

use crate::download;

/// Where releases are published (GitHub's "latest release" redirect). `SHELLCRAFT_UPDATE_URL`
/// overrides it.
pub const DEFAULT_BASE: &str = "https://github.com/cmdworks/shellcraft/releases/latest/download";

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    UpToDate(String),
    Available(String),
    Updated {
        from: String,
        to: String,
    },
    /// A package manager owns this binary; the string is what to run instead.
    Managed(&'static str),
}

fn parts(v: &str) -> Vec<u64> {
    v.trim()
        .trim_start_matches('v')
        .split('.')
        .map(|p| {
            p.split(|c: char| !c.is_ascii_digit())
                .next()
                .unwrap_or("0")
                .parse()
                .unwrap_or(0)
        })
        .collect()
}

/// Is `latest` a higher version than `current`? Dotted numbers compare numerically; with equal
/// numbers a release is newer than its pre-release (`0.1.0` over `0.1.0-alpha`), and pre-releases
/// compare by their text (`alpha` before `beta`).
pub fn newer(latest: &str, current: &str) -> bool {
    let split = |v: &str| {
        let v = v.trim().trim_start_matches('v');
        let (num, pre) = v.split_once('-').map_or((v, ""), |(n, p)| (n, p));
        (parts(num), pre.to_string())
    };
    let ((mut a, pa), (mut b, pb)) = (split(latest), split(current));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    match a.cmp(&b) {
        std::cmp::Ordering::Equal => match (pa.is_empty(), pb.is_empty()) {
            (true, false) => true,
            (false, true) | (true, true) => false,
            (false, false) => pa > pb,
        },
        other => other == std::cmp::Ordering::Greater,
    }
}

fn managed(exe: &Path) -> Option<&'static str> {
    let p = exe.canonicalize().unwrap_or_else(|_| exe.to_path_buf());
    let s = p.to_string_lossy();
    (s.contains("/Cellar/") || s.contains("/homebrew/")).then_some("brew upgrade shellcraft")
}

/// Check `base` for a newer version than `current`; unless `check_only`, install it over `exe`.
pub fn run(base: &str, current: &str, exe: &Path, check_only: bool) -> Result<Outcome> {
    if let Some(how) = managed(exe) {
        return Ok(Outcome::Managed(how));
    }
    let text = download::fetch(&format!("{base}/version.txt"))
        .context("looking for a newer shellcraft")?;
    let latest = String::from_utf8_lossy(&text).trim().to_string();
    if latest.is_empty()
        || !latest
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-+v".contains(c))
    {
        bail!("{base}/version.txt does not hold a version");
    }
    if !newer(&latest, current) {
        return Ok(Outcome::UpToDate(current.to_string()));
    }
    if check_only {
        return Ok(Outcome::Available(latest));
    }
    let triple = platform::release_triple().context("no release build for this system")?;
    if exe.file_name().and_then(|n| n.to_str()) != Some("shellcraft") {
        bail!("the running program is not called `shellcraft`; update it by hand");
    }
    let dir = exe.parent().context("the binary has no folder")?;
    download::install(
        base,
        &format!("shellcraft-{triple}.tar.gz"),
        "shellcraft",
        dir,
    )
    .with_context(|| format!("updating {} (is the folder writable?)", exe.display()))?;
    Ok(Outcome::Updated {
        from: current.to_string(),
        to: latest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_dotted_versions() {
        assert!(newer("0.2.0", "0.1.9") && newer("1.0", "0.9.9") && newer("v0.1.1", "0.1.0"));
        assert!(!newer("0.1.0", "0.1.0") && !newer("0.1.0", "0.2.0") && !newer("0.1", "0.1.0"));
        assert!(
            newer("0.1.0.1", "0.1.0") && newer("0.10.0", "0.9.0"),
            "numeric, not text, order"
        );
        assert!(
            newer("0.1.0", "0.1.0-alpha") && !newer("0.1.0-alpha", "0.1.0"),
            "a release beats its pre-release"
        );
        assert!(newer("0.1.0-beta", "0.1.0-alpha") && !newer("0.1.0-alpha", "0.1.0-alpha"));
        assert!(newer("0.1.1-alpha", "0.1.0") && !newer("0.1.0-alpha", "0.1.1"));
    }

    #[test]
    fn homebrew_binaries_are_left_to_homebrew() {
        assert_eq!(
            managed(Path::new(
                "/opt/homebrew/Cellar/shellcraft/0.1.0/bin/shellcraft"
            )),
            Some("brew upgrade shellcraft")
        );
        assert_eq!(managed(Path::new("/home/u/.local/bin/shellcraft")), None);
    }
}
