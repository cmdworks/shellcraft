//! Native tool download: fetch a release archive, verify its published SHA-256, install the binary.
//!
//! The checksum comes from the release's own `.sha256` file, so it catches corrupted or
//! truncated downloads but not a compromised release. Plain HTTP is accepted only for loopback.

use std::fs;
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};

const MAX_BYTES: u64 = 200 * 1024 * 1024;

/// tool name, binary name, release download base URL, archive name (`{triple}` is replaced)
pub const SOURCES: &[(&str, &str, &str, &str)] = &[
    (
        "starship",
        "starship",
        "https://github.com/starship/starship/releases/latest/download",
        "starship-{triple}.tar.gz",
    ),
    (
        "atuin",
        "atuin",
        "https://github.com/atuinsh/atuin/releases/latest/download",
        "atuin-{triple}.tar.gz",
    ),
];

/// SHA-256 of `bytes` as lowercase hex.
pub fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// First word of a `.sha256` file (`<hash>` or `<hash>  <name>`), if it is 64 hex digits.
fn parse_checksum(text: &str) -> Option<String> {
    let w = text.split_whitespace().next()?.to_ascii_lowercase();
    (w.len() == 64 && w.bytes().all(|b| b.is_ascii_hexdigit())).then_some(w)
}

pub fn fetch(url: &str) -> Result<Vec<u8>> {
    if !(url.starts_with("https://") || url.starts_with("http://127.0.0.1")) {
        bail!("refusing non-https url {url}");
    }
    let mut resp = ureq::get(url)
        .call()
        .with_context(|| format!("downloading {url}"))?;
    resp.body_mut()
        .with_config()
        .limit(MAX_BYTES)
        .read_to_vec()
        .with_context(|| format!("reading {url}"))
}

/// The file `bin` inside a `.tar.gz`: a bare name matches by file name anywhere in the archive,
/// a path (`bin/tool`) matches the whole entry path (a leading `./` is ignored).
pub fn extract_binary(targz: &[u8], bin: &str) -> Result<Vec<u8>> {
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(targz));
    for entry in archive.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?.into_owned();
        let hit = if bin.contains('/') {
            path.strip_prefix("./").unwrap_or(&path) == Path::new(bin)
        } else {
            path.file_name().is_some_and(|n| n == bin)
        };
        if hit {
            let mut data = Vec::new();
            entry.read_to_end(&mut data)?;
            return Ok(data);
        }
    }
    bail!("`{bin}` not found in the archive")
}

/// Read a plugin's prebuilt artifact and check its SHA-256.
///
/// `https://` (and loopback `http://`) URLs are downloaded. A `file://` URL or a bare file name is
/// only accepted when `dev_dir` is given (`SHELLCRAFT_DEV_ARTIFACTS`), and then only for files
/// inside that directory: development without a network, never a way to read arbitrary files.
pub fn read_artifact(url: &str, sha256: &str, dev_dir: Option<&Path>) -> Result<Vec<u8>> {
    let bytes = if url.starts_with("https://") || url.starts_with("http://127.0.0.1") {
        fetch(url)?
    } else {
        let Some(dir) = dev_dir else {
            bail!(
                "{url}: only https URLs are downloaded; set SHELLCRAFT_DEV_ARTIFACTS=<dir> to use local files while developing"
            );
        };
        let name = url.strip_prefix("file://").unwrap_or(url);
        let path = if Path::new(name).is_absolute() {
            PathBuf::from(name)
        } else {
            dir.join(name)
        };
        let (path, dir) = (path.canonicalize(), dir.canonicalize());
        let (Ok(path), Ok(dir)) = (path, dir) else {
            bail!("{url}: not found in the development artifacts directory");
        };
        if !path.starts_with(&dir) {
            bail!("{url}: outside the development artifacts directory");
        }
        fs::read(&path).with_context(|| format!("reading {}", path.display()))?
    };
    let have = sha256_hex(&bytes);
    if !have.eq_ignore_ascii_case(sha256) {
        bail!("{url}: checksum mismatch (expected {sha256}, got {have})");
    }
    Ok(bytes)
}

