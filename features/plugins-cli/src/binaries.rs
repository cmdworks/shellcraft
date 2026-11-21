//! Prebuilt binaries of a plugin: download or read, verify, unpack, store outside the plugin
//! folder (so the plugin's content hash stays about its source).

use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use shellcraft_installer::download::{extract_binary, read_artifact, sha256_hex};
use shellcraft_plugin::manifest::{BinaryDef, Manifest};
use shellcraft_plugin::store::bin_dir;

/// Folder with local artifacts for development (`SHELLCRAFT_DEV_ARTIFACTS`).
fn dev_dir() -> Option<PathBuf> {
    std::env::var_os("SHELLCRAFT_DEV_ARTIFACTS")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

/// The triple builds are chosen by; `SHELLCRAFT_DEV_TARGET` overrides it for tests.
fn target() -> Result<String> {
    if let Ok(t) = std::env::var("SHELLCRAFT_DEV_TARGET") {
        return Ok(t);
    }
    shellcraft_platform::release_triple()
        .map(str::to_string)
        .context("prebuilt binaries: unsupported OS or CPU")
}

/// Lines for the install summary: what will be downloaded.
pub fn summary_lines(m: &Manifest) -> Vec<String> {
    let Ok(t) = target() else {
        return Vec::new();
    };
    let (have, missing) = m.binaries_for(&t);
    let mut lines: Vec<String> = have
        .iter()
        .map(|b| {
            format!(
                "binary:   {} from {} (sha256 {}...)",
                b.name,
                b.url,
                &b.sha256[..12]
            )
        })
        .collect();
    lines.extend(
        missing
            .iter()
            .map(|n| format!("binary:   {n} has NO build for {t}")),
    );
    lines
}

fn install_one(b: &BinaryDef, dir: &Path) -> Result<String> {
    let bytes = read_artifact(&b.url, &b.sha256, dev_dir().as_deref())?;
    let data = match b.archive.as_str() {
        "tar.gz" => extract_binary(&bytes, b.member.as_deref().unwrap_or(&b.name))?,
        _ => bytes,
    };
    fs::create_dir_all(dir)?;
    let path = dir.join(&b.name);
    let tmp = dir.join(format!(".{}.tmp", b.name));
    fs::write(&tmp, &data)?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
    fs::rename(&tmp, &path)?;
    Ok(sha256_hex(&data))
}

/// Install the binaries `m` needs on this machine. Returns name -> SHA-256 of the installed
/// file (for the lock). A plugin with no binaries returns an empty map; one that lacks a build
/// for this platform is an error. On failure nothing is left behind.
pub fn fetch_for(m: &Manifest, data_dir: &Path) -> Result<BTreeMap<String, String>> {
    let mut out = BTreeMap::new();
    if m.binary.is_empty() {
        return Ok(out);
    }
    let t = target()?;
    let (have, missing) = m.binaries_for(&t);
    if !missing.is_empty() {
        bail!(
            "{}: no prebuilt binary of `{}` for {t}",
            m.name,
            missing.join("`, `")
        );
    }
    let dir = bin_dir(data_dir, &m.name);
    let _ = fs::remove_dir_all(&dir);
    for b in have {
        match install_one(b, &dir) {
            Ok(sha) => {
                out.insert(b.name.clone(), sha);
            }
            Err(e) => {
                let _ = fs::remove_dir_all(&dir);
                return Err(e.context(format!("{}: binary `{}`", m.name, b.name)));
            }
        }
    }
    Ok(out)
}

/// Remove a plugin's downloaded binaries.
pub fn remove_for(data_dir: &Path, plugin: &str) {
    let _ = fs::remove_dir_all(bin_dir(data_dir, plugin));
}
