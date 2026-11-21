//! Completion learned from `--help`.
//!
//! For a command that has no completion of its own, shellcraft runs `<cmd> [sub...] --help` once
//! (with safety checks and a 1 s limit), parses the flags and subcommands out of the text and
//! caches them. Candidates are then answered from the cache without running anything.

mod cache;
mod cmd;
mod exec;
mod parse;

pub use cmd::{run, CompleteArgs};

use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub use cache::TTL_SECS;
use cache::{cache_file, read_cache, write_cache};
pub use exec::resolve;
use exec::{run_help, NEVER};
pub use parse::{parse_help, Spec};

const MAX_DEPTH: usize = 4;

/// Settings that limit what is run.
#[derive(Debug, Clone, Default)]
pub struct Policy {
    /// Extra command names never to run.
    pub skip: Vec<String>,
}

/// Spec for `node` (`["git"]` or `["git", "remote"]`), from the cache or by running `--help`.
pub fn spec_for(node: &[String], path: &str, cache_dir: &Path, policy: &Policy) -> Option<Spec> {
    let cmd = node.first()?;
    if NEVER.contains(&cmd.as_str()) || policy.skip.iter().any(|s| s == cmd) {
        return None;
    }
    let exe = resolve(cmd, path)?;
    let mtime = fs::metadata(&exe).ok()?.mtime();
    let file = cache_file(cache_dir, node);
    if let Some(spec) = read_cache(&file, mtime) {
        return Some(spec);
    }
    let Some(text) = run_help(&exe, &node[1..], cache_dir) else {
        write_cache(&file, mtime, None);
        return Some(Spec::default());
    };
    let spec = parse_help(&text);
    write_cache(&file, mtime, Some(&spec));
    Some(spec)
}

