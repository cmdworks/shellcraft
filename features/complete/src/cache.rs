//! The on-disk cache of parsed help texts.

use shellcraft_core::proc::now_secs as now;
use std::fs;
use std::path::{Path, PathBuf};

use crate::parse::Spec;

/// How long a parsed help text is trusted.
pub const TTL_SECS: u64 = 7 * 24 * 3600;
/// How long a failed attempt (timeout, error) is remembered, so Tab does not retry every time.
const NEGATIVE_TTL_SECS: u64 = 3600;

pub(crate) fn cache_file(dir: &Path, node: &[String]) -> PathBuf {
    let key: String = node
        .join(".")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "-_.".contains(c) {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join("completions").join(format!("{key}.tsv"))
}

pub(crate) fn write_cache(file: &Path, exe_mtime: i64, spec: Option<&Spec>) {
    if let Some(d) = file.parent() {
        let _ = fs::create_dir_all(d);
    }
    let mut text = format!(
        "# {}\t{}{}\n",
        now(),
        exe_mtime,
        if spec.is_some() { "" } else { "\tneg" }
    );
    let empty = Spec::default();
    let spec = spec.unwrap_or(&empty);
    for (n, d) in &spec.subcommands {
        text.push_str(&format!("s\t{n}\t{}\n", d.replace('\t', " ")));
    }
    for (n, d) in &spec.flags {
        text.push_str(&format!("f\t{n}\t{}\n", d.replace('\t', " ")));
    }
    let tmp = file.with_extension(format!("tmp{}", std::process::id()));
    if fs::write(&tmp, text).is_ok() {
        let _ = fs::rename(&tmp, file);
    }
}

pub(crate) fn read_cache(file: &Path, exe_mtime: i64) -> Option<Spec> {
    let text = fs::read_to_string(file).ok()?;
    let mut lines = text.lines();
    let mut head = lines.next()?.strip_prefix("# ")?.split('\t');
    let ts: u64 = head.next()?.parse().ok()?;
    let mtime: i64 = head.next()?.parse().ok()?;
    let ttl = if head.next() == Some("neg") {
        NEGATIVE_TTL_SECS
    } else {
        TTL_SECS
    };
    if mtime != exe_mtime || now().saturating_sub(ts) > ttl {
        return None; // the program changed or the entry is old
    }
    let mut spec = Spec::default();
    for l in lines {
        let mut f = l.splitn(3, '\t');
        let (kind, name, desc) = (
            f.next()?,
            f.next()?.to_string(),
            f.next().unwrap_or("").to_string(),
        );
        match kind {
            "s" => spec.subcommands.push((name, desc)),
            "f" => spec.flags.push((name, desc)),
            _ => {}
        }
    }
    Some(spec)
}
