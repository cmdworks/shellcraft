//! Git plugin: branch, dirty marker and repository state, read from `.git` without a `git`
//! subprocess.

use std::fs;
use std::path::{Path, PathBuf};

use shellcraft_plugin::{Ctx, Plugin, Segment, Tone};

pub struct Git;

impl Plugin for Git {
    fn name(&self) -> &'static str {
        "git"
    }

    fn describe(&self) -> &'static str {
        "branch, dirty marker and merge/rebase state, no git subprocess"
    }

    fn segments(&self, ctx: &Ctx) -> Vec<Segment> {
        let Some(branch) = git_branch(ctx.cwd) else {
            return Vec::new();
        };
        // options (`[plugin.git.options]`, or set by a theme): `prefix` before the branch, `dirty`
        // and `clean` markers (default: `*` when dirty, nothing when clean)
        let opt = |k: &str, default: &str| (ctx.opt)(k).unwrap_or_else(|| default.to_string());
        let prefix = opt("prefix", "");
        let mut seg = Segment::new(format!("{prefix}{branch}"), Tone::Git);
        if let Some(state) = git_state(ctx.cwd) {
            seg = seg.push(format!("|{state}"), Tone::Warn);
        }
        match git_dirty(ctx.cwd) {
            Some(true) => {
                let dirty = opt("dirty", "*");
                if !dirty.is_empty() {
                    let tone = if (ctx.opt)("dirty").is_some() {
                        Tone::Err
                    } else {
                        Tone::Warn
                    };
                    seg = seg.push(dirty, tone);
                }
            }
            Some(false) => {
                let clean = opt("clean", "");
                if !clean.is_empty() {
                    seg = seg.push(clean, Tone::Ok);
                }
            }
            None => {}
        }
        vec![seg]
    }
}

/// Work tree root and git dir of the repository containing `dir`.
pub fn find_repo(dir: &Path) -> Option<(PathBuf, PathBuf)> {
    for d in dir.ancestors() {
        let dot = d.join(".git");
        if dot.is_dir() {
            return Some((d.to_path_buf(), dot));
        }
        if dot.is_file() {
            // worktree or submodule: `gitdir: <path>`
            let text = fs::read_to_string(&dot).ok()?;
            let p = Path::new(text.strip_prefix("gitdir:")?.trim());
            let gitdir = if p.is_absolute() {
                p.to_path_buf()
            } else {
                d.join(p)
            };
            return Some((d.to_path_buf(), gitdir));
        }
    }
    None
}

/// Branch name (or short hash when detached) of the repository containing `dir`.
pub fn git_branch(dir: &Path) -> Option<String> {
    let (_, gitdir) = find_repo(dir)?;
    let head = fs::read_to_string(gitdir.join("HEAD")).ok()?;
    let head = head.trim();
    Some(match head.strip_prefix("ref: refs/heads/") {
        Some(b) => b.to_string(),
        None => head.chars().take(7).collect(),
    })
}

/// Largest index scanned; bigger repositories get no marker rather than a slow prompt.
const MAX_INDEX_ENTRIES: usize = 20_000;

