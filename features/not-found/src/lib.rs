//! `shellcraft suggest`: closest command names in PATH (used by command-not-found and `fix`).

use std::collections::BTreeSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use anyhow::{bail, Result};

/// Edit distance where swapping two neighbouring letters costs 1 (the common typo).
pub fn distance(a: &str, b: &str) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, x) in d[0].iter_mut().enumerate() {
        *x = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let sub = usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = (d[i - 1][j] + 1)
                .min(d[i][j - 1] + 1)
                .min(d[i - 1][j - 1] + sub);
            if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                d[i][j] = d[i][j].min(d[i - 2][j - 2] + 1);
            }
        }
    }
    d[a.len()][b.len()]
}

/// Up to `max` executable names within edit distance `len/3` (at least 1), closest first.
pub fn closest(word: &str, path: &str, max: usize) -> Vec<String> {
    // One or two letters are within one edit of far too many commands to be a useful guess.
    if word.chars().count() < 3 {
        return Vec::new();
    }
    let limit = (word.chars().count() / 3).max(1);
    let mut names = BTreeSet::new();
    for dir in path.split(':').filter(|d| !d.is_empty()) {
        let Ok(rd) = fs::read_dir(Path::new(dir)) else {
            continue;
        };
        for e in rd.flatten() {
            let executable = e
                .metadata()
                .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0);
            if let (true, Some(n)) = (executable, e.file_name().to_str()) {
                names.insert(n.to_string());
            }
        }
    }
    let mut hits: Vec<(usize, String)> = names
        .into_iter()
        .map(|n| (distance(word, &n), n))
        .filter(|(d, n)| *d > 0 && *d <= limit && n != word && n.chars().count() >= 2)
        .collect();
    hits.sort();
    hits.into_iter().take(max).map(|(_, n)| n).collect()
}

/// Plain mode prints one name per line; `pretty` prints the whole "not found" message to stderr.
/// Arguments of `shellcraft suggest`.
#[derive(clap::Args)]
pub struct SuggestArgs {
    pub word: String,
    /// Print the whole styled "command not found" message to stderr
    #[arg(long)]
    pub pretty: bool,
}

pub fn run(args: SuggestArgs) -> Result<()> {
    let SuggestArgs { word, pretty } = args;
    let word = word.as_str();
    use std::io::IsTerminal;
    let path = std::env::var("PATH").unwrap_or_default();
    let hits = closest(word, &path, 3);
    if !pretty {
        if hits.is_empty() {
            bail!("no suggestion");
        }
        for h in hits {
            println!("{h}");
        }
        return Ok(());
    }
    let color = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let (red, bold, dim, green, off) = if color {
        ("\x1b[31m", "\x1b[1m", "\x1b[2m", "\x1b[32m", "\x1b[0m")
    } else {
        ("", "", "", "", "")
    };
    eprintln!("{red}\u{2717}{off} command not found: {bold}{word}{off}");
    if !hits.is_empty() {
        let names: Vec<String> = hits.iter().map(|h| format!("{green}{h}{off}")).collect();
        eprintln!(
            "  {dim}did you mean{off} {}  {dim}\u{b7} type{off} fix {dim}to retype it{off}",
            names.join(&format!("{dim},{off} "))
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    #[test]
    fn edit_distance() {
        assert_eq!(distance("gti", "git"), 1);
        assert_eq!(distance("git", "git"), 0);
        assert_eq!(distance("", "ab"), 2);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(distance("ab", ""), 2);
    }

    #[test]
    fn finds_close_executables_only() {
        let dir = std::env::temp_dir().join(format!("sc-suggest-{}", std::process::id()));
        fs::create_dir_all(&dir).expect("mkdir");
        for (name, mode) in [
            ("git", 0o755),
            ("gist", 0o755),
            ("grep", 0o755),
            ("gitx", 0o644),
        ] {
            let f = dir.join(name);
            File::create(&f).expect("create");
            fs::set_permissions(&f, fs::Permissions::from_mode(mode)).expect("chmod");
        }
        let p = dir.to_str().expect("utf8");
        assert_eq!(
            closest("gti", p, 3),
            vec!["git"; 1],
            "gist is distance 3, gitx not executable"
        );
        assert_eq!(closest("gitt", p, 3), vec!["gist", "git"]);
        assert!(closest("zzzzzz", p, 3).is_empty());
        assert!(closest("git", p, 3).iter().all(|n| n != "git"));
        assert!(
            closest("gi", p, 3).is_empty(),
            "two letters are too short to guess"
        );
        assert!(closest("\u{e7}", p, 3).is_empty());
        let _ = fs::remove_dir_all(&dir);
    }
}
