//! Versions plugin: shows the runtime of the project you are in (node, bun, deno, python, go,
//! rust, ruby).
//!
//! The prompt never runs a tool. It shows the active version from a small cache
//! (`versions.tsv`); when an entry is missing or older than 10 minutes it starts
//! `shellcraft plugin refresh versions <name> <dir>` in the background and shows the version
//! the project declares (`.nvmrc`, `go.mod`, ...) until the answer is in.

mod cache;
mod ecosystem;

use cache::{key, load_cache, spawn_refresh, store_cache, RETRY_SECS};
use ecosystem::{declared, eco, project_dir, satisfies, ECOS};
use shellcraft_core::proc::now_secs as now;
use shellcraft_plugin::{Ctx, Plugin, Segment, Tone};

pub use cache::{refresh, TTL_SECS};

pub struct Versions;

impl Plugin for Versions {
    fn name(&self) -> &'static str {
        "versions"
    }

    fn describe(&self) -> &'static str {
        "node, bun, deno, python, go, rust, ruby versions for the current project (option: ecosystems)"
    }

    fn segments(&self, ctx: &Ctx) -> Vec<Segment> {
        let wanted = (ctx.opt)("ecosystems");
        let background = (ctx.env)("SHELLCRAFT_NO_BG").is_none();
        let mut cache = ctx.cache_dir.map(load_cache);
        let mut dirty = false;
        let mut out = Vec::new();
        let bun_dir = project_dir(ctx.cwd, eco("bun").map_or(&[], |b| b.markers));
        for e in ECOS {
            if wanted.as_deref().is_some_and(|w| {
                !w.split(',')
                    .any(|n| n.trim() == e.name || n.trim() == e.label)
            }) {
                continue;
            }
            let Some(dir) = project_dir(ctx.cwd, e.markers) else {
                continue;
            };
            // bun projects have a package.json too; only show node there when it is asked for.
            if e.name == "node"
                && bun_dir.as_deref() == Some(dir.as_path())
                && declared(e, &dir).is_none()
            {
                continue;
            }
            let want = declared(e, &dir);
            let k = key(e, &dir);
            let entry = cache.as_ref().and_then(|c| c.get(&k)).cloned();
            let stale = entry
                .as_ref()
                .map_or(true, |(_, ts)| now().saturating_sub(*ts) >= TTL_SECS);
            if stale && background {
                if let Some(c) = cache.as_mut() {
                    // Retry only after RETRY_SECS if the refresh never reports back.
                    let old = entry.as_ref().map_or(String::new(), |(v, _)| v.clone());
                    c.insert(
                        k.clone(),
                        (old, now().saturating_sub(TTL_SECS - RETRY_SECS)),
                    );
                    dirty = true;
                }
                spawn_refresh(e.name, &dir);
            }
            let active = entry.map(|(v, _)| v).filter(|v| !v.is_empty());
            let seg = match (active.as_deref(), want) {
                (Some("-"), _) => continue, // tool not installed
                (Some(v), Some(w)) if !satisfies(&w, v) => {
                    Segment::new(format!("{} {v}", e.label), Tone::Version)
                        .push(format!(" (wants {w})"), Tone::Warn)
                }
                (Some(v), _) => Segment::new(format!("{} {v}", e.label), Tone::Version),
                (None, Some(w)) => Segment::new(format!("{} {w}", e.label), Tone::Dim),
                (None, None) => Segment::new(e.label, Tone::Dim),
            };
            out.push(seg);
        }
        if dirty {
            if let (Some(dir), Some(c)) = (ctx.cache_dir, cache.as_ref()) {
                store_cache(dir, c);
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    use super::cache::Cache;
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sc-ver-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).expect("mkdir");
        d
    }

    fn ctx<'a>(
        cwd: &'a Path,
        cache: &'a Path,
        env: &'a dyn Fn(&str) -> Option<String>,
        opt: &'a dyn Fn(&str) -> Option<String>,
    ) -> Ctx<'a> {
        Ctx {
            cwd,
            home: None,
            cache_dir: Some(cache),
            env,
            opt,
        }
    }

    #[test]
    fn segments_show_active_declared_and_mismatch() {
        let proj = temp("seg-proj");
        let cache = temp("seg-cache");
        fs::write(proj.join(".nvmrc"), "18\n").expect("write");
        fs::write(proj.join("Cargo.toml"), "[package]\nname = \"x\"\n").expect("write");
        let env = |k: &str| (k == "SHELLCRAFT_NO_BG").then(|| "1".to_string());
        let none = |_: &str| None;
        // nothing cached yet: declared (dim) or just the label
        let segs = Versions.segments(&ctx(&proj, &cache, &env, &none));
        let text: Vec<String> = segs
            .iter()
            .map(|s| s.parts.iter().map(|p| p.0.clone()).collect())
            .collect();
        assert_eq!(text, vec!["node 18", "rs"]);
        // cached active versions: a match, a mismatch and a missing tool
        let mut c = Cache::new();
        c.insert(format!("node@{}", proj.display()), ("20.1.0".into(), now()));
        c.insert(format!("rust@{}", proj.display()), ("-".into(), now()));
        store_cache(&cache, &c);
        let segs = Versions.segments(&ctx(&proj, &cache, &env, &none));
        assert_eq!(segs.len(), 1, "missing rust is hidden");
        assert_eq!(segs[0].parts[0], ("node 20.1.0".to_string(), Tone::Version));
        assert_eq!(segs[0].parts[1], (" (wants 18)".to_string(), Tone::Warn));
        // option filters ecosystems
        let only_rust = |k: &str| (k == "ecosystems").then(|| "rust".to_string());
        assert!(Versions
            .segments(&ctx(&proj, &cache, &env, &only_rust))
            .is_empty());
        // bun projects do not also show node
        fs::remove_file(proj.join(".nvmrc")).expect("rm");
        fs::write(proj.join("package.json"), "{}").expect("write");
        fs::write(proj.join("bun.lockb"), "").expect("write");
        let labels: Vec<String> = Versions
            .segments(&ctx(&proj, &cache, &env, &none))
            .iter()
            .map(|s| s.parts[0].0.clone())
            .collect();
        assert_eq!(labels, vec!["bun"]);
        for d in [proj, cache] {
            let _ = fs::remove_dir_all(d);
        }
    }

    #[test]
    fn stale_entries_are_marked_for_retry_without_spawning_in_tests() {
        let proj = temp("stale-proj");
        let cache = temp("stale-cache");
        fs::write(proj.join("go.mod"), "go 1.22\n").expect("write");
        let env = |k: &str| (k == "SHELLCRAFT_NO_BG").then(|| "1".to_string());
        let none = |_: &str| None;
        let mut c = Cache::new();
        c.insert(format!("go@{}", proj.display()), ("1.21.0".into(), 0));
        store_cache(&cache, &c);
        let segs = Versions.segments(&ctx(&proj, &cache, &env, &none));
        assert_eq!(
            segs[0].parts[0].0, "go 1.21.0",
            "stale value is still shown"
        );
        let _ = fs::remove_dir_all(&proj);
        let _ = fs::remove_dir_all(&cache);
    }
}
