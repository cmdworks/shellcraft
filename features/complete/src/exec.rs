//! Running `--help` safely: which programs may be run, and how.

use shellcraft_core::proc::wait_timeout;
use std::fs;
use std::io::Read as _;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

const MAX_HELP_BYTES: usize = 128 * 1024;

/// Programs whose `--help` is never run: they can be destructive, are interactive, or start
/// something else.
pub(crate) const NEVER: &[&str] = &[
    "rm", "dd", "shred", "mkfs", "fdisk", "diskutil", "shutdown", "reboot", "halt", "poweroff",
    "init", "kill", "killall", "pkill", "sh", "bash", "zsh", "fish", "dash", "ksh", "csh", "tcsh",
    "nu", "pwsh", "python", "python3", "node", "ruby", "perl", "irb", "lua", "vi", "vim", "nvim",
    "emacs", "nano", "less", "more", "top", "htop", "ssh", "telnet", "nc", "sudo", "su", "doas",
    "login", "screen", "tmux",
];

/// Absolute path of `cmd` in `path`, if it is a plain executable in a directory only its owner
/// or root can write to.
pub fn resolve(cmd: &str, path: &str) -> Option<PathBuf> {
    if cmd.contains('/') || cmd.is_empty() {
        return None; // relative or explicit paths are never run
    }
    for dir in path.split(':').filter(|d| d.starts_with('/')) {
        let p = Path::new(dir).join(cmd);
        let Ok(meta) = fs::metadata(&p) else { continue };
        if !meta.is_file() || meta.permissions().mode() & 0o111 == 0 {
            continue;
        }
        let dir_mode = fs::metadata(dir).map_or(0o777, |m| m.permissions().mode());
        if dir_mode & 0o002 != 0 || meta.permissions().mode() & 0o002 != 0 {
            return None; // world-writable: anyone could have put it there
        }
        return Some(p);
    }
    None
}

/// Output of `<exe> <args...> --help` (stdout and stderr, 1 s limit, no stdin, no pager).
pub(crate) fn run_help(exe: &Path, args: &[String], cwd: &Path) -> Option<String> {
    let mut child = Command::new(exe)
        .args(args)
        .arg("--help")
        // A neutral directory: project files (rust-toolchain, .tool-versions, ...) must not change
        // what a tool does for `--help`.
        .current_dir(cwd)
        .env("LC_ALL", "C")
        .env("TERM", "dumb")
        .env("PAGER", "cat")
        .env("MANPAGER", "cat")
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    if !wait_timeout(&mut child, Duration::from_secs(1), Duration::from_millis(5)).ok()? {
        return None;
    }
    let mut text = String::new();
    for mut s in [
        child
            .stdout
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read>),
        child
            .stderr
            .take()
            .map(|s| Box::new(s) as Box<dyn std::io::Read>),
    ]
    .into_iter()
    .flatten()
    {
        let mut buf = Vec::new();
        let _ = std::io::Read::take(&mut s, MAX_HELP_BYTES as u64).read_to_end(&mut buf);
        text.push_str(&String::from_utf8_lossy(&buf));
        text.push('\n');
    }
    Some(text)
}
