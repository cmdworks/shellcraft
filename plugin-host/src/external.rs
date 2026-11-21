//! Outside plugins: any executable that prints one line becomes a prompt segment.
//!
//! Configure it as `[plugin.<name>] segment = "/path/to/exe"` and list `<name>` in
//! `[prompt] segments`. The executable prints a single line, optionally starting with a tone in
//! brackets (`[warn] 3 todos`; tones: git, version, tool, warn, dim; default `tool`). Empty output
//! shows nothing.
//!
//! The prompt never runs it. It shows the last output cached for the current directory, and when
//! that is older than `ttl_ms` it starts `shellcraft plugin refresh segment <name> <dir>` in the
//! background, which runs the executable (2 s limit) and stores the line.

use shellcraft_core::proc::wait_timeout;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::{Ctx, Segment, Tone};

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}

fn cache_file(dir: &Path, name: &str) -> PathBuf {
    dir.join("segments").join(format!("{name}.txt"))
}

/// Parse `[tone] text` (or plain text) into a segment.
pub fn parse_line(line: &str) -> Option<Segment> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    if let Some(rest) = line.strip_prefix('[') {
        if let Some((tone, text)) = rest.split_once(']') {
            let tone = match tone.trim() {
                "git" => Some(Tone::Git),
                "version" => Some(Tone::Version),
                "tool" => Some(Tone::Tool),
                "warn" => Some(Tone::Warn),
                "dim" => Some(Tone::Dim),
                "ok" => Some(Tone::Ok),
                "err" => Some(Tone::Err),
                _ => None,
            };
            if let (Some(tone), text) = (tone, text.trim()) {
                return (!text.is_empty()).then(|| Segment::new(text, tone));
            }
        }
    }
    Some(Segment::new(line, Tone::Tool))
}

/// Cached line for `cwd` (`ts_ms \t cwd \t line`), and whether it is stale.
fn read_cache(file: &Path, cwd: &Path, ttl_ms: u64) -> Option<(String, bool)> {
    let text = fs::read_to_string(file).ok()?;
    let mut f = text.lines().next()?.splitn(3, '\t');
    let ts: u128 = f.next()?.parse().ok()?;
    if Path::new(f.next()?) != cwd {
        return None;
    }
    let line = f.next().unwrap_or("").to_string();
    Some((line, now_ms().saturating_sub(ts) >= u128::from(ttl_ms)))
}

/// Segment for plugin `name`, from cache; starts a background refresh when it is stale.
pub fn segments(name: &str, ttl_ms: u64, ctx: &Ctx) -> Vec<Segment> {
    let Some(dir) = ctx.cache_dir else {
        return Vec::new();
    };
    let file = cache_file(dir, name);
    let cached = read_cache(&file, ctx.cwd, ttl_ms);
    let needs_refresh = cached.as_ref().map_or(true, |(_, stale)| *stale);
    if needs_refresh && (ctx.env)("SHELLCRAFT_NO_BG").is_none() {
        if let Ok(exe) = std::env::current_exe() {
            let _ = Command::new(exe)
                .args(["plugin", "refresh", "segment", name])
                .arg(ctx.cwd)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn();
        }
    }
    cached
        .and_then(|(line, _)| parse_line(&line))
        .into_iter()
        .collect()
}

/// Segment of an installed plugin (a row of the baked list): hidden unless one of its `when`
/// files exists in the project, otherwise cached and refreshed like any outside segment.
pub fn baked_segments(row: &crate::store::Baked, ctx: &Ctx) -> Vec<Segment> {
    if !row.when.is_empty()
        && !ctx
            .cwd
            .ancestors()
            .take(8)
            .any(|d| row.when.iter().any(|f| d.join(f).exists()))
    {
        return Vec::new();
    }
    segments(&format!("{}.{}", row.plugin, row.id), row.ttl_ms, ctx)
}

/// Run `command` in `cwd` (2 s limit) and store its first output line for the prompt.
pub fn refresh(name: &str, command: &str, cwd: &Path, cache_dir: &Path) -> std::io::Result<()> {
    let mut child = Command::new(command)
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let line = if wait_timeout(
        &mut child,
        Duration::from_secs(2),
        Duration::from_millis(10),
    )? {
        let mut out = String::new();
        if let Some(mut so) = child.stdout.take() {
            std::io::Read::read_to_string(&mut so, &mut out)?;
        }
        out.lines().next().unwrap_or("").to_string()
    } else {
        String::new()
    };
    let file = cache_file(cache_dir, name);
    if let Some(parent) = file.parent() {
        fs::create_dir_all(parent)?;
    }
    let tmp = file.with_extension(format!("tmp{}", std::process::id()));
    fs::write(
        &tmp,
        format!(
            "{}\t{}\t{}\n",
            now_ms(),
            cwd.display(),
            line.replace('\t', " ")
        ),
    )?;
    fs::rename(&tmp, file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn parses_tone_prefix() {
        assert_eq!(
            parse_line("[warn] 3 todos"),
            Some(Segment::new("3 todos", Tone::Warn))
        );
        assert_eq!(parse_line("plain"), Some(Segment::new("plain", Tone::Tool)));
        assert_eq!(
            parse_line("[unknown] x"),
            Some(Segment::new("[unknown] x", Tone::Tool))
        );
        assert_eq!(parse_line("   "), None);
        assert_eq!(parse_line("[dim]"), None);
    }

    #[test]
    fn refresh_then_read_from_cache() {
        let root = std::env::temp_dir().join(format!("sc-ext-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("mkdir");
        let exe = root.join("seg");
        fs::write(
            &exe,
            "#!/bin/sh\necho \"[version] hello from $(basename \"$PWD\")\"\n",
        )
        .expect("write");
        fs::set_permissions(&exe, fs::Permissions::from_mode(0o755)).expect("chmod");
        let cache = root.join("cache");
        refresh("demo", exe.to_str().expect("utf8"), &root, &cache).expect("refresh");
        let env = |k: &str| (k == "SHELLCRAFT_NO_BG").then(|| "1".to_string());
        let none = |_: &str| None;
        let ctx = Ctx {
            cwd: &root,
            home: None,
            cache_dir: Some(&cache),
            env: &env,
            opt: &none,
        };
        let segs = segments("demo", 60_000, &ctx);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].parts[0].1, Tone::Version);
        assert!(segs[0].parts[0].0.starts_with("hello from sc-ext-"));
        // another directory does not reuse the cached line
        let other = Ctx {
            cwd: &cache,
            home: None,
            cache_dir: Some(&cache),
            env: &env,
            opt: &none,
        };
        assert!(segments("demo", 60_000, &other).is_empty());
        // a missing executable is an error, not a panic
        assert!(refresh("bad", "/no/such/exe", &root, &cache).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