/// Whether a tracked file differs from the index by size or mtime (`git status` without the
/// subprocess). `None` when it cannot tell: unsupported index version, huge repo, or no index.
/// Untracked files and staged-only changes are not detected.
pub fn git_dirty(dir: &Path) -> Option<bool> {
    use std::os::unix::fs::MetadataExt;
    let (work, gitdir) = find_repo(dir)?;
    let data = fs::read(gitdir.join("index")).ok()?;
    let u32_at = |o: usize| {
        data.get(o..o + 4)
            .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    if data.get(..4)? != b"DIRC" || !matches!(u32_at(4)?, 2 | 3) {
        return None; // v4 compresses paths
    }
    let count = u32_at(8)? as usize;
    if count > MAX_INDEX_ENTRIES {
        return None;
    }
    let mut off = 12;
    for _ in 0..count {
        let (mtime_s, mtime_ns, size) = (u32_at(off + 8)?, u32_at(off + 12)?, u32_at(off + 36)?);
        let flags = u16::from_be_bytes([*data.get(off + 60)?, *data.get(off + 61)?]);
        let ext = u32_at(4)? == 3 && flags & 0x4000 != 0;
        let name_off = if ext { 64 } else { 62 };
        let len = data.get(off + name_off..)?.iter().position(|&b| b == 0)?;
        let rel = std::str::from_utf8(&data[off + name_off..off + name_off + len]).ok()?;
        // entries are NUL-padded so each starts on a multiple of 8 bytes from the previous
        off += (name_off + len + 8) & !7;
        match fs::symlink_metadata(work.join(rel)) {
            Ok(m) => {
                if m.size() as u32 != size
                    || m.mtime() as u32 != mtime_s
                    || m.mtime_nsec() as u32 != mtime_ns
                {
                    return Some(true);
                }
            }
            Err(_) => return Some(true), // deleted
        }
    }
    Some(false)
}

/// `merge`, `rebase`, `cherry-pick`, `revert` or `bisect` while one is in progress.
pub fn git_state(dir: &Path) -> Option<&'static str> {
    let (_, gitdir) = find_repo(dir)?;
    let has = |n: &str| gitdir.join(n).exists();
    if has("rebase-merge") || has("rebase-apply") {
        Some("rebase")
    } else if has("MERGE_HEAD") {
        Some("merge")
    } else if has("CHERRY_PICK_HEAD") {
        Some("cherry-pick")
    } else if has("REVERT_HEAD") {
        Some("revert")
    } else if has("BISECT_LOG") {
        Some("bisect")
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dirty_marker_follows_tracked_changes() {
        let root = std::env::temp_dir().join(format!("sc-dirty-{}", std::process::id()));
        fs::create_dir_all(&root).expect("mkdir");
        let git = |args: &[&str]| {
            let ok = std::process::Command::new("git")
                .args(["-c", "user.name=t", "-c", "user.email=t@t", "-C"])
                .arg(&root)
                .args(args)
                .output()
                .expect("git")
                .status
                .success();
            assert!(ok, "git {args:?}");
        };
        git(&["init", "-q", "."]);
        fs::write(root.join("a.txt"), "one").expect("write");
        fs::create_dir_all(root.join("d")).expect("mkdir");
        fs::write(root.join("d/b.txt"), "two").expect("write");
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "x"]);
        assert_eq!(git_dirty(&root), Some(false));
        fs::write(root.join("d/b.txt"), "changed!").expect("write");
        assert_eq!(git_dirty(&root.join("d")), Some(true));
        git(&["checkout", "-q", "--", "."]);
        assert_eq!(git_dirty(&root), Some(false));
        fs::remove_file(root.join("a.txt")).expect("rm");
        assert_eq!(git_dirty(&root), Some(true));
        assert_eq!(
            git_dirty(&std::env::temp_dir().join("sc-no-repo-here")),
            None
        );
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn git_branch_from_head_files() {
        let root = std::env::temp_dir().join(format!("sc-prompt-{}", std::process::id()));
        let repo = root.join("repo");
        fs::create_dir_all(repo.join(".git")).expect("mkdir");
        fs::create_dir_all(repo.join("src/deep")).expect("mkdir");
        fs::write(repo.join(".git/HEAD"), "ref: refs/heads/feature/x\n").expect("write");
        assert_eq!(
            git_branch(&repo.join("src/deep")).as_deref(),
            Some("feature/x")
        );
        fs::write(repo.join(".git/HEAD"), "0123456789abcdef\n").expect("write");
        assert_eq!(git_branch(&repo).as_deref(), Some("0123456"));
        // worktree style `.git` file
        let wt = root.join("wt");
        fs::create_dir_all(&wt).expect("mkdir");
        let gitdir = root.join("gitdir");
        fs::create_dir_all(&gitdir).expect("mkdir");
        fs::write(gitdir.join("HEAD"), "ref: refs/heads/wt-branch\n").expect("write");
        fs::write(wt.join(".git"), format!("gitdir: {}\n", gitdir.display())).expect("write");
        assert_eq!(git_branch(&wt).as_deref(), Some("wt-branch"));
        assert_eq!(git_branch(&root.join("none")), None);
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn repository_state_files() {
        let root = std::env::temp_dir().join(format!("sc-gitstate-{}", std::process::id()));
        fs::create_dir_all(root.join(".git")).expect("mkdir");
        assert_eq!(git_state(&root), None);
        fs::write(root.join(".git/MERGE_HEAD"), "x").expect("write");
        assert_eq!(git_state(&root), Some("merge"));
        fs::create_dir_all(root.join(".git/rebase-merge")).expect("mkdir");
        assert_eq!(git_state(&root), Some("rebase"), "rebase wins over merge");
        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn segment_combines_branch_state_and_dirty() {
        let root = std::env::temp_dir().join(format!("sc-gitseg-{}", std::process::id()));
        fs::create_dir_all(root.join(".git")).expect("mkdir");
        fs::write(root.join(".git/HEAD"), "ref: refs/heads/main\n").expect("write");
        fs::write(root.join(".git/CHERRY_PICK_HEAD"), "x").expect("write");
        let env = |_: &str| None;
        let ctx = Ctx {
            cwd: &root,
            home: None,
            cache_dir: None,
            env: &env,
            opt: &env,
        };
        let segs = Git.segments(&ctx);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].parts[0], ("main".to_string(), Tone::Git));
        assert_eq!(segs[0].parts[1], ("|cherry-pick".to_string(), Tone::Warn));
        assert!(Git
            .segments(&Ctx {
                cwd: &std::env::temp_dir().join("sc-nogit-here"),
                home: None,
                cache_dir: None,
                env: &env,
                opt: &env,
            })
            .is_empty());
        let _ = fs::remove_dir_all(&root);
    }
}