/// Download `<base>/<asset>` and `<base>/<asset>.sha256`, verify, and install `bin` into `dest`.
pub fn install(base: &str, asset: &str, bin: &str, dest: &Path) -> Result<PathBuf> {
    let archive = fetch(&format!("{base}/{asset}"))?;
    let sum_text = String::from_utf8_lossy(&fetch(&format!("{base}/{asset}.sha256"))?).into_owned();
    let expected = parse_checksum(&sum_text).context("checksum file has no sha256")?;
    let actual = sha256_hex(&archive);
    if actual != expected {
        bail!("checksum mismatch for {asset}: expected {expected}, got {actual}");
    }
    let data = extract_binary(&archive, bin)?;
    fs::create_dir_all(dest)?;
    let target = dest.join(bin);
    let tmp = dest.join(format!(".{bin}.tmp{}", std::process::id()));
    fs::write(&tmp, data)?;
    fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
    fs::rename(&tmp, &target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    fn targz(name: &str, body: &[u8]) -> Vec<u8> {
        let mut tar_bytes = Vec::new();
        {
            let mut b = tar::Builder::new(&mut tar_bytes);
            let mut h = tar::Header::new_gnu();
            h.set_size(body.len() as u64);
            h.set_mode(0o644);
            h.set_cksum();
            b.append_data(&mut h, name, body).expect("append");
            b.finish().expect("finish");
        }
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&tar_bytes).expect("gz");
        gz.finish().expect("gz finish")
    }

    /// Serve `routes` (path -> body) on a loopback port; returns the base URL.
    fn serve(routes: Vec<(String, Vec<u8>)>) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            for stream in listener.incoming().take(routes.len()) {
                let mut s = stream.expect("conn");
                let mut buf = [0u8; 2048];
                let n = s.read(&mut buf).expect("read");
                let req = String::from_utf8_lossy(&buf[..n]).into_owned();
                let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
                let body = routes
                    .iter()
                    .find(|(p, _)| *p == path)
                    .map(|(_, b)| b.clone());
                let head = if body.is_some() {
                    "200 OK"
                } else {
                    "404 Not Found"
                };
                let body = body.unwrap_or_default();
                let _ = write!(
                    s,
                    "HTTP/1.1 {head}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = s.write_all(&body);
            }
        });
        format!("http://127.0.0.1:{port}")
    }

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("sc-dl-{name}-{}", std::process::id()))
    }

    #[test]
    fn installs_after_checksum_matches() {
        let archive = targz("pkg-1/tool", b"#!/bin/sh\necho hi\n");
        let sum = format!("{}  tool.tar.gz\n", sha256_hex(&archive));
        let base = serve(vec![
            ("/tool.tar.gz".into(), archive),
            ("/tool.tar.gz.sha256".into(), sum.into_bytes()),
        ]);
        let dest = temp("ok");
        let path = install(&base, "tool.tar.gz", "tool", &dest).expect("install");
        assert_eq!(fs::read(&path).expect("read"), b"#!/bin/sh\necho hi\n");
        assert_eq!(
            fs::metadata(&path).expect("meta").permissions().mode() & 0o111,
            0o111
        );
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn checksum_mismatch_installs_nothing() {
        let archive = targz("tool", b"payload");
        let bad = "0".repeat(64);
        let base = serve(vec![
            ("/tool.tar.gz".into(), archive),
            ("/tool.tar.gz.sha256".into(), bad.into_bytes()),
        ]);
        let dest = temp("bad");
        let err = install(&base, "tool.tar.gz", "tool", &dest).expect_err("must fail");
        assert!(err.to_string().contains("checksum mismatch"), "{err}");
        assert!(!dest.join("tool").exists());
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn missing_binary_and_bad_inputs_fail() {
        let archive = targz("other", b"x");
        assert!(extract_binary(&archive, "tool").is_err());
        assert!(
            fetch("http://example.com/x").is_err(),
            "plain http is refused"
        );
        assert!(fetch("ftp://x/y").is_err());
        assert_eq!(parse_checksum("zz"), None);
        assert_eq!(
            parse_checksum(&format!("{}  f", "A".repeat(64))),
            Some("a".repeat(64))
        );
    }
}
