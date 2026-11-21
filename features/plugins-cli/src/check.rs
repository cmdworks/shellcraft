//! `plugin check`: validate a plugin directory and run its segments once.

use shellcraft_core::proc::wait_timeout;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use super::manage::read_manifest;

pub(super) fn check(path: &Path) -> Result<()> {
    let path = &path
        .canonicalize()
        .with_context(|| format!("{} not found", path.display()))?;
    let m = read_manifest(path)?;
    let mut failed = 0;
    let mut report = |ok: bool, text: String| {
        println!("{} {text}", if ok { "  ok  " } else { " FAIL " });
        if !ok {
            failed += 1;
        }
    };
    report(true, format!("manifest {} {}", m.name, m.version));
    if m.description.is_empty() {
        println!(" note   description is empty");
    }
    for (shell, file) in &m.snippets {
        let full = path.join(file);
        if !full.is_file() {
            report(false, format!("snippet {shell}: {file} is missing"));
            continue;
        }
        let tool = match shell.as_str() {
            "zsh" => Some(("zsh", vec!["-n"])),
            "bash" => Some(("bash", vec!["-n"])),
            "fish" => Some(("fish", vec!["-n"])),
            _ => None,
        };
        match tool.and_then(|(bin, args)| Command::new(bin).args(args).arg(&full).stdin(Stdio::null()).output().ok()) {
            Some(out) if out.status.success() => report(true, format!("snippet {shell}: syntax ok")),
            Some(out) => report(false, format!("snippet {shell}: {}", String::from_utf8_lossy(&out.stderr).trim())),
            None => println!(" note   snippet {shell}: syntax not checked (shell not installed or not checkable)"),
        }
    }
    let executables = m
        .command
        .iter()
        .map(|(n, c)| (format!("command {n}"), &c.run))
        .chain(
            m.hook
                .iter()
                .map(|(i, h)| (format!("hook {i} ({})", h.event), &h.run)),
        );
    for (label, run) in executables {
        if run.starts_with('@') {
            // a prebuilt binary: fetched and checksummed at install, not part of the folder
            println!(" note   {label}: {run} is a prebuilt binary (checked when installed)");
            continue;
        }
        let f = path.join(run);
        let ok = fs::metadata(&f).is_ok_and(|md| {
            md.is_file() && std::os::unix::fs::PermissionsExt::mode(&md.permissions()) & 0o111 != 0
        });
        report(
            ok,
            format!(
                "{label}: {run} {}",
                if ok {
                    "is executable"
                } else {
                    "is missing or not executable"
                }
            ),
        );
    }
    for (id, seg) in &m.segment {
        if seg.command.starts_with('@') {
            println!(
                " note   segment {id}: {} is a prebuilt binary (checked when installed)",
                seg.command
            );
            continue;
        }
        let exe = path.join(&seg.command);
        if !exe.is_file() {
            report(false, format!("segment {id}: {} is missing", seg.command));
            continue;
        }
        let start = Instant::now();
        let child = Command::new(&exe)
            .current_dir(path)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .stdout(Stdio::piped())
            .spawn();
        let Ok(mut child) = child else {
            report(
                false,
                format!(
                    "segment {id}: cannot run {} (is it executable?)",
                    seg.command
                ),
            );
            continue;
        };
        let status =
            match wait_timeout(&mut child, Duration::from_secs(2), Duration::from_millis(5))? {
                true => child.wait().ok(),
                false => None,
            };
        let ms = start.elapsed().as_millis();
        let mut out = String::new();
        if let Some(mut so) = child.stdout.take() {
            let _ = std::io::Read::read_to_string(&mut so, &mut out);
        }
        match status {
            None => report(false, format!("segment {id}: timed out after 2 s")),
            Some(s) if !s.success() => report(false, format!("segment {id}: exited with {s}")),
            Some(_) => {
                let line = out.lines().next().unwrap_or("").trim().to_string();
                let slow = if ms > 50 {
                    " (slow: prompts show the cached line while it refreshes)"
                } else {
                    ""
                };
                report(
                    true,
                    format!("segment {id}: {ms} ms, prints {:?}{slow}", line),
                );
            }
        }
    }
    if failed > 0 {
        bail!("{failed} problem(s)");
    }
    Ok(())
}