/// Candidates `(value, description)` for the last of `words` (the one being completed).
///
/// `words[0]` is the command. Words that are known subcommands move the lookup one level down;
/// flags are skipped; any other word counts as a positional argument and ends subcommand
/// completion.
pub fn complete(
    words: &[String],
    path: &str,
    cache_dir: &Path,
    policy: &Policy,
) -> Vec<(String, String)> {
    let Some((current, before)) = words.split_last() else {
        return Vec::new();
    };
    if before.is_empty() {
        return Vec::new();
    }
    let mut node = vec![before[0].clone()];
    let mut positional = false;
    for w in &before[1..] {
        if w.starts_with('-') || positional {
            continue;
        }
        let known = spec_for(&node, path, cache_dir, policy)
            .is_some_and(|s| s.subcommands.iter().any(|(n, _)| n == w));
        if known && node.len() < MAX_DEPTH {
            node.push(w.clone());
        } else {
            positional = true;
        }
    }
    let Some(spec) = spec_for(&node, path, cache_dir, policy) else {
        return Vec::new();
    };
    let mut out: Vec<(String, String)> = if current.starts_with('-') {
        spec.flags
    } else if positional {
        Vec::new()
    } else {
        spec.subcommands
    };
    out.retain(|(n, _)| n.starts_with(current.as_str()) && n != current);
    out
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::cache::{cache_file, read_cache};

    fn names(v: &[(String, String)]) -> Vec<&str> {
        v.iter().map(|(n, _)| n.as_str()).collect()
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sc-compl-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).expect("mkdir");
        d
    }

    fn script(dir: &Path, name: &str, body: &str) {
        let p = dir.join(name);
        fs::write(&p, body).expect("write");
        fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    #[test]
    fn completes_from_help_and_caches_without_rerunning() {
        let bin = temp("bin");
        let cache = temp("cache");
        let counter = bin.join("count");
        // `mytool --help` lists subcommands; `mytool deploy --help` lists its own flags
        script(
            &bin,
            "mytool",
            &format!(
                "#!/bin/sh\necho x >> {}\ncase \"$1\" in\n  deploy) printf 'Usage: mytool deploy\\n\\nOptions:\\n      --force   skip checks\\n      --env <E>  target\\n' ;;\n  *) printf 'Commands:\\n  deploy  Ship it\\n  destroy  Tear down\\n  status  Show\\n\\nOptions:\\n  -q, --quiet  quiet\\n' ;;\nesac\n",
                counter.display()
            ),
        );
        let path = bin.to_str().expect("utf8");
        let w = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        let p = Policy::default();
        assert_eq!(
            names(&complete(&w(&["mytool", "de"]), path, &cache, &p)),
            ["deploy", "destroy"]
        );
        assert_eq!(
            names(&complete(&w(&["mytool", "-"]), path, &cache, &p)),
            ["-q", "--quiet"]
        );
        assert_eq!(
            names(&complete(
                &w(&["mytool", "deploy", "--f"]),
                path,
                &cache,
                &p
            )),
            ["--force"]
        );
        assert!(
            complete(&w(&["mytool", "deploy", "prod", ""]), path, &cache, &p).is_empty(),
            "positional ends subcommand completion"
        );
        assert!(complete(&w(&["mytool", "status", "x"]), path, &cache, &p).is_empty());
        let runs = fs::read_to_string(&counter)
            .expect("counter")
            .lines()
            .count();
        assert_eq!(
            runs, 3,
            "one run each for `mytool`, `mytool deploy` and `mytool status`"
        );
        // a second round answers from the cache
        let _ = complete(&w(&["mytool", "d"]), path, &cache, &p);
        assert_eq!(
            fs::read_to_string(&counter)
                .expect("counter")
                .lines()
                .count(),
            runs
        );
        for d in [bin, cache] {
            let _ = fs::remove_dir_all(d);
        }
    }

    #[test]
    fn refuses_unsafe_programs_and_stale_binaries() {
        let bin = temp("safe-bin");
        let cache = temp("safe-cache");
        let marker = bin.join("ran");
        let body = format!(
            "#!/bin/sh\ntouch {}\nprintf 'Commands:\\n  go  Go\\n'\n",
            marker.display()
        );
        script(&bin, "rm", &body); // on the never-run list
        script(&bin, "skipme", &body);
        script(&bin, "okay", &body);
        let path = bin.to_str().expect("utf8");
        let w = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(complete(&w(&["rm", ""]), path, &cache, &Policy::default()).is_empty());
        let policy = Policy {
            skip: vec!["skipme".into()],
        };
        assert!(complete(&w(&["skipme", ""]), path, &cache, &policy).is_empty());
        assert!(!marker.exists(), "neither program was started");
        // relative and explicit paths are not resolved
        assert!(resolve("./okay", path).is_none() && resolve("../okay", path).is_none());
        // a world-writable directory is not trusted
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o777)).expect("chmod");
        assert!(complete(&w(&["okay", ""]), path, &cache, &Policy::default()).is_empty());
        fs::set_permissions(&bin, fs::Permissions::from_mode(0o755)).expect("chmod");
        assert_eq!(
            names(&complete(
                &w(&["okay", ""]),
                path,
                &cache,
                &Policy::default()
            )),
            ["go"]
        );
        // a changed binary invalidates its cache entry
        let file = cache_file(&cache, &["okay".to_string()]);
        let mtime = fs::metadata(bin.join("okay")).expect("meta").mtime();
        assert!(read_cache(&file, mtime).is_some() && read_cache(&file, mtime + 1).is_none());
        for d in [bin, cache] {
            let _ = fs::remove_dir_all(d);
        }
    }

    #[test]
    fn a_hanging_help_is_cut_off() {
        let bin = temp("hang-bin");
        let cache = temp("hang-cache");
        script(&bin, "slow", "#!/bin/sh\nsleep 5\n");
        let start = Instant::now();
        let out = complete(
            &["slow".to_string(), "".to_string()],
            bin.to_str().expect("utf8"),
            &cache,
            &Policy::default(),
        );
        assert!(
            out.is_empty() && start.elapsed() < Duration::from_millis(2500),
            "{:?}",
            start.elapsed()
        );
        for d in [bin, cache] {
            let _ = fs::remove_dir_all(d);
        }
    }
}
