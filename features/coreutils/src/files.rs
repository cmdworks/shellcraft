//! `shellcraft files`: list files or directories under a path, honouring `.gitignore`
//! (fd/find replacement for the pickers).

use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::Result;
use ignore::WalkBuilder;

/// Relative paths under `root` (files, or directories with `dirs`), depth-first, capped at `max`.
pub fn list(root: &Path, dirs: bool, hidden: bool, max: usize) -> Vec<String> {
    let mut out = Vec::new();
    let walker = WalkBuilder::new(root)
        .hidden(!hidden)
        .filter_entry(|e| e.file_name() != ".git")
        .build();
    for entry in walker.flatten() {
        let Some(ft) = entry.file_type() else {
            continue;
        };
        if entry.depth() == 0 || ft.is_dir() != dirs {
            continue;
        }
        if let Ok(rel) = entry.path().strip_prefix(root) {
            out.push(rel.to_string_lossy().into_owned());
            if out.len() >= max {
                break;
            }
        }
    }
    out
}

/// Arguments of `shellcraft files`.
#[derive(clap::Args)]
pub struct FilesArgs {
    #[arg(default_value = ".")]
    pub path: PathBuf,
    /// List directories instead of files
    #[arg(long)]
    pub dirs: bool,
    /// Include hidden entries
    #[arg(long)]
    pub hidden: bool,
    /// Stop after this many entries
    #[arg(long, default_value_t = 100_000)]
    pub max: usize,
}

pub fn run(args: FilesArgs) -> Result<()> {
    let (root, dirs, hidden, max) = (args.path.as_path(), args.dirs, args.hidden, args.max);
    let mut out = io::stdout().lock();
    for p in list(root, dirs, hidden, max) {
        writeln!(out, "{p}")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn honours_gitignore_and_hidden_and_dirs() {
        let root = std::env::temp_dir().join(format!("sc-files-{}", std::process::id()));
        fs::create_dir_all(root.join("src")).expect("mkdir");
        fs::create_dir_all(root.join("target")).expect("mkdir");
        fs::create_dir_all(root.join(".git")).expect("mkdir");
        fs::write(root.join(".gitignore"), "target/\n").expect("write");
        fs::write(root.join("src/a.rs"), "").expect("write");
        fs::write(root.join("target/x.o"), "").expect("write");
        fs::write(root.join(".hidden"), "").expect("write");
        fs::write(root.join(".git/HEAD"), "").expect("write");
        let mut files = list(&root, false, false, 100);
        files.sort();
        assert_eq!(
            files,
            vec!["src/a.rs"],
            "ignored, hidden and .git are skipped"
        );
        let mut all = list(&root, false, true, 100);
        all.sort();
        assert_eq!(all, vec![".gitignore", ".hidden", "src/a.rs"]);
        assert_eq!(list(&root, true, false, 100), vec!["src"]);
        assert_eq!(list(&root, false, true, 1).len(), 1);
        let _ = fs::remove_dir_all(&root);
    }
}
